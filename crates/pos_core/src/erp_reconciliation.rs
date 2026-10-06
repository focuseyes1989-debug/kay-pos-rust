use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::{PgConnection, PgPool};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ReconciliationConfig {
    pub url: String,
    pub api_token: Option<String>,
    pub provider: String,
    pub interval: Duration,
}

impl ReconciliationConfig {
    pub fn from_env() -> Result<Option<Self>> {
        if !crate::ERP_ENABLED { return Ok(None); }
        let url = std::env::var("ZAY_POS_ERP_STOCK_SNAPSHOT_URL")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if url.is_empty() {
            return Ok(None);
        }
        validate_url(&url)?;
        let seconds = std::env::var("ZAY_POS_ERP_RECONCILIATION_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "900".into())
            .parse::<u64>()
            .context("ZAY_POS_ERP_RECONCILIATION_INTERVAL_SECONDS must be a number")?;
        anyhow::ensure!(
            (60..=86400).contains(&seconds),
            "Reconciliation interval must be between 60 and 86400 seconds"
        );
        Ok(Some(Self {
            url,
            api_token: optional_env("ZAY_POS_ERP_API_TOKEN"),
            provider: optional_env("ZAY_POS_ERP_PROVIDER").unwrap_or_else(|| "default".into()),
            interval: Duration::from_secs(seconds),
        }))
    }
}

fn optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn validate_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("ERP stock snapshot URL must be valid")?;
    let local_http = url.scheme() == "http"
        && url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
    anyhow::ensure!(
        url.scheme() == "https" || local_http,
        "ERP stock snapshot URL must use HTTPS (HTTP is allowed only for localhost)"
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SnapshotPage {
    snapshot_id: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    items: Vec<SnapshotItem>,
}

#[derive(Debug, Deserialize)]
struct SnapshotItem {
    product_external_id: String,
    #[serde(default)]
    variant_external_id: Option<String>,
    #[serde(default)]
    location_external_id: Option<String>,
    quantity: f64,
}

pub async fn run_worker(pool: PgPool, config: ReconciliationConfig) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("ERP stock reconciliation client could not start: {error}");
            return;
        }
    };
    loop {
        match reconcile(&pool, &client, &config).await {
            Ok(()) => {
                let _ = crate::erp_operations::heartbeat(&pool, "stock-reconciliation", None).await;
            }
            Err(error) => {
                let message = format!("{error:#}");
                let _ =
                    crate::erp_operations::heartbeat(&pool, "stock-reconciliation", Some(&message))
                        .await;
                eprintln!("ERP stock reconciliation failed: {message}");
            }
        }
        tokio::time::sleep(config.interval).await;
    }
}

async fn reconcile(
    pool: &PgPool,
    client: &reqwest::Client,
    config: &ReconciliationConfig,
) -> Result<()> {
    let mut cursor: Option<String> = None;
    let mut run_id: Option<i64> = None;
    let mut snapshot_id: Option<String> = None;
    let result: Result<()> = async {
        for _ in 0..100 {
            let mut request = client.get(&config.url).query(&[("limit", "500")]);
            if let Some(value) = &cursor {
                request = request.query(&[("cursor", value)]);
            }
            if let Some(token) = &config.api_token {
                request = request.bearer_auth(token);
            }
            let response = request
                .send()
                .await
                .context("ERP stock snapshot request failed")?;
            anyhow::ensure!(
                response.status().is_success(),
                "ERP stock endpoint returned {}",
                response.status()
            );
            if let Some(length) = response.content_length() {
                anyhow::ensure!(
                    length <= 10 * 1024 * 1024,
                    "ERP stock response is too large"
                );
            }
            let bytes = response.bytes().await?;
            anyhow::ensure!(
                bytes.len() <= 10 * 1024 * 1024,
                "ERP stock response is too large"
            );
            let page: SnapshotPage =
                serde_json::from_slice(&bytes).context("ERP stock response was invalid")?;
            validate_page(&page, cursor.is_none())?;
            if let Some(expected) = &snapshot_id {
                anyhow::ensure!(
                    expected == &page.snapshot_id,
                    "ERP stock snapshot_id changed between pages"
                );
            } else {
                snapshot_id = Some(page.snapshot_id.clone());
            }
            let id = match run_id {
                Some(id) => id,
                None => start_run(pool, &config.provider, &page.snapshot_id).await?,
            };
            run_id = Some(id);
            apply_page(pool, &config.provider, id, &page).await?;
            cursor = page.cursor.clone();
            if !page.has_more {
                finish_run(pool, id).await?;
                return Ok(());
            }
            anyhow::ensure!(
                cursor.is_some(),
                "ERP stock response has_more=true without a cursor"
            );
        }
        anyhow::bail!("ERP stock reconciliation exceeded 100 pages")
    }
    .await;
    if let (Some(id), Err(error)) = (run_id, &result) {
        let message: String = format!("{error:#}").chars().take(2000).collect();
        let _ = sqlx::query("UPDATE rust_erp_reconciliation_runs SET status='failed',last_error=$2,completed_at=CURRENT_TIMESTAMP WHERE id=$1")
            .bind(id).bind(message).execute(pool).await;
    }
    result
}

