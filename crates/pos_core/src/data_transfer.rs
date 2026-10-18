//! Explicit, one-way snapshots. Never merges databases, changes the server or starts sync.
use crate::{
    auth::{Permission, Session},
    storage::{self, Pool},
};
use anyhow::{ensure, Context, Result};
use futures_util::TryStreamExt;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub tables: usize,
    pub rows: u64,
}
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn columns() -> BTreeMap<String, Vec<String>> {
    serde_json::from_str(include_str!("../../../migrations/sqlite/columns.json"))
        .expect("standalone schema metadata")
}
fn file_url(path: &Path) -> String {
    format!(
        "sqlite://{}",
        path.to_string_lossy()
            .replace('\\', "/")
            .replace('%', "%25")
            .replace('?', "%3F")
            .replace('#', "%23")
    )
}
fn temporary(destination: &Path) -> Result<tempfile::TempPath> {
    ensure!(!destination.exists(),"Destination already exists. Choose a new database file; existing records are never overwritten");
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    Ok(tempfile::Builder::new()
        .prefix(".kay-pos-transfer-")
        .suffix(".db")
        .tempfile_in(parent)?
        .into_temp_path())
}
fn publish(temp: tempfile::TempPath, destination: &Path) -> Result<()> {
    temp.persist_noclobber(destination)
        .map_err(|e| anyhow::anyhow!("Cannot save exported database: {}", e.error))?;
    Ok(())
}
async fn verify(conn: &mut sqlx::SqliteConnection) -> Result<Summary> {
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM kay_schema_versions")
        .fetch_one(&mut *conn)
        .await
        .context("Not a KAY POS standalone export")?;
    ensure!(
        version == Some(1),
        "Unsupported standalone database version"
    );
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&mut *conn)
        .await?;
    ensure!(
        integrity == "ok",
        "Database integrity check failed: {integrity}"
    );
    ensure!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_optional(&mut *conn)
            .await?
            .is_none(),
        "Database contains broken record references; source data must be repaired first"
    );
    let addons:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name IN ('rust_serial_units','rust_warranty_claims','rust_warranty_events')").fetch_one(&mut *conn).await?;
    ensure!(addons==0 || addons==3,"Incomplete warranty schema; restore the missing tables before transfer");
    let mut summary = Summary::default();
    for (table, expected) in columns() {
        let actual: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(&mut *conn)
            .await?;
        if actual.is_empty() && (crate::warranty::is_addon_table(&table)||table=="rust_location_hierarchy") { continue; }
        ensure!(
            expected.iter().all(|c| actual.contains(c)),
            "Missing standalone columns in {table}"
        );
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", quoted(&table)))
            .fetch_one(&mut *conn)
            .await?;
        summary.tables += 1;
        summary.rows += count as u64;
    }
    let active: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM rust_held_sessions WHERE state='active'")
            .fetch_one(&mut *conn)
            .await?;
    ensure!(
        active == 0,
        "Return or complete active held sales on the source before exporting/importing"
    );
    let admins: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_active=1 AND lower(role)='admin'")
            .fetch_one(&mut *conn)
            .await?;
    ensure!(admins > 0, "Source has no active administrator account");
    Ok(summary)
}

