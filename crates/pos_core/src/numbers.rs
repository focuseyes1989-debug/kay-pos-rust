use anyhow::{Context, Result};
use sqlx::PgConnection;

pub async fn invoice(connection: &mut PgConnection) -> Result<String> {
    loop {
        let n: i64 = sqlx::query_scalar("SELECT nextval('rust_invoice_number_seq')")
            .fetch_one(&mut *connection).await
            .context("Apply migrations/002_short_numbers.sql before using short numbers")?;
        let number = format!("INV{n:06}");
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sales WHERE invoice_no=$1)")
            .bind(&number).fetch_one(&mut *connection).await?;
        if !used { return Ok(number); }
    }
}

pub async fn batch(connection: &mut PgConnection) -> Result<String> {
    loop {
        let n: i64 = sqlx::query_scalar("SELECT nextval('rust_batch_number_seq')")
            .fetch_one(&mut *connection).await
            .context("Apply migrations/002_short_numbers.sql before using short numbers")?;
        let number = format!("B{n:06}");
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_locations WHERE batch_no=$1 UNION ALL SELECT 1 FROM variant_stock_batches WHERE batch_no=$1)")
            .bind(&number).fetch_one(&mut *connection).await?;
        if !used { return Ok(number); }
    }
}