fn validate_page(page: &SnapshotPage, first: bool) -> Result<()> {
    anyhow::ensure!(
        !page.snapshot_id.trim().is_empty() && page.snapshot_id.len() <= 200,
        "Invalid ERP stock snapshot_id"
    );
    anyhow::ensure!(
        page.items.len() <= 1000,
        "ERP stock page exceeds 1000 items"
    );
    let mut keys = std::collections::HashSet::new();
    for item in &page.items {
        anyhow::ensure!(
            !item.product_external_id.trim().is_empty() && item.product_external_id.len() <= 200,
            "Invalid ERP product external ID"
        );
        anyhow::ensure!(
            item.quantity.is_finite() && item.quantity >= 0.0,
            "Invalid ERP stock quantity"
        );
        let key = item_key(item);
        anyhow::ensure!(keys.insert(key), "Duplicate stock identity in ERP page");
    }
    if !first {
        anyhow::ensure!(
            page.cursor.is_some() || !page.has_more,
            "Paged ERP stock response is missing a cursor"
        );
    }
    Ok(())
}

async fn start_run(pool: &PgPool, provider: &str, snapshot: &str) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let id: i64 = sqlx::query_scalar("INSERT INTO rust_erp_reconciliation_runs(provider,snapshot_id,status) VALUES($1,$2,'running') ON CONFLICT(provider,snapshot_id) DO UPDATE SET status='running',compared_count=0,difference_count=0,unmapped_count=0,cursor_value=NULL,last_error=NULL,started_at=CURRENT_TIMESTAMP,completed_at=NULL RETURNING id")
        .bind(provider).bind(snapshot).fetch_one(&mut *tx).await
        .context("ERP reconciliation schema unavailable; apply migrations/012_erp_stock_reconciliation.sql")?;
    sqlx::query("DELETE FROM rust_erp_stock_discrepancies WHERE run_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(id)
}