/// Export PostgreSQL with one repeatable-read snapshot, or export the selected SQLite shop.
/// Authorization is rechecked against the database; the output contains account hashes.
pub async fn export(pool: &Pool, actor: &Session, destination: &Path) -> Result<Summary> {
    let mut source = pool.begin().await?;
    storage::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *source)
        .await?;
    actor.authorize(&mut source, Permission::Admin).await?;
    if pool.is_sqlite() {
        // Release the single local connection before SQLite's online backup acquires it.
        source.commit().await?;
        return export_local(pool, actor, destination).await;
    }
    let temp = temporary(destination)?;
    let target = crate::sqlite::connect(&file_url(&temp)).await?;
    let Pool::Sqlite(native) = &target else {
        unreachable!()
    };
    let result = async {
        let pg = source.postgres_connection()?;
        let mut sqlite = native.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("PRAGMA defer_foreign_keys=ON")
            .execute(&mut *sqlite)
            .await?;
        let result = copy_postgres(pg, &mut sqlite).await?;
        verify(&mut sqlite).await?;
        sqlite.commit().await?;
        source.commit().await?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(native)
            .await?;
        Ok::<_, anyhow::Error>(result)
    }
    .await;
    target.close().await;
    let result = result?;
    publish(temp, destination)?;
    Ok(result)
}
async fn export_local(pool: &Pool, actor: &Session, destination: &Path) -> Result<Summary> {
    let Pool::Sqlite(native) = pool else {
        unreachable!()
    };
    let temp = temporary(destination)?;
    let mut conn = pool.acquire().await?;
    actor.authorize(&mut conn, Permission::Admin).await?;
    drop(conn);
    sqlx::query("VACUUM INTO ?")
        .bind(temp.to_string_lossy().as_ref())
        .execute(native)
        .await?;
    let summary = inspect(&temp).await?;
    publish(temp, destination)?;
    Ok(summary)
}
async fn copy_postgres(
    pg: &mut sqlx::PgConnection,
    target: &mut sqlx::SqliteConnection,
) -> Result<Summary> {
    let tables = columns();
    let schema: String = sqlx::query_scalar("SELECT current_schema()")
        .fetch_one(&mut *pg)
        .await?;
    // Catalogs also include tables hidden by privileges. Never produce a partial shop.
    let catalog:Vec<(String,bool)>=sqlx::query_as("SELECT c.relname,has_table_privilege(c.oid,'SELECT') FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind IN ('r','p') ORDER BY c.relname").bind(&schema).fetch_all(&mut *pg).await?;
    for (table, can_read) in &catalog {
        ensure!(*can_read,"Read permission is required for server table {table}; partial exports are not supported");
    }
    let existing = catalog
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    for required in [
        "users",
        "products",
        "sales",
        "customers",
        "settings",
        "payment_types",
        "locations",
    ] {
        ensure!(
            existing.iter().any(|t| t == required),
            "Required POS table {required} is missing"
        );
    }
    // If row-level security would hide records, PostgreSQL rejects this export.
    sqlx::query("SET LOCAL row_security=off")
        .execute(&mut *pg)
        .await?;
    for table in &existing {
        ensure!(tables.contains_key(table),"Server table {table} is not supported by this standalone schema; export cancelled to prevent data loss");
    }
    for table in tables.keys() {
        sqlx::query(&format!("DELETE FROM {}", quoted(table)))
            .execute(&mut *target)
            .await?;
    }
    let mut summary = Summary::default();
    for (table, allowed) in tables {
        if !existing.contains(&table) {
            continue;
        }
        let fields:Vec<(String,String)>=sqlx::query_as("SELECT column_name,udt_name FROM information_schema.columns WHERE table_schema=$1 AND table_name=$2 ORDER BY ordinal_position").bind(&schema).bind(&table).fetch_all(&mut *pg).await?;
        ensure!(
            fields.iter().all(|(name, _)| allowed.contains(name)),
            "Server table {table} has unsupported columns; export cancelled to prevent data loss"
        );
        let expr = fields
            .iter()
            .map(|(name, ty)| {
                let n = quoted(name);
                if ty == "bytea" {
                    format!("encode({n},'hex')")
                } else if ty == "timestamptz" {
                    format!("{n}::timestamp::text")
                } else {
                    format!("{n}::text")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let select = format!(
            "SELECT json_build_array({expr})::text FROM {}.{}",
            quoted(&schema),
            quoted(&table)
        );
        let insert = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quoted(&table),
            fields
                .iter()
                .map(|(n, _)| quoted(n))
                .collect::<Vec<_>>()
                .join(","),
            vec!["?"; fields.len()].join(",")
        );
        let mut rows = sqlx::query_scalar::<_, String>(&select).fetch(&mut *pg);
        let mut count = 0u64;
        while let Some(row) = rows.try_next().await? {
            let values: Vec<Option<String>> = serde_json::from_str(&row)?;
            let mut query = sqlx::query(&insert);
            for ((_, ty), value) in fields.iter().zip(values) {
                query = if ty == "bytea" {
                    query.bind(value.map(hex::decode).transpose()?)
                } else if ty == "bool" {
                    query.bind(value.map(|v| i32::from(v == "true")))
                } else if matches!(ty.as_str(), "timestamp" | "timestamptz") {
                    let value = value
                        .map(|v| {
                            chrono::NaiveDateTime::parse_from_str(&v, "%Y-%m-%d %H:%M:%S%.f")
                                .map(|v| v.format("%F %T%.f").to_string())
                        })
                        .transpose()?;
                    query.bind(value)
                } else {
                    ensure!(
                        !value.as_ref().is_some_and(|v| matches!(
                            ty.as_str(),
                            "float8" | "float4" | "numeric"
                        ) && matches!(
                            v.as_str(),
                            "NaN" | "Infinity" | "-Infinity"
                        )),
                        "Non-finite value in {table}"
                    );
                    query.bind(value)
                };
            }
            query.execute(&mut *target).await.with_context(|| {
                format!("Cannot copy table {table}; original databases were not changed")
            })?;
            count += 1;
        }
        let copied: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", quoted(&table)))
            .fetch_one(&mut *target)
            .await?;
        ensure!(copied as u64 == count, "Row count mismatch in {table}");
        summary.tables += 1;
        summary.rows += count;
    }
    // Preserve server high-water marks even when the latest invoice/batch was deleted.
    for name in ["rust_invoice_number_seq", "rust_batch_number_seq"] {
        let sequence: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
            .bind(format!("{}.{}", quoted(&schema), quoted(name)))
            .fetch_one(&mut *pg)
            .await?;
        let high = if sequence.is_some() {
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT last_value FROM {}.{}",
                quoted(&schema),
                quoted(name)
            ))
            .fetch_one(&mut *pg)
            .await?
        } else {
            0
        };
        sqlx::query("UPDATE rust_sequences SET value=? WHERE name=?")
            .bind(high)
            .bind(name)
            .execute(&mut *target)
            .await?;
    }
    for table in ["service_orders", "category_groups"] {
        if !existing.iter().any(|t| t == table) {
            continue;
        }
        let sequence: Option<String> = sqlx::query_scalar("SELECT pg_get_serial_sequence($1,'id')")
            .bind(format!("{}.{}", quoted(&schema), quoted(table)))
            .fetch_one(&mut *pg)
            .await?;
        if let Some(sequence) = sequence {
            // pg_get_serial_sequence returns an already quoted catalog identifier.
            let high: i64 = sqlx::query_scalar(&format!("SELECT last_value FROM {sequence}"))
                .fetch_one(&mut *pg)
                .await?;
            sqlx::query("INSERT INTO rust_sequences(name,value) VALUES(?,?) ON CONFLICT(name) DO UPDATE SET value=max(value,excluded.value)").bind(table).bind(high).execute(&mut *target).await?;
        }
    }
    // Missing optional extension tables are empty; defaults needed for older compatible servers.
    sqlx::query("INSERT INTO expense_notification_settings(id) SELECT 1 WHERE NOT EXISTS(SELECT 1 FROM expense_notification_settings)").execute(&mut *target).await?;
    Ok(summary)
}

