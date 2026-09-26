use crate::auth::{Permission, Session};
use anyhow::{ensure, Result};
use chrono::NaiveDate;
use sqlx::{PgConnection, PgPool};

// Call inside the business transaction. Never log credentials or attachment bodies.
pub(crate) async fn record(
    conn: &mut PgConnection,
    actor: &Session,
    action: &str,
    details: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO user_activity_log(user_id,username,action,details,ip_address,created_at) VALUES((SELECT id FROM users WHERE username=$1),$1,$2,$3,NULL,LOCALTIMESTAMP)")
        .bind(actor.username()).bind(action).bind(details).execute(conn).await?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct Entry {
    pub id: i32,
    pub username: String,
    pub action: String,
    pub details: String,
    pub ip_address: String,
    pub created_at: String,
}

pub async fn list(
    pool: &PgPool,
    actor: &Session,
    from: &str,
    to: &str,
    query: &str,
    page: i64,
) -> Result<Vec<Entry>> {
    let from = NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
    let to = NaiveDate::parse_from_str(to, "%Y-%m-%d")?;
    ensure!(
        from <= to && (0..=1_000_000).contains(&page),
        "Invalid date range or page"
    );
    ensure!(query.len() <= 300, "Search is too long");
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let rows=sqlx::query_as("SELECT id,username,action,COALESCE(details,'') AS details,COALESCE(ip_address,'') AS ip_address,created_at::text AS created_at FROM user_activity_log WHERE created_at >= $1::date AND created_at < $2::date + INTERVAL '1 day' AND ($3='' OR strpos(lower(username||' '||action||' '||COALESCE(details,'')),lower($3))>0) ORDER BY created_at DESC,id DESC LIMIT 51 OFFSET $4")
        .bind(from).bind(to).bind(query.trim()).bind(page*50).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}
