use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Hold {
    pub id: i32,
    pub hold_no: String,
    pub cart_json: String,
    pub customer_id: Option<i32>,
    pub customer_name: String,
    pub payment_type: String,
    pub note: String,
    pub total_amount: f64,
    pub item_count: i32,
    pub created_at: String,
}
const SELECT:&str="SELECT id,hold_no,cart_json,customer_id,COALESCE(customer_name,'') AS customer_name,COALESCE(payment_type,'Cash') AS payment_type,COALESCE(note,'') AS note,COALESCE(total_amount,0)::float8 AS total_amount,COALESCE(item_count,0) AS item_count,created_at::text AS created_at FROM held_sales";
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Save {
        hold: Hold,
        return_token: Option<String>,
    },
    Resume {
        id: i32,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub request_id: String,
    pub action: Action,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub hold: Hold,
    pub token: Option<String>,
}

pub fn validate(hold: &Hold) -> Result<()> {
    ensure!(
        hold.cart_json.len() <= 2 * 1024 * 1024 && hold.note.len() <= 2000,
        "Held sale is too large"
    );
    let rows: Vec<serde_json::Value> = serde_json::from_str(&hold.cart_json)?;
    ensure!(
        !rows.is_empty() && rows.len() <= 200,
        "Hold requires 1 to 200 lines"
    );
    let mut total = 0.0;
    let mut qty = 0.0;
    for r in &rows {
        ensure!(
            r.get("id")
                .and_then(|v| v.as_i64())
                .is_some_and(|id| id > 0 && id <= i32::MAX as i64),
            "Held product ID is missing or invalid"
        );
        let q = r
            .get("qty")
            .and_then(|v| v.as_f64())
            .context("Held quantity is missing")?;
        let p = r
            .get("price")
            .and_then(|v| v.as_f64())
            .context("Held price is missing")?;
        ensure!(
            q > 0.0 && q <= 1_000_000.0 && p >= 0.0 && p <= 1e12 && q.is_finite() && p.is_finite(),
            "Invalid held quantity or price"
        );
        total += q * p;
        qty += q;
    }
    ensure!(
        total.is_finite()
            && hold.total_amount.is_finite()
            && hold.total_amount >= 0.0
            && qty <= i32::MAX as f64,
        "Invalid held total"
    );
    Ok(())
}
pub async fn list(pool: &PgPool, actor: &Session) -> Result<Vec<Hold>> {
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    let rows = sqlx::query_as(&format!("{SELECT} ORDER BY created_at DESC,id DESC"))
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(rows)
}
pub async fn session(pool: &PgPool, actor: &Session, token: &str) -> Result<Option<Outcome>> {
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT username,body,state FROM rust_held_sessions WHERE token=$1")
            .bind(token)
            .fetch_optional(&mut *tx)
            .await?;
    let row = row.context("Held session no longer exists")?;
    ensure!(
        row.0 == actor.username(),
        "Sign in as the operator who resumed this hold"
    );
    let result = if row.2 == "active" {
        Some(Outcome {
            hold: serde_json::from_str(&row.1)?,
            token: Some(token.into()),
        })
    } else {
        None
    };
    tx.commit().await?;
    Ok(result)
}
pub async fn execute(pool: &PgPool, actor: &Session, c: &Command) -> Result<Outcome> {
    ensure!(
        c.request_id.starts_with("RUST-") && c.request_id.len() == 37,
        "Invalid hold request ID"
    );
    let hash = crate::auth::fingerprint(&serde_json::to_vec(c)?);
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    sqlx::query("SET LOCAL lock_timeout='8s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,721008))")
        .bind(&c.request_id)
        .execute(&mut *tx)
        .await?;
    let prior: Option<(String, String, String)> = sqlx::query_as(
        "SELECT username,payload_hash,result_json FROM rust_held_requests WHERE request_id=$1",
    )
    .bind(&c.request_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((who, h, json)) = prior {
        ensure!(who == actor.username() && h == hash, "Hold request changed");
        let out: Option<Outcome> = serde_json::from_str(&json)?;
        let out = out.context("Hold request was cancelled")?;
        tx.commit().await?;
        return Ok(out);
    }
    let out = match &c.action {
        Action::Save { hold, return_token } => {
            validate(hold)?;
            if let Some(token) = return_token {
                lock_session(&mut tx, actor, token).await?;
            }
            let customer_name = if let Some(id) = hold.customer_id {
                sqlx::query_scalar::<_, String>(
                    "SELECT COALESCE(name,'') FROM customers WHERE id=$1",
                )
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .context("Customer no longer exists")?
            } else {
                hold.customer_name.clone()
            };
            let no = format!("HOLD-{}", c.request_id.trim_start_matches("RUST-"));
            let id:i32=sqlx::query_scalar("INSERT INTO held_sales(hold_no,cart_json,customer_id,customer_name,payment_type,note,total_amount,item_count) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
                .bind(no).bind(&hold.cart_json).bind(hold.customer_id).bind(customer_name).bind(&hold.payment_type).bind(&hold.note).bind(hold.total_amount).bind(hold.item_count).fetch_one(&mut *tx).await?;
            if let Some(token) = return_token {
                sqlx::query("UPDATE rust_held_sessions SET state='returned',updated_at=CURRENT_TIMESTAMP WHERE token=$1").bind(token).execute(&mut *tx).await?;
            }
            let hold = sqlx::query_as(&format!("{SELECT} WHERE id=$1"))
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            Outcome { hold, token: None }
        }
        Action::Resume { id } => {
            let hold: Hold = sqlx::query_as(&format!("{SELECT} WHERE id=$1 FOR UPDATE"))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .context("This hold was already resumed; refresh the list")?;
            validate(&hold)?;
            sqlx::query(
                "INSERT INTO rust_held_sessions(token,username,held_id,body) VALUES($1,$2,$3,$4)",
            )
            .bind(&c.request_id)
            .bind(actor.username())
            .bind(id)
            .bind(serde_json::to_string(&hold)?)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM held_sales WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            Outcome {
                hold,
                token: Some(c.request_id.clone()),
            }
        }
    };
    sqlx::query("INSERT INTO rust_held_requests(request_id,username,payload_hash,result_json) VALUES($1,$2,$3,$4)").bind(&c.request_id).bind(actor.username()).bind(hash).bind(serde_json::to_string(&out)?).execute(&mut *tx).await?;
    crate::activity::record(&mut tx,actor,if out.token.is_some(){"rust.held.resume"}else{"rust.held.save"},&format!("hold_id={}; request={}",out.hold.id,c.request_id)).await?;
    tx.commit().await?;
    Ok(out)
}
pub async fn cancel_pending(pool: &PgPool, actor: &Session, c: &Command) -> Result<bool> {
    ensure!(
        c.request_id.starts_with("RUST-") && c.request_id.len() == 37,
        "Invalid hold request ID"
    );
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    sqlx::query("SET LOCAL lock_timeout='8s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,721008))")
        .bind(&c.request_id)
        .execute(&mut *tx)
        .await?;
    let hash = crate::auth::fingerprint(&serde_json::to_vec(c)?);
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT username,payload_hash,result_json FROM rust_held_requests WHERE request_id=$1",
    )
    .bind(&c.request_id)
    .fetch_optional(&mut *tx)
    .await?;
    let committed = if let Some((user, h, result)) = row {
        ensure!(
            user == actor.username() && h == hash,
            "Hold request changed"
        );
        result != "null"
    } else {
        sqlx::query("INSERT INTO rust_held_requests(request_id,username,payload_hash,result_json) VALUES($1,$2,$3,'null')").bind(&c.request_id).bind(actor.username()).bind(hash).execute(&mut *tx).await?;
        false
    };
    tx.commit().await?;
    Ok(committed)
}
pub(crate) async fn lock_session(
    conn: &mut PgConnection,
    actor: &Session,
    token: &str,
) -> Result<()> {
    let (who, state): (String, String) =
        sqlx::query_as("SELECT username,state FROM rust_held_sessions WHERE token=$1 FOR UPDATE")
            .bind(token)
            .fetch_optional(conn)
            .await?
            .context("Held session is unavailable")?;
    ensure!(
        who == actor.username() && state == "active",
        "This held sale is no longer active for this operator"
    );
    Ok(())
}
pub(crate) async fn finish(conn: &mut PgConnection, token: &str, sale: i32) -> Result<()> {
    ensure!(sqlx::query("UPDATE rust_held_sessions SET state='completed',sale_id=$1,updated_at=CURRENT_TIMESTAMP WHERE token=$2 AND state='active'").bind(sale).bind(token).execute(conn).await?.rows_affected()==1,"Held sale was already resolved");
    Ok(())
}