async fn apply_page(pool: &PgPool, provider: &str, run: i64, page: &SnapshotPage) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('rust.erp.reconcile:' || $1,0))")
        .bind(provider)
        .execute(&mut *tx)
        .await?;
    for item in &page.items {
        let product = mapped_id(&mut tx, provider, "product", &item.product_external_id).await?;
        let variant = match &item.variant_external_id {
            Some(id) => mapped_id(&mut tx, provider, "product_variant", id).await?,
            None => None,
        };
        let location = match &item.location_external_id {
            Some(id) => mapped_id(&mut tx, provider, "location", id).await?,
            None => None,
        };
        let required_mapping_missing = product.is_none()
            || (item.variant_external_id.is_some() && variant.is_none())
            || (item.location_external_id.is_some() && location.is_none());
        let pos = if required_mapping_missing {
            None
        } else {
            Some(local_quantity(&mut tx, product.unwrap(), variant, location).await?)
        };
        let difference = pos.map(|value| value - item.quantity);
        let status = if pos.is_none() {
            "unmapped"
        } else if difference.is_some_and(|value| value.abs() <= 0.000_001) {
            "matched"
        } else {
            "difference"
        };
        sqlx::query("INSERT INTO rust_erp_stock_discrepancies(run_id,item_key,product_external_id,variant_external_id,location_external_id,local_product_id,local_variant_id,local_location_id,erp_quantity,pos_quantity,difference,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(run_id,item_key) DO UPDATE SET local_product_id=EXCLUDED.local_product_id,local_variant_id=EXCLUDED.local_variant_id,local_location_id=EXCLUDED.local_location_id,erp_quantity=EXCLUDED.erp_quantity,pos_quantity=EXCLUDED.pos_quantity,difference=EXCLUDED.difference,status=EXCLUDED.status,created_at=CURRENT_TIMESTAMP")
            .bind(run).bind(item_key(item)).bind(&item.product_external_id).bind(&item.variant_external_id).bind(&item.location_external_id).bind(product).bind(variant).bind(location).bind(item.quantity).bind(pos).bind(difference).bind(status).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE rust_erp_reconciliation_runs SET cursor_value=$2 WHERE id=$1")
        .bind(run)
        .bind(&page.cursor)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn local_quantity(
    conn: &mut PgConnection,
    product: i32,
    variant: Option<i32>,
    location: Option<i32>,
) -> Result<f64> {
    match (variant, location) {
        (Some(variant), Some(location)) => Ok(sqlx::query_scalar("SELECT COALESCE(SUM(b.quantity),0)::float8 FROM variant_stock_batches b JOIN locations l ON LOWER(TRIM(l.name))=LOWER(TRIM(b.location)) WHERE b.product_id=$1 AND b.variant_id=$2 AND l.id=$3").bind(product).bind(variant).bind(location).fetch_one(&mut *conn).await?),
        (Some(variant), None) => Ok(sqlx::query_scalar("SELECT COALESCE(stock,0)::float8 FROM product_variants WHERE product_id=$1 AND id=$2").bind(product).bind(variant).fetch_one(&mut *conn).await?),
        (None, Some(location)) => Ok(sqlx::query_scalar("SELECT COALESCE(SUM(pl.quantity),0)::float8 FROM product_locations pl JOIN locations l ON LOWER(TRIM(l.name))=LOWER(TRIM(pl.location)) WHERE pl.product_id=$1 AND l.id=$2").bind(product).bind(location).fetch_one(&mut *conn).await?),
        (None, None) => Ok(sqlx::query_scalar("SELECT COALESCE(stock,0)::float8 FROM products WHERE id=$1").bind(product).fetch_one(&mut *conn).await?),
    }
}

async fn mapped_id(
    conn: &mut PgConnection,
    provider: &str,
    kind: &str,
    external: &str,
) -> Result<Option<i32>> {
    Ok(sqlx::query_scalar("SELECT local_id FROM rust_erp_entity_map WHERE provider=$1 AND entity_type=$2 AND external_id=$3").bind(provider).bind(kind).bind(external).fetch_optional(&mut *conn).await?)
}

async fn finish_run(pool: &PgPool, run: i64) -> Result<()> {
    sqlx::query("UPDATE rust_erp_reconciliation_runs r SET status='complete',compared_count=s.total,difference_count=s.differences,unmapped_count=s.unmapped,completed_at=CURRENT_TIMESTAMP,last_error=NULL FROM (SELECT COUNT(*)::int total,COUNT(*) FILTER(WHERE status='difference')::int differences,COUNT(*) FILTER(WHERE status='unmapped')::int unmapped FROM rust_erp_stock_discrepancies WHERE run_id=$1) s WHERE r.id=$1")
        .bind(run).execute(pool).await?;
    Ok(())
}

fn item_key(item: &SnapshotItem) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(format!(
        "{}\0{}\0{}",
        item.product_external_id,
        item.variant_external_id.as_deref().unwrap_or(""),
        item.location_external_id.as_deref().unwrap_or("")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_identity_is_stable_and_specific() {
        let a = SnapshotItem {
            product_external_id: "p".into(),
            variant_external_id: None,
            location_external_id: Some("a".into()),
            quantity: 1.0,
        };
        let b = SnapshotItem {
            location_external_id: Some("b".into()),
            ..SnapshotItem {
                product_external_id: "p".into(),
                variant_external_id: None,
                location_external_id: None,
                quantity: 1.0,
            }
        };
        assert_eq!(item_key(&a), item_key(&a));
        assert_ne!(item_key(&a), item_key(&b));
    }
}
