use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use std::time::Duration;

const MAX_ATTEMPTS: i32 = 12;

#[derive(Clone, Debug)]
pub struct ErpSyncConfig {
    pub webhook_url: String,
    pub api_token: Option<String>,
    pub tenant: Option<String>,
    pub interval: Duration,
    pub batch_size: i64,
}

impl ErpSyncConfig {
    pub fn from_env() -> Result<Option<Self>> {
        if !crate::ERP_ENABLED { return Ok(None); }
        let webhook_url = std::env::var("ZAY_POS_ERP_WEBHOOK_URL")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if webhook_url.is_empty() {
            return Ok(None);
        }
        let parsed = reqwest::Url::parse(&webhook_url)
            .context("ZAY_POS_ERP_WEBHOOK_URL must be a valid URL")?;
        let local_http = parsed.scheme() == "http"
            && parsed
                .host_str()
                .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
        anyhow::ensure!(
            parsed.scheme() == "https" || local_http,
            "ZAY_POS_ERP_WEBHOOK_URL must use HTTPS (HTTP is allowed only for localhost)"
        );
        let interval_seconds = env_number("ZAY_POS_ERP_SYNC_INTERVAL_SECONDS", 30, 5, 3600)?;
        let batch_size = env_number("ZAY_POS_ERP_SYNC_BATCH_SIZE", 25, 1, 100)?;
        Ok(Some(Self {
            webhook_url,
            api_token: optional_env("ZAY_POS_ERP_API_TOKEN"),
            tenant: optional_env("ZAY_POS_ERP_TENANT"),
            interval: Duration::from_secs(interval_seconds as u64),
            batch_size,
        }))
    }
}

fn optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn env_number(key: &str, default: i64, min: i64, max: i64) -> Result<i64> {
    let value = match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => value
            .parse::<i64>()
            .with_context(|| format!("{key} must be a number"))?,
        _ => default,
    };
    anyhow::ensure!(
        (min..=max).contains(&value),
        "{key} must be between {min} and {max}"
    );
    Ok(value)
}

pub async fn enqueue_sale_completed(tx: &mut PgConnection, sale_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    enqueue_sale(tx, sale_id, "sale.completed").await
}

pub async fn enqueue_sale_refunded(tx: &mut PgConnection, sale_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    enqueue_sale(tx, sale_id, "sale.refunded").await
}

async fn enqueue_sale(tx: &mut PgConnection, sale_id: i32, event_type: &str) -> Result<()> {
    let payload: Value = sqlx::query_scalar(
        r#"
        SELECT jsonb_build_object(
            'schema_version', 1,
            'sale', to_jsonb(s),
            'items', COALESCE((
                SELECT jsonb_agg(to_jsonb(si) ORDER BY si.id)
                FROM sale_items si WHERE si.sale_id = s.id
            ), '[]'::jsonb)
        )
        FROM sales s WHERE s.id = $1
        "#,
    )
    .bind(sale_id)
    .fetch_optional(&mut *tx)
    .await
    .context("failed to build ERP sale event")?
    .context("sale not found while building ERP event")?;

    sqlx::query(
        "INSERT INTO rust_erp_outbox(event_key,event_type,aggregate_type,aggregate_id,payload) VALUES($1,$2,'sale',$3,$4) ON CONFLICT(event_key) DO NOTHING",
    )
    .bind(format!("{event_type}:{sale_id}"))
    .bind(event_type)
    .bind(sale_id.to_string())
    .bind(payload)
    .execute(&mut *tx)
    .await
    .context("ERP outbox unavailable; apply migrations/010_erp_outbox.sql")?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct OutboxEvent {
    id: i64,
    event_key: String,
    event_type: String,
    aggregate_type: String,
    aggregate_id: String,
    payload: Value,
    attempts: i32,
    created_at: chrono::NaiveDateTime,
}

#[derive(Serialize)]
struct Delivery<'a> {
    event_id: i64,
    idempotency_key: &'a str,
    event_type: &'a str,
    aggregate_type: &'a str,
    aggregate_id: &'a str,
    occurred_at: chrono::NaiveDateTime,
    payload: &'a Value,
}

pub async fn run_worker(pool: PgPool, config: ErpSyncConfig) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            eprintln!("ERP sync client could not start: {error}");
            return;
        }
    };
    loop {
        match sync_once(&pool, &client, &config).await {
            Ok(_) => {
                let _ = crate::erp_operations::heartbeat(&pool, "outbox-delivery", None).await;
            }
            Err(error) => {
                let message = format!("{error:#}");
                let _ = crate::erp_operations::heartbeat(&pool, "outbox-delivery", Some(&message))
                    .await;
                eprintln!("ERP sync cycle failed: {message}");
            }
        }
        tokio::time::sleep(config.interval).await;
    }
}

