//! Local standalone database lifecycle. No network connection or sync is created here.
use crate::storage::Pool;
use anyhow::{ensure, Context, Result};
use base64::Engine;
use rusqlite::functions::FunctionFlags;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use std::{path::Path, str::FromStr, time::Duration};

pub async fn connect(url: &str) -> Result<Pool> {
    ensure!(
        url.starts_with("sqlite:"),
        "Standalone requires a SQLite file"
    );
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(Duration::from_secs(30));
    if let Some(path) = options
        .get_filename()
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(path)?;
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(35))
        .after_connect(|conn, _| {
            Box::pin(async move {
                let mut handle = conn.lock_handle().await?;
                // SQLx locks the handle for this scope. rusqlite's from_handle is non-owning;
                // functions are owned by SQLite and remain registered after this wrapper is dropped.
                let raw =
                    unsafe { rusqlite::Connection::from_handle(handle.as_raw_handle().as_ptr()) }
                        .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;
                register(&raw).map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .context("Cannot open standalone database")?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let known:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='kay_schema_versions')").fetch_one(&mut *tx).await?;
    if !known {
        let existing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&mut *tx)
        .await?;
        ensure!(existing==0,"This is an existing database from another app. Choose a new standalone file; automatic import is not supported.");
        sqlx::raw_sql(include_str!(
            "../../../migrations/sqlite/001_standalone.sql"
        ))
        .execute(&mut *tx)
        .await?;
        for (key, value) in [
            ("currency_symbol", "Ks"),
            ("currency_code", "MMK"),
            ("credit_limit_enabled", "true"),
            ("credit_due_days", "15"),
            ("loyalty_points_per_dollar", "0"),
            ("default_payment_type", "Cash"),
            ("shop_name", "KAY POS"),
        ] {
            sqlx::query("INSERT OR IGNORE INTO settings(key,value) VALUES(?,?)")
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT INTO payment_types(name) VALUES('Cash'),('Credit')")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO expense_notification_settings(id) VALUES(1)")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO locations(name) VALUES('Shop')")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO kay_schema_versions(version) VALUES(1)")
            .execute(&mut *tx)
            .await?;
    }
    let version: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(version),0) FROM kay_schema_versions")
            .fetch_one(&mut *tx)
            .await?;
    ensure!(
        version == 1,
        "Standalone schema version {version} is not supported by this app"
    );
    sqlx::raw_sql(crate::locations::HIERARCHY_SCHEMA).execute(&mut *tx).await?;
    sqlx::raw_sql(crate::warranty::SCHEMA).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Pool::Sqlite(pool))
}

pub async fn needs_admin(pool: &Pool) -> Result<bool> {
    ensure!(
        pool.is_sqlite(),
        "Admin setup is only available for standalone databases"
    );
    Ok(
        crate::storage::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await?
            == 0,
    )
}
pub async fn create_initial_admin(pool: &Pool, username: &str, password: &str) -> Result<()> {
    ensure!(
        pool.is_sqlite(),
        "Admin setup is only available for standalone databases"
    );
    ensure!(!username.trim().is_empty(), "Enter an admin username");
    ensure!(
        password.chars().count() >= 8,
        "Admin password must contain at least 8 characters"
    );
    let mut tx = pool.begin().await?;
    let users: i64 = crate::storage::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *tx)
        .await?;
    ensure!(
        users == 0,
        "This database already has users. Sign in with an existing account"
    );
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let mut hash = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, 100000, &mut hash);
    crate::storage::query(
        "INSERT INTO users(username,password_hash,salt,role,is_active) VALUES($1,$2,$3,'admin',1)",
    )
    .bind(username.trim())
    .bind(hex::encode(hash))
    .bind(hex::encode(salt))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
