use anyhow::{Context, Result};
use sqlx::PgPool;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub pending: i64,
    pub processing: i64,
    pub sent: i64,
    pub dead: i64,
    pub oldest_pending: String,
    pub last_sent: String,
    pub mapped_entities: i64,
    pub latest_reconciliation: String,
    pub reconciliation_differences: i32,
    pub reconciliation_unmapped: i32,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct WorkerHealth {
    pub worker_name: String,
    pub status: String,
    pub last_success: String,
    pub last_error: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct FailedEvent {
    pub id: i64,
    pub event_key: String,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub status: String,
    pub attempts: i32,
    pub next_attempt: String,
    pub last_error: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OperationsData {
    pub summary: Summary,
    pub workers: Vec<WorkerHealth>,
    pub failures: Vec<FailedEvent>,
    pub mappings: Vec<crate::erp_accounting::AccountMapping>,
}

pub async fn load(pool: &PgPool, actor: &crate::auth::Session) -> Result<OperationsData> {
    let mut tx = pool.begin().await?;
    actor
        .authorize(&mut tx, crate::auth::Permission::Admin)
        .await?;
    let (pending, processing, sent, dead, oldest, last_sent): (i64,i64,i64,i64,Option<String>,Option<String>) = sqlx::query_as(
        "SELECT COUNT(*) FILTER(WHERE status='pending'),COUNT(*) FILTER(WHERE status='processing'),COUNT(*) FILTER(WHERE status='sent'),COUNT(*) FILTER(WHERE status='dead'),MIN(created_at) FILTER(WHERE status IN ('pending','processing'))::text,MAX(sent_at)::text FROM rust_erp_outbox"
    ).fetch_one(&mut *tx).await.context("ERP operations unavailable; apply migrations 010-014")?;
    let mapped_entities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rust_erp_entity_map")
        .fetch_one(&mut *tx)
        .await
        .context("Apply migration 011 for ERP mappings")?;
    let recon: Option<(String,i32,i32)> = sqlx::query_as("SELECT concat(snapshot_id,' / ',status),difference_count,unmapped_count FROM rust_erp_reconciliation_runs ORDER BY id DESC LIMIT 1")
        .fetch_optional(&mut *tx).await.context("Apply migration 012 for reconciliation")?;
    let workers = sqlx::query_as("SELECT worker_name,status,COALESCE(last_success_at::text,'Never') AS last_success,COALESCE(last_error,'') AS last_error,updated_at::text AS updated_at FROM rust_erp_worker_health ORDER BY worker_name")
        .fetch_all(&mut *tx).await.context("Apply migration 014 for ERP worker health")?;
    let failures = sqlx::query_as("SELECT id,event_key,event_type,aggregate_type,aggregate_id,status,attempts,next_attempt_at::text AS next_attempt,COALESCE(last_error,'') AS last_error,created_at::text AS created_at FROM rust_erp_outbox WHERE status IN ('pending','dead') AND (attempts>0 OR status='dead') ORDER BY CASE status WHEN 'dead' THEN 0 ELSE 1 END,id DESC LIMIT 100")
        .fetch_all(&mut *tx).await?;
    let mappings = sqlx::query_as("SELECT mapping_key,account_code,account_name FROM rust_erp_account_mappings ORDER BY mapping_key")
        .fetch_all(&mut *tx).await.context("Apply migration 013 for ERP accounting")?;
    tx.commit().await?;
    let (latest_reconciliation, reconciliation_differences, reconciliation_unmapped) =
        recon.unwrap_or_else(|| ("Not run".into(), 0, 0));
    Ok(OperationsData {
        summary: Summary {
            pending,
            processing,
            sent,
            dead,
            oldest_pending: oldest.unwrap_or_else(|| "None".into()),
            last_sent: last_sent.unwrap_or_else(|| "Never".into()),
            mapped_entities,
            latest_reconciliation,
            reconciliation_differences,
            reconciliation_unmapped,
        },
        workers,
        failures,
        mappings,
    })
}

pub async fn retry(pool: &PgPool, actor: &crate::auth::Session, event_id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    actor
        .authorize(&mut tx, crate::auth::Permission::Admin)
        .await?;
    let event: (String, String) =
        sqlx::query_as("SELECT event_key,status FROM rust_erp_outbox WHERE id=$1 FOR UPDATE")
            .bind(event_id)
            .fetch_optional(&mut *tx)
            .await?
            .context("ERP event no longer exists")?;
    anyhow::ensure!(
        matches!(event.1.as_str(), "pending" | "dead"),
        "Only pending or dead events can be retried"
    );
    sqlx::query("UPDATE rust_erp_outbox SET status='pending',attempts=0,next_attempt_at=CURRENT_TIMESTAMP,locked_until=NULL,last_error=NULL WHERE id=$1")
        .bind(event_id).execute(&mut *tx).await?;
    crate::activity::record(
        &mut tx,
        actor,
        "rust.erp.event.retry",
        &format!("event_id={event_id}; key={}", event.0),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn heartbeat(pool: &PgPool, worker: &str, error: Option<&str>) -> Result<()> {
    let message = error.map(|v| v.chars().take(2000).collect::<String>());
    sqlx::query("INSERT INTO rust_erp_worker_health(worker_name,status,last_started_at,last_finished_at,last_success_at,last_error) VALUES($1,$2,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,CASE WHEN $2='healthy' THEN CURRENT_TIMESTAMP END,$3) ON CONFLICT(worker_name) DO UPDATE SET status=EXCLUDED.status,last_finished_at=CURRENT_TIMESTAMP,last_success_at=CASE WHEN EXCLUDED.status='healthy' THEN CURRENT_TIMESTAMP ELSE rust_erp_worker_health.last_success_at END,last_error=EXCLUDED.last_error,updated_at=CURRENT_TIMESTAMP")
        .bind(worker).bind(if error.is_some(){"error"}else{"healthy"}).bind(message).execute(pool).await?;
    Ok(())
}
