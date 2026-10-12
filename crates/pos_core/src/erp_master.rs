use crate::storage as sqlx;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct MasterSyncConfig {
    pub url: String,
    pub api_token: Option<String>,
    pub provider: String,
    pub interval: Duration,
}

impl MasterSyncConfig {
    pub fn from_env() -> Result<Option<Self>> {
        if !crate::ERP_ENABLED { return Ok(None); }
        let url = std::env::var("ZAY_POS_ERP_MASTER_DATA_URL")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if url.is_empty() {
            return Ok(None);
        }
        validate_url(&url)?;
        let seconds = std::env::var("ZAY_POS_ERP_MASTER_SYNC_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "300".into())
            .parse::<u64>()
            .context("ZAY_POS_ERP_MASTER_SYNC_INTERVAL_SECONDS must be a number")?;
        anyhow::ensure!(
            (30..=86400).contains(&seconds),
            "Master sync interval must be between 30 and 86400 seconds"
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
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("ERP master-data URL must be valid")?;
    let local_http = url.scheme() == "http"
        && url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
    anyhow::ensure!(
        url.scheme() == "https" || local_http,
        "ERP master-data URL must use HTTPS (HTTP is allowed only for localhost)"
    );
    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MasterPage {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub categories: Vec<MasterCategory>,
    #[serde(default)]
    pub products: Vec<MasterProduct>,
    #[serde(default)]
    pub customers: Vec<MasterCustomer>,
    #[serde(default)]
    pub suppliers: Vec<MasterSupplier>,
    #[serde(default)]
    pub locations: Vec<MasterLocation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MasterCategory {
    pub external_id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "active")]
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MasterProduct {
    pub external_id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub name: String,
    #[serde(default)]
    pub category_external_id: Option<String>,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    pub price: f64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub low_stock: f64,
    #[serde(default = "each")]
    pub sold_by: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub variants: Vec<MasterVariant>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MasterVariant {
    pub external_id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    pub price: f64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub low_stock: i32,
    #[serde(default)]
    pub wholesale_min_qty: i32,
    #[serde(default)]
    pub wholesale_price: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MasterCustomer {
    pub external_id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub name: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub remarks: String,
    #[serde(default)]
    pub credit_limit: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MasterSupplier {
    pub external_id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub name: String,
    #[serde(default)]
    pub company_name: String,
    #[serde(default)]
    pub contact_person: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub tax_number: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub payment_terms: String,
    #[serde(default)]
    pub bank_account: String,
    #[serde(default = "active")]
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MasterLocation {
    pub external_id: String,
    #[serde(default)]
    pub version: Option<String>,
    pub name: String,
}

fn active() -> bool {
    true
}
fn each() -> String {
    "Each".into()
}

pub async fn run_worker(pool: PgPool, config: MasterSyncConfig) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("ERP master sync client could not start: {error}");
            return;
        }
    };
    loop {
        match pull_all(&pool, &client, &config).await {
            Ok(()) => {
                let _ = crate::erp_operations::heartbeat(&pool, "master-data", None).await;
            }
            Err(error) => {
                let message = truncate(&format!("{error:#}"), 2000);
                let _ = record_error(&pool, &config.provider, &message).await;
                let _ =
                    crate::erp_operations::heartbeat(&pool, "master-data", Some(&message)).await;
                eprintln!("ERP master sync failed: {message}");
            }
        }
        tokio::time::sleep(config.interval).await;
    }
}

async fn pull_all(
    pool: &PgPool,
    client: &reqwest::Client,
    config: &MasterSyncConfig,
) -> Result<()> {
    let mut cursor: Option<String> = sqlx::query_scalar(
        "SELECT cursor_value FROM rust_erp_sync_state WHERE provider=$1 AND stream='master-data'",
    )
    .bind(&config.provider)
    .fetch_optional(pool)
    .await
    .context("ERP master mapping unavailable; apply migrations/011_erp_master_data.sql")?
    .flatten();

    for _ in 0..100 {
        let mut request = client.get(&config.url).query(&[("limit", "250")]);
        if let Some(value) = &cursor {
            request = request.query(&[("cursor", value)]);
        }
        if let Some(token) = &config.api_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .context("ERP master-data request failed")?;
        let status = response.status();
        anyhow::ensure!(
            status.is_success(),
            "ERP master-data endpoint returned {status}"
        );
        if let Some(length) = response.content_length() {
            anyhow::ensure!(
                length <= 10 * 1024 * 1024,
                "ERP master-data response is too large"
            );
        }
        let bytes = response
            .bytes()
            .await
            .context("Could not read ERP master-data response")?;
        anyhow::ensure!(
            bytes.len() <= 10 * 1024 * 1024,
            "ERP master-data response is too large"
        );
        let page: MasterPage =
            serde_json::from_slice(&bytes).context("ERP master-data response was invalid")?;
        validate_page(&page)?;
        apply_page(pool, &config.provider, &page).await?;
        cursor = page.cursor.clone();
        if !page.has_more {
            return Ok(());
        }
        anyhow::ensure!(
            cursor.is_some(),
            "ERP response has_more=true without a cursor"
        );
    }
    anyhow::bail!("ERP master sync exceeded 100 pages")
}

pub async fn apply_page(pool: &PgPool, provider: &str, page: &MasterPage) -> Result<()> {
    anyhow::ensure!(!provider.trim().is_empty(), "ERP provider is required");
    validate_page(page)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('rust.erp.master:' || $1,0))")
        .bind(provider)
        .execute(&mut *tx)
        .await?;

    for item in &page.categories {
        let hash = source_hash(item)?;
        if unchanged(&mut tx, provider, "category", &item.external_id, &hash).await? {
            continue;
        }
        let local = mapped_id(&mut tx, provider, "category", &item.external_id).await?;
        let id: i32 = if let Some(id) = local {
            sqlx::query_scalar("UPDATE categories SET name=$1,description=$2,status=$3,updated_at=CURRENT_TIMESTAMP WHERE id=$4 RETURNING id")
                .bind(item.name.trim()).bind(item.description.trim()).bind(if item.active { "active" } else { "inactive" }).bind(id).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar(
                "INSERT INTO categories(name,description,status) VALUES($1,$2,$3) RETURNING id",
            )
            .bind(item.name.trim())
            .bind(item.description.trim())
            .bind(if item.active { "active" } else { "inactive" })
            .fetch_one(&mut *tx)
            .await?
        };
        save_map(
            &mut tx,
            provider,
            "category",
            &item.external_id,
            id,
            item.version.as_deref(),
            &hash,
        )
        .await?;
    }

    for item in &page.products {
        let hash = source_hash(item)?;
        if unchanged(&mut tx, provider, "product", &item.external_id, &hash).await? {
            continue;
        }
        let category_id = match &item.category_external_id {
            Some(external) => Some(
                mapped_id(&mut tx, provider, "category", external)
                    .await?
                    .with_context(|| {
                        format!(
                            "Product {} refers to unknown ERP category {external}",
                            item.external_id
                        )
                    })?,
            ),
            None => None,
        };
        let category_name: Option<String> = match category_id {
            Some(id) => {
                sqlx::query_scalar("SELECT name FROM categories WHERE id=$1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
            }
            None => None,
        };
        let local = mapped_id(&mut tx, provider, "product", &item.external_id).await?;
        let id: i32 = if let Some(id) = local {
            sqlx::query_scalar("UPDATE products SET name=$1,category_id=$2,category=$3,sku=$4,barcode=$5,price=$6,cost=$7,low_stock=$8,sold_by=$9,description=$10,last_updated=CURRENT_TIMESTAMP WHERE id=$11 AND (LOWER(COALESCE(sold_by,'Each'))=LOWER($9) OR COALESCE(stock,0)=0) RETURNING id")
                .bind(item.name.trim()).bind(category_id).bind(category_name).bind(clean(&item.sku)).bind(clean(&item.barcode)).bind(item.price).bind(item.cost).bind(item.low_stock).bind(item.sold_by.trim()).bind(item.description.trim()).bind(id).fetch_optional(&mut *tx).await?
                .context("ERP cannot change a product type while the POS product has stock")?
        } else {
            sqlx::query_scalar("INSERT INTO products(name,category_id,category,sku,barcode,price,cost,low_stock,sold_by,description,stock) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,0) RETURNING id")
                .bind(item.name.trim()).bind(category_id).bind(category_name).bind(clean(&item.sku)).bind(clean(&item.barcode)).bind(item.price).bind(item.cost).bind(item.low_stock).bind(item.sold_by.trim()).bind(item.description.trim()).fetch_one(&mut *tx).await?
        };
        save_map(
            &mut tx,
            provider,
            "product",
            &item.external_id,
            id,
            item.version.as_deref(),
            &hash,
        )
        .await?;
        for variant in &item.variants {
            let hash = source_hash(variant)?;
            if unchanged(
                &mut tx,
                provider,
                "product_variant",
                &variant.external_id,
                &hash,
            )
            .await?
            {
                continue;
            }
            let local =
                mapped_id(&mut tx, provider, "product_variant", &variant.external_id).await?;
            let variant_id: i32 = if let Some(variant_id) = local {
                sqlx::query_scalar("UPDATE product_variants SET size=$1,color=$2,sku=$3,barcode=$4,price=$5,cost=$6,low_stock=$7,wholesale_min_qty=$8,wholesale_price=$9,active=1,updated_at=CURRENT_TIMESTAMP WHERE id=$10 AND product_id=$11 RETURNING id")
                    .bind(variant.size.trim()).bind(variant.color.trim()).bind(clean(&variant.sku)).bind(clean(&variant.barcode)).bind(variant.price).bind(variant.cost).bind(variant.low_stock).bind(variant.wholesale_min_qty).bind(variant.wholesale_price).bind(variant_id).bind(id).fetch_optional(&mut *tx).await?
                    .context("ERP variant mapping belongs to a different product or no longer exists")?
            } else {
                sqlx::query_scalar("INSERT INTO product_variants(product_id,size,color,sku,barcode,price,cost,low_stock,wholesale_min_qty,wholesale_price,stock,active) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,0,1) RETURNING id")
                    .bind(id).bind(variant.size.trim()).bind(variant.color.trim()).bind(clean(&variant.sku)).bind(clean(&variant.barcode)).bind(variant.price).bind(variant.cost).bind(variant.low_stock).bind(variant.wholesale_min_qty).bind(variant.wholesale_price).fetch_one(&mut *tx).await?
            };
            save_map(
                &mut tx,
                provider,
                "product_variant",
                &variant.external_id,
                variant_id,
                variant.version.as_deref(),
                &hash,
            )
            .await?;
        }
    }

    for item in &page.customers {
        let hash = source_hash(item)?;
        if unchanged(&mut tx, provider, "customer", &item.external_id, &hash).await? {
            continue;
        }
        let local = mapped_id(&mut tx, provider, "customer", &item.external_id).await?;
        let id: i32 = if let Some(id) = local {
            sqlx::query_scalar("UPDATE customers SET name=$1,phone=$2,email=$3,address=$4,remarks=$5,credit_limit=$6 WHERE id=$7 RETURNING id")
                .bind(item.name.trim()).bind(item.phone.trim()).bind(item.email.trim()).bind(item.address.trim()).bind(item.remarks.trim()).bind(item.credit_limit).bind(id).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar("INSERT INTO customers(name,phone,email,address,remarks,credit_limit,total_visit,total_spent,points,current_balance) VALUES($1,$2,$3,$4,$5,$6,0,0,0,0) RETURNING id")
                .bind(item.name.trim()).bind(item.phone.trim()).bind(item.email.trim()).bind(item.address.trim()).bind(item.remarks.trim()).bind(item.credit_limit).fetch_one(&mut *tx).await?
        };
        save_map(
            &mut tx,
            provider,
            "customer",
            &item.external_id,
            id,
            item.version.as_deref(),
            &hash,
        )
        .await?;
    }

    for item in &page.suppliers {
        let hash = source_hash(item)?;
        if unchanged(&mut tx, provider, "supplier", &item.external_id, &hash).await? {
            continue;
        }
        let local = mapped_id(&mut tx, provider, "supplier", &item.external_id).await?;
        let status = if item.active { "Active" } else { "Inactive" };
        let id: i32 = if let Some(id) = local {
            sqlx::query_scalar("UPDATE suppliers SET name=$1,company_name=$2,contact_person=$3,phone=$4,email=$5,address=$6,tax_number=$7,website=$8,payment_terms=$9,bank_account=$10,status=$11 WHERE id=$12 RETURNING id")
                .bind(item.name.trim()).bind(item.company_name.trim()).bind(item.contact_person.trim()).bind(item.phone.trim()).bind(item.email.trim()).bind(item.address.trim()).bind(item.tax_number.trim()).bind(item.website.trim()).bind(item.payment_terms.trim()).bind(item.bank_account.trim()).bind(status).bind(id).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar("INSERT INTO suppliers(name,company_name,contact_person,phone,email,address,tax_number,website,payment_terms,bank_account,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id")
                .bind(item.name.trim()).bind(item.company_name.trim()).bind(item.contact_person.trim()).bind(item.phone.trim()).bind(item.email.trim()).bind(item.address.trim()).bind(item.tax_number.trim()).bind(item.website.trim()).bind(item.payment_terms.trim()).bind(item.bank_account.trim()).bind(status).fetch_one(&mut *tx).await?
        };
        save_map(
            &mut tx,
            provider,
            "supplier",
            &item.external_id,
            id,
            item.version.as_deref(),
            &hash,
        )
        .await?;
    }

    for item in &page.locations {
        let hash = source_hash(item)?;
        if unchanged(&mut tx, provider, "location", &item.external_id, &hash).await? {
            continue;
        }
        let local = mapped_id(&mut tx, provider, "location", &item.external_id).await?;
        let id: i32 = if let Some(id) = local {
            let old: String =
                sqlx::query_scalar("SELECT name FROM locations WHERE id=$1 FOR UPDATE")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            for (table, column) in [
                ("products", "warehouse"),
                ("stock_movements", "location"),
                ("product_locations", "location"),
            ] {
                sqlx::query(&format!(
                    "UPDATE {table} SET {column}=$1 WHERE LOWER(TRIM({column}))=LOWER(TRIM($2))"
                ))
                .bind(item.name.trim())
                .bind(&old)
                .execute(&mut *tx)
                .await?;
            }
            let variants: bool =
                sqlx::query_scalar("SELECT to_regclass('variant_stock_batches') IS NOT NULL")
                    .fetch_one(&mut *tx)
                    .await?;
            if variants {
                sqlx::query("UPDATE variant_stock_batches SET location=$1 WHERE LOWER(TRIM(location))=LOWER(TRIM($2))")
                    .bind(item.name.trim()).bind(&old).execute(&mut *tx).await?;
            }
            sqlx::query_scalar("UPDATE locations SET name=$1 WHERE id=$2 RETURNING id")
                .bind(item.name.trim())
                .bind(id)
                .fetch_one(&mut *tx)
                .await?
        } else {
            sqlx::query_scalar("INSERT INTO locations(name) VALUES($1) RETURNING id")
                .bind(item.name.trim())
                .fetch_one(&mut *tx)
                .await?
        };
        save_map(
            &mut tx,
            provider,
            "location",
            &item.external_id,
            id,
            item.version.as_deref(),
            &hash,
        )
        .await?;
    }

    sqlx::query("INSERT INTO rust_erp_sync_state(provider,stream,cursor_value,last_started_at,last_succeeded_at,last_error) VALUES($1,'master-data',$2,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,NULL) ON CONFLICT(provider,stream) DO UPDATE SET cursor_value=EXCLUDED.cursor_value,last_started_at=CURRENT_TIMESTAMP,last_succeeded_at=CURRENT_TIMESTAMP,last_error=NULL")
        .bind(provider).bind(&page.cursor).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO rust_erp_import_log(provider,cursor_value,categories_count,products_count,customers_count,suppliers_count,locations_count) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(provider).bind(&page.cursor).bind(page.categories.len() as i32).bind(page.products.len() as i32).bind(page.customers.len() as i32).bind(page.suppliers.len() as i32).bind(page.locations.len() as i32).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn validate_page(page: &MasterPage) -> Result<()> {
    let total = page.categories.len()
        + page.products.len()
        + page
            .products
            .iter()
            .map(|product| product.variants.len())
            .sum::<usize>()
        + page.customers.len()
        + page.suppliers.len()
        + page.locations.len();
    anyhow::ensure!(total <= 1000, "ERP master-data page exceeds 1000 records");
    let mut identities = std::collections::HashSet::new();
    for (kind, external, name) in page
        .categories
        .iter()
        .map(|v| ("category", &v.external_id, &v.name))
        .chain(
            page.products
                .iter()
                .map(|v| ("product", &v.external_id, &v.name)),
        )
        .chain(
            page.customers
                .iter()
                .map(|v| ("customer", &v.external_id, &v.name)),
        )
        .chain(
            page.suppliers
                .iter()
                .map(|v| ("supplier", &v.external_id, &v.name)),
        )
        .chain(
            page.locations
                .iter()
                .map(|v| ("location", &v.external_id, &v.name)),
        )
    {
        anyhow::ensure!(
            !external.trim().is_empty() && external.len() <= 200,
            "Invalid {kind} external_id"
        );
        anyhow::ensure!(
            identities.insert(format!("{kind}:{external}")),
            "Duplicate {kind} external_id {external}"
        );
        anyhow::ensure!(
            !name.trim().is_empty() && name.len() <= 500,
            "Invalid {kind} name"
        );
    }
    for product in &page.products {
        anyhow::ensure!(
            [product.price, product.cost, product.low_stock]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0),
            "Invalid product price or threshold"
        );
        anyhow::ensure!(
            matches!(
                product.sold_by.to_lowercase().as_str(),
                "each" | "weight" | "service" | "variants"
            ),
            "Invalid product sold_by"
        );
        anyhow::ensure!(
            product.variants.is_empty() || product.sold_by.eq_ignore_ascii_case("variants"),
            "Product variants require sold_by=Variants"
        );
        for variant in &product.variants {
            anyhow::ensure!(
                !variant.external_id.trim().is_empty() && variant.external_id.len() <= 200,
                "Invalid product_variant external_id"
            );
            anyhow::ensure!(
                identities.insert(format!("product_variant:{}", variant.external_id)),
                "Duplicate product_variant external_id {}",
                variant.external_id
            );
            anyhow::ensure!(
                [variant.price, variant.cost, variant.wholesale_price]
                    .iter()
                    .all(|value| value.is_finite() && *value >= 0.0)
                    && variant.low_stock >= 0
                    && variant.wholesale_min_qty >= 0,
                "Invalid product variant price or threshold"
            );
        }
    }
    for customer in &page.customers {
        anyhow::ensure!(
            customer.credit_limit.is_finite() && customer.credit_limit >= 0.0,
            "Invalid customer credit limit"
        );
    }
    Ok(())
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn source_hash<T: Serialize>(value: &T) -> Result<String> {
    use sha2::{Digest, Sha256};
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}

async fn mapped_id(
    conn: &mut PgConnection,
    provider: &str,
    kind: &str,
    external: &str,
) -> Result<Option<i32>> {
    Ok(sqlx::query_scalar("SELECT local_id FROM rust_erp_entity_map WHERE provider=$1 AND entity_type=$2 AND external_id=$3")
        .bind(provider).bind(kind).bind(external).fetch_optional(&mut *conn).await?)
}

async fn unchanged(
    conn: &mut PgConnection,
    provider: &str,
    kind: &str,
    external: &str,
    hash: &str,
) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM rust_erp_entity_map WHERE provider=$1 AND entity_type=$2 AND external_id=$3 AND source_hash=$4)")
        .bind(provider).bind(kind).bind(external).bind(hash).fetch_one(&mut *conn).await?)
}

async fn save_map(
    conn: &mut PgConnection,
    provider: &str,
    kind: &str,
    external: &str,
    local: i32,
    version: Option<&str>,
    hash: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO rust_erp_entity_map(provider,entity_type,external_id,local_id,source_version,source_hash) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(provider,entity_type,external_id) DO UPDATE SET local_id=EXCLUDED.local_id,source_version=EXCLUDED.source_version,source_hash=EXCLUDED.source_hash,synced_at=CURRENT_TIMESTAMP")
        .bind(provider).bind(kind).bind(external).bind(local).bind(version).bind(hash).execute(&mut *conn).await?;
    Ok(())
}

async fn record_error(pool: &PgPool, provider: &str, error: &str) -> Result<()> {
    sqlx::query("INSERT INTO rust_erp_sync_state(provider,stream,last_started_at,last_error) VALUES($1,'master-data',CURRENT_TIMESTAMP,$2) ON CONFLICT(provider,stream) DO UPDATE SET last_started_at=CURRENT_TIMESTAMP,last_error=EXCLUDED.last_error")
        .bind(provider).bind(error).execute(pool).await?;
    Ok(())
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_financial_master_data() {
        let mut page = MasterPage::default();
        page.products.push(MasterProduct {
            external_id: "p-1".into(),
            version: None,
            name: "Product".into(),
            category_external_id: None,
            sku: None,
            barcode: None,
            price: f64::NAN,
            cost: 0.0,
            low_stock: 0.0,
            sold_by: "Each".into(),
            description: String::new(),
            variants: vec![],
        });
        assert!(validate_page(&page).is_err());
    }

    #[test]
    fn source_hash_is_stable() {
        let location = MasterLocation {
            external_id: "l-1".into(),
            version: Some("2".into()),
            name: "Shop".into(),
        };
        assert_eq!(
            source_hash(&location).unwrap(),
            source_hash(&location).unwrap()
        );
    }

    #[test]
    fn duplicate_external_ids_are_rejected() {
        let mut page = MasterPage::default();
        page.locations = vec![
            MasterLocation {
                external_id: "same".into(),
                version: None,
                name: "A".into(),
            },
            MasterLocation {
                external_id: "same".into(),
                version: None,
                name: "B".into(),
            },
        ];
        assert!(validate_page(&page).is_err());
    }
}