/// SQLite online backup includes committed WAL pages; copying the .db file alone does not.
pub async fn backup(pool: &Pool, destination: &Path) -> Result<()> {
    ensure!(
        pool.is_sqlite(),
        "Use the server backup tools for PostgreSQL"
    );
    ensure!(!destination.exists(), "Backup destination already exists");
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let Pool::Sqlite(p) = pool else {
        unreachable!()
    };
    sqlx::query("VACUUM INTO ?")
        .bind(destination.to_string_lossy().as_ref())
        .execute(p)
        .await?;
    Ok(())
}
fn text(value: rusqlite::types::ValueRef<'_>) -> String {
    use rusqlite::types::ValueRef::*;
    match value {
        Null => String::new(),
        Integer(n) => n.to_string(),
        Real(n) => n.to_string(),
        Text(s) => String::from_utf8_lossy(s).into_owned(),
        Blob(b) => base64::engine::general_purpose::STANDARD.encode(b),
    }
}
fn register(c: &rusqlite::Connection) -> rusqlite::Result<()> {
    // Use the same canonical fractional timestamp format as SQLx's chrono binder.
    // A monotonic microsecond clock prevents stale edits sharing one millisecond.
    c.create_scalar_function("kay_clock_timestamp", 0, FunctionFlags::SQLITE_UTF8, |_| {
        use std::sync::atomic::{AtomicI64, Ordering};
        static LAST: AtomicI64 = AtomicI64::new(0);
        let now = chrono::Local::now()
            .naive_local()
            .and_utc()
            .timestamp_micros();
        let previous = LAST
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last| {
                Some(now.max(last + 1))
            })
            .unwrap();
        let value = chrono::DateTime::from_timestamp_micros(now.max(previous + 1))
            .unwrap()
            .naive_utc();
        Ok(value.format("%F %T%.f").to_string())
    })?;
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    c.create_scalar_function("lower", 1, flags, |v| {
        Ok(v.get::<Option<String>>(0)?.map(|s| s.to_lowercase()))
    })?;
    c.create_scalar_function("upper", 1, flags, |v| {
        Ok(v.get::<Option<String>>(0)?.map(|s| s.to_uppercase()))
    })?;
    c.create_scalar_function("concat", -1, flags, |v| {
        Ok((0..v.len()).map(|i| text(v.get_raw(i))).collect::<String>())
    })?;
    c.create_scalar_function("concat_ws", -1, flags, |v| {
        let sep = v.get::<String>(0)?;
        Ok((1..v.len())
            .filter(|i| !matches!(v.get_raw(*i), rusqlite::types::ValueRef::Null))
            .map(|i| text(v.get_raw(i)))
            .collect::<Vec<_>>()
            .join(&sep))
    })?;
    for (name, greatest) in [("kay_greatest", true), ("kay_least", false)] {
        c.create_scalar_function(name, -1, flags, move |v| {
            let mut values = (0..v.len())
                .filter(|i| !matches!(v.get_raw(*i), rusqlite::types::ValueRef::Null))
                .map(|i| text(v.get_raw(i)).parse::<f64>().unwrap_or(0.0));
            Ok(values
                .next()
                .map(|first| values.fold(first, |a, b| if greatest { a.max(b) } else { a.min(b) })))
        })?;
    }
    c.create_scalar_function("encode", 2, flags, |v| {
        let encoding = v.get::<String>(1)?;
        let bytes = v.get::<Option<Vec<u8>>>(0)?;
        Ok(bytes.map(|b| {
            if encoding == "base64" {
                base64::engine::general_purpose::STANDARD.encode(b)
            } else {
                hex::encode(b)
            }
        }))
    })?;
    c.create_scalar_function("decode", 2, flags, |v| {
        let s = v.get::<String>(0)?;
        let enc = v.get::<String>(1)?;
        let b = if enc == "base64" {
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?
        } else {
            hex::decode(s).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?
        };
        Ok(b)
    })?;
    c.create_scalar_function("kay_regexp", 2, flags, |v| {
        let pattern = v.get::<Option<String>>(0)?;
        let s = v.get::<Option<String>>(1)?;
        match (pattern, s) {
            (Some(p), Some(s)) => Ok(Some(
                regex::Regex::new(&p)
                    .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?
                    .is_match(&s),
            )),
            _ => Ok(None),
        }
    })?;
    c.create_scalar_function("regexp_replace", -1, flags, |v| {
        let s = v.get::<Option<String>>(0)?;
        let pat = v.get::<String>(1)?;
        let replacement = v.get::<String>(2)?;
        let global = v.len() > 3 && v.get::<String>(3)?.contains('g');
        let re =
            regex::Regex::new(&pat).map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
        Ok(s.map(|s| {
            if global {
                re.replace_all(&s, replacement.as_str()).into_owned()
            } else {
                re.replace(&s, replacement.as_str()).into_owned()
            }
        }))
    })?;
    c.create_scalar_function("kay_substring_regex", 2, flags, |v| {
        let s = v.get::<String>(0)?;
        let re = regex::Regex::new(&v.get::<String>(1)?)
            .map_err(|e| rusqlite::Error::UserFunctionError(Box::new(e)))?;
        Ok(re.find(&s).map(|m| m.as_str().to_string()))
    })?;
    c.create_scalar_function("kay_to_char", 2, flags, |v| {
        let s = text(v.get_raw(0));
        let format = v
            .get::<String>(1)?
            .replace("YYYY", "%Y")
            .replace("HH24", "%H")
            .replace("MM", "%m")
            .replace("DD", "%d")
            .replace("MI", "%M")
            .replace("SS", "%S")
            .replace(" TZ", "");
        let d = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f")
            .ok()
            .or_else(|| {
                chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                    .ok()
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
            });
        Ok(d.map(|d| d.format(&format).to_string()))
    })?;
    Ok(())
}
