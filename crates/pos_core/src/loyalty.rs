use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use sqlx::{PgConnection, PgPool};

pub fn earned(total: Decimal, rate: &str) -> Result<i32> {
    let rate: Decimal = rate.parse().context("Invalid loyalty earning rate")?;
    ensure!(
        rate >= Decimal::ZERO && rate <= Decimal::from(1_000_000),
        "Invalid loyalty earning rate"
    );
    ensure!(total >= Decimal::ZERO, "Invalid loyalty sale total");
    total
        .checked_mul(rate)
        .context("Loyalty points exceed supported limit")?
        .floor()
        .to_i32()
        .context("Loyalty points exceed supported limit")
}
pub(crate) async fn award(
    conn: &mut PgConnection,
    sale_id: i32,
    invoice: &str,
    customer: Option<i32>,
    payment: &str,
    total: f64,
) -> Result<()> {
    let Some(customer) = customer else {
        return Ok(());
    };
    if payment.eq_ignore_ascii_case("credit") {
        return Ok(());
    }
    let rate: Option<Option<String>> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key='loyalty_points_per_dollar'")
            .fetch_optional(&mut *conn)
            .await?;
    let amount = earned(
        Decimal::from_f64_retain(total)
            .context("Invalid loyalty sale total")?
            .round_dp(2),
        &rate.flatten().unwrap_or_else(|| "0".into()),
    )?;
    if amount == 0 {
        return Ok(());
    }
    let months: Option<Option<String>> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key='points_expiry_months'")
            .fetch_optional(&mut *conn)
            .await?;
    let months: i64 = months
        .flatten()
        .unwrap_or_else(|| "12".into())
        .parse()
        .context("Invalid points expiry setting")?;
    ensure!(
        (1..=120).contains(&months),
        "Points expiry must be 1 to 120 months"
    );
    let balance: i32 =
        sqlx::query_scalar("SELECT COALESCE(points,0) FROM customers WHERE id=$1 FOR UPDATE")
            .bind(customer)
            .fetch_one(&mut *conn)
            .await?;
    let next = balance
        .checked_add(amount)
        .context("Points balance exceeds supported limit")?;
    let expiry = (chrono::Local::now().date_naive() + chrono::Duration::days(months * 30))
        .format("%Y-%m-%d")
        .to_string();
    sqlx::query("INSERT INTO rust_sale_points(sale_id,customer_id,earned) VALUES($1,$2,$3)")
        .bind(sale_id)
        .bind(customer)
        .bind(amount)
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT INTO customer_points_log(customer_id,points,type,reference,expiry_date) VALUES($1,$2,'earn',$3,$4)").bind(customer).bind(amount).bind(invoice).bind(expiry).execute(&mut *conn).await?;
    sqlx::query("UPDATE customers SET points=$1 WHERE id=$2")
        .bind(next)
        .bind(customer)
        .execute(conn)
        .await?;
    Ok(())
}
pub(crate) async fn refund(
    conn: &mut PgConnection,
    sale: i32,
    customer: Option<i32>,
    invoice: &str,
) -> Result<()> {
    let Some(customer) = customer else {
        return Ok(());
    };
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('rust_sale_points') IS NOT NULL")
        .fetch_one(&mut *conn)
        .await?;
    if !exists {
        return Ok(());
    }
    // Same customer-before-stock order as checkout; reverse the recorded award.
    sqlx::query("SELECT id FROM customers WHERE id=$1 FOR UPDATE")
        .bind(customer)
        .fetch_one(&mut *conn)
        .await?;
    let row: Option<(i32, i32, bool)> = sqlx::query_as(
        "SELECT customer_id,earned,refunded FROM rust_sale_points WHERE sale_id=$1 FOR UPDATE",
    )
    .bind(sale)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((owner, earned, refunded)) = row {
        ensure!(
            owner == customer && !refunded,
            "Loyalty refund does not match the receipt"
        );
        sqlx::query("UPDATE customers SET points=COALESCE(points,0)-$1 WHERE id=$2")
            .bind(earned)
            .bind(customer)
            .execute(&mut *conn)
            .await?;
        sqlx::query("INSERT INTO customer_points_log(customer_id,points,type,reference) VALUES($1,$2,'refund',$3)").bind(customer).bind(-earned).bind(invoice).execute(&mut *conn).await?;
        sqlx::query("UPDATE rust_sale_points SET refunded=TRUE WHERE sale_id=$1")
            .bind(sale)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct Entry {
    pub id: i32,
    pub points: i32,
    pub kind: String,
    pub reference: String,
    pub expiry: String,
    pub date: String,
}
pub async fn history(pool: &PgPool, actor: &Session, customer: i32) -> Result<(i32, Vec<Entry>)> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let points = sqlx::query_scalar("SELECT COALESCE(points,0) FROM customers WHERE id=$1")
        .bind(customer)
        .fetch_one(&mut *tx)
        .await?;
    let rows=sqlx::query_as("SELECT id,points,type AS kind,COALESCE(reference,'') AS reference,COALESCE(expiry_date,'') AS expiry,created_at::text AS date FROM customer_points_log WHERE customer_id=$1 ORDER BY created_at DESC,id DESC").bind(customer).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok((points, rows))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn points_round_down() {
        assert_eq!(earned("1250.99".parse().unwrap(), "0.01").unwrap(), 12);
        assert!(earned(100.into(), "-1").is_err());
        assert!(earned(100.into(), "NaN").is_err());
    }
}