async fn sync_once(
    pool: &PgPool,
    client: &reqwest::Client,
    config: &ErpSyncConfig,
) -> Result<usize> {
    let mut delivered = 0;
    for _ in 0..config.batch_size {
        let Some(event) = claim_next(pool).await? else {
            break;
        };
        let result = deliver(client, config, &event).await;
        match result {
            Ok(()) => {
                sqlx::query("UPDATE rust_erp_outbox SET status='sent',sent_at=CURRENT_TIMESTAMP,locked_until=NULL,last_error=NULL WHERE id=$1")
                    .bind(event.id).execute(pool).await?;
                delivered += 1;
            }
            Err(error) => mark_failed(pool, &event, &format!("{error:#}")).await?,
        }
    }
    Ok(delivered)
}

async fn claim_next(pool: &PgPool) -> Result<Option<OutboxEvent>> {
    let mut tx = pool.begin().await?;
    let event = sqlx::query_as::<_, OutboxEvent>(
        r#"
        SELECT id,event_key,event_type,aggregate_type,aggregate_id,payload,attempts,created_at
        FROM rust_erp_outbox
        WHERE (status='pending' OR (status='processing' AND locked_until<CURRENT_TIMESTAMP))
          AND next_attempt_at<=CURRENT_TIMESTAMP
        ORDER BY id
        FOR UPDATE SKIP LOCKED LIMIT 1
        "#,
    )
    .fetch_optional(&mut *tx)
    .await
    .context("ERP outbox unavailable; apply migrations/010_erp_outbox.sql")?;
    if let Some(ref event) = event {
        sqlx::query("UPDATE rust_erp_outbox SET status='processing',attempts=attempts+1,locked_until=CURRENT_TIMESTAMP+INTERVAL '2 minutes' WHERE id=$1")
            .bind(event.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(event)
}

async fn deliver(
    client: &reqwest::Client,
    config: &ErpSyncConfig,
    event: &OutboxEvent,
) -> Result<()> {
    let body = Delivery {
        event_id: event.id,
        idempotency_key: &event.event_key,
        event_type: &event.event_type,
        aggregate_type: &event.aggregate_type,
        aggregate_id: &event.aggregate_id,
        occurred_at: event.created_at,
        payload: &event.payload,
    };
    let mut request = client
        .post(&config.webhook_url)
        .header("Idempotency-Key", &event.event_key)
        .json(&body);
    if let Some(token) = &config.api_token {
        request = request.bearer_auth(token);
    }
    if let Some(tenant) = &config.tenant {
        request = request.header("X-ERP-Tenant", tenant);
    }
    let response = request.send().await.context("ERP request failed")?;
    let status = response.status();
    if !status.is_success() {
        let message = response.text().await.unwrap_or_default();
        anyhow::bail!("ERP returned {status}: {}", truncate(&message, 500));
    }
    Ok(())
}

async fn mark_failed(pool: &PgPool, event: &OutboxEvent, error: &str) -> Result<()> {
    let attempts = event.attempts + 1;
    let dead = attempts >= MAX_ATTEMPTS;
    let delay = retry_delay_seconds(attempts);
    sqlx::query("UPDATE rust_erp_outbox SET status=$2,next_attempt_at=CURRENT_TIMESTAMP+make_interval(secs=>$3),locked_until=NULL,last_error=$4 WHERE id=$1")
        .bind(event.id)
        .bind(if dead { "dead" } else { "pending" })
        .bind(delay)
        .bind(truncate(error, 2000))
        .execute(pool).await?;
    Ok(())
}

fn retry_delay_seconds(attempts: i32) -> i32 {
    5_i32
        .saturating_mul(2_i32.saturating_pow(attempts.clamp(1, 10) as u32))
        .min(3600)
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_is_bounded() {
        assert_eq!(retry_delay_seconds(1), 10);
        assert_eq!(retry_delay_seconds(20), 3600);
    }

    #[test]
    fn long_errors_are_truncated_on_character_boundaries() {
        let truncated = truncate("မြန်မာ", 3);
        assert_eq!(truncated.chars().count(), 3);
        assert!("မြန်မာ".starts_with(&truncated));
    }
}