/// Inspect and copy an export/backup to a NEW local file. Source is opened read-only.
/// Existing databases, server data and the currently selected mode are untouched.
pub async fn import(source: &Path, destination: &Path) -> Result<Summary> {
    let temp = temporary(destination)?;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(source)
                .read_only(true)
                .create_if_missing(false),
        )
        .await
        .context("Cannot open export file")?;
    let result = async {
        let mut tx = pool.begin().await?;
        let summary = verify(&mut tx).await?;
        tx.commit().await?;
        sqlx::query("VACUUM INTO ?")
            .bind(temp.to_string_lossy().as_ref())
            .execute(&pool)
            .await?;
        // Verify the copied snapshot as well, guarding source changes between validation and copy.
        let copied = inspect(&temp).await?;
        ensure!(
            copied.tables == summary.tables,
            "Imported schema changed during copy"
        );
        Ok::<_, anyhow::Error>(copied)
    }
    .await;
    pool.close().await;
    let summary = result?;
    publish(temp, destination)?;
    Ok(summary)
}
async fn inspect(path: &Path) -> Result<Summary> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .read_only(true)
                .create_if_missing(false),
        )
        .await?;
    let result = async {
        let mut tx = pool.begin().await?;
        let summary = verify(&mut tx).await?;
        tx.commit().await?;
        Ok::<_, anyhow::Error>(summary)
    }
    .await;
    pool.close().await;
    result
}
