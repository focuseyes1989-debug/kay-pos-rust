use anyhow::{ensure, Context, Result};
use rust_decimal::Decimal;
use sqlx::PgPool;

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct CreditRow {
    pub id: i32,
    pub customer: String,
    pub invoice: String,
    pub date: String,
    pub due: String,
    pub total: Decimal,
    pub paid: Decimal,
    pub balance: Decimal,
    pub status: String,
}

pub async fn invoices(pool: &PgPool, customer_id: Option<i32>) -> Result<Vec<CreditRow>> {
    Ok(sqlx::query_as("SELECT cs.id, c.name AS customer, cs.invoice_no AS invoice, cs.sale_date AS date, COALESCE(cs.due_date,'') AS due, cs.total_amount::numeric AS total, COALESCE(cs.paid_amount,0)::numeric AS paid, cs.balance_amount::numeric AS balance, COALESCE(cs.status,'pending') AS status FROM credit_sales cs JOIN customers c ON c.id=cs.customer_id WHERE ($1::int IS NULL OR cs.customer_id=$1) ORDER BY COALESCE(NULLIF(cs.due_date,''),cs.sale_date), cs.id")
        .bind(customer_id).fetch_all(pool).await?)
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct PaymentRow {
    pub date: String,
    pub invoice: String,
    pub amount: Decimal,
    pub method: String,
    pub reference: String,
    pub note: String,
}

pub async fn payments(pool: &PgPool, customer_id: i32) -> Result<Vec<PaymentRow>> {
    let mut rows: Vec<PaymentRow> = sqlx::query_as("SELECT cp.payment_date AS date, cs.invoice_no AS invoice, cp.amount::numeric AS amount, COALESCE(cp.payment_method,'') AS method, COALESCE(cp.reference_no,'') AS reference, COALESCE(cp.note,'') AS note FROM credit_payments cp JOIN credit_sales cs ON cs.id=cp.credit_sale_id WHERE cp.customer_id=$1 ORDER BY cp.payment_date, cp.id").bind(customer_id).fetch_all(pool).await?;
    for (table, kind) in [
        ("credit_adjustments", "Adjustment"),
        ("credit_writeoffs", "Write-off"),
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(pool)
            .await?;
        if exists {
            // These identifiers come only from the fixed table list above. JSON keys support legacy schemas.
            let query = format!("SELECT COALESCE(j->>'adjustment_date',j->>'writeoff_date',j->>'created_at','') AS date, '' AS invoice, COALESCE((j->>'amount')::numeric,0) AS amount, $2 || COALESCE(' - ' || (j->>'adjustment_type'),'') AS method, COALESCE(j->>'reference_no',j->>'reference','') AS reference, COALESCE(j->>'reason',j->>'note',j->>'description','') AS note FROM (SELECT to_jsonb(t) AS j FROM {table} t WHERE customer_id=$1) data");
            rows.extend(
                sqlx::query_as::<_, PaymentRow>(&query)
                    .bind(customer_id)
                    .bind(kind)
                    .fetch_all(pool)
                    .await?,
            );
        }
    }
    rows.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(rows)
}

pub fn amount(value: &str) -> Result<Decimal> {
    let number: Decimal = value.trim().parse().context("Enter a valid amount")?;
    ensure!(
        number >= Decimal::ZERO && number.scale() <= 2,
        "Amounts must be non-negative with at most two decimal places"
    );
    Ok(number)
}

fn valid_date(date: &str) -> Result<()> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").context("Enter a valid date")?;
    Ok(())
}

pub async fn create(
    pool: &PgPool,
    actor: &crate::auth::Session,
    customer: i32,
    invoice: &str,
    total: &str,
    paid: &str,
    date: &str,
    due: &str,
    note: &str,
) -> Result<()> {
    let total = amount(total)?;
    let paid = amount(paid)?;
    ensure!(
        total > Decimal::ZERO && paid <= total,
        "Paid amount must not exceed a positive sale total"
    );
    ensure!(!invoice.trim().is_empty(), "Invoice is required");
    valid_date(date)?;
    valid_date(due)?;
    ensure!(due >= date, "Due date must not precede sale date");
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx,crate::auth::Permission::Manage).await?;
    let (limit, balance): (Decimal, Decimal) = sqlx::query_as("SELECT COALESCE(credit_limit,0)::numeric, COALESCE(current_balance,0)::numeric FROM customers WHERE id=$1 FOR UPDATE").bind(customer).fetch_one(&mut *tx).await?;
    let enabled: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key='credit_limit_enabled'")
            .fetch_optional(&mut *tx)
            .await?;
    let enforce = enabled
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(true);
    let remaining = total - paid;
    ensure!(
        !enforce || limit <= Decimal::ZERO || balance + remaining <= limit,
        "Customer credit limit would be exceeded"
    );
    let status = if remaining == Decimal::ZERO {
        "paid"
    } else if paid > Decimal::ZERO {
        "partial"
    } else {
        "pending"
    };
    sqlx::query("INSERT INTO credit_sales (invoice_no,customer_id,total_amount,paid_amount,balance_amount,sale_date,due_date,status,notes) VALUES ($1,$2,$3::numeric,$4::numeric,$5::numeric,$6,$7,$8,$9)")
        .bind(invoice.trim()).bind(customer).bind(total).bind(paid).bind(remaining).bind(date).bind(due).bind(status).bind(note).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE customers SET current_balance=COALESCE(current_balance,0)+$1::numeric WHERE id=$2",
    )
    .bind(remaining)
    .bind(customer)
    .execute(&mut *tx)
    .await?;
    crate::activity::record(&mut tx,actor,"rust.credit.create",&format!("customer_id={customer}; invoice={invoice}")).await?;
    tx.commit().await?;
    Ok(())
}

pub fn allocate(amount: Decimal, balances: &[(i32, Decimal)]) -> Result<Vec<(i32, Decimal)>> {
    ensure!(amount > Decimal::ZERO, "Payment must be greater than zero");
    let total: Decimal = balances.iter().map(|(_, b)| *b).sum();
    ensure!(amount <= total, "Payment exceeds outstanding balance");
    let mut remaining = amount;
    let mut result = Vec::new();
    for (id, balance) in balances {
        let applied = remaining.min(*balance);
        if applied > Decimal::ZERO {
            result.push((*id, applied));
            remaining -= applied;
        }
    }
    Ok(result)
}

pub async fn collect(
    pool: &PgPool,
    actor: &crate::auth::Session,
    customer: i32,
    invoice: Option<i32>,
    value: &str,
    date: &str,
    method: &str,
    reference: &str,
    note: &str,
) -> Result<()> {
    let amount = amount(value)?;
    valid_date(date)?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx,crate::auth::Permission::Manage).await?;
    sqlx::query("SELECT id FROM customers WHERE id=$1 FOR UPDATE")
        .bind(customer)
        .fetch_one(&mut *tx)
        .await?;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM payment_types WHERE name=$1 AND COALESCE(active,1)=1)",
    )
    .bind(method)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(valid, "Select an active payment method");
    let balances: Vec<(i32,Decimal)> = sqlx::query_as("SELECT id,balance_amount::numeric FROM credit_sales WHERE customer_id=$1 AND ($2::int IS NULL OR id=$2) AND balance_amount>0 AND status NOT IN ('paid','refunded') ORDER BY COALESCE(NULLIF(due_date,''),sale_date),id FOR UPDATE")
        .bind(customer).bind(invoice).fetch_all(&mut *tx).await?;
    for (id, applied) in allocate(amount, &balances)? {
        sqlx::query("UPDATE credit_sales SET paid_amount=COALESCE(paid_amount,0)+$1::numeric, balance_amount=balance_amount-$1::numeric, status=CASE WHEN balance_amount::numeric-$1::numeric<=0 THEN 'paid' ELSE 'partial' END WHERE id=$2").bind(applied).bind(id).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO credit_payments (credit_sale_id,customer_id,amount,payment_date,payment_method,reference_no,note) VALUES ($1,$2,$3::numeric,$4,$5,$6,$7)").bind(id).bind(customer).bind(applied).bind(date).bind(method).bind(reference).bind(note).execute(&mut *tx).await?;
    }
    sqlx::query(
        "UPDATE customers SET current_balance=COALESCE(current_balance,0)-$1::numeric WHERE id=$2",
    )
    .bind(amount)
    .bind(customer)
    .execute(&mut *tx)
    .await?;
    crate::activity::record(&mut tx,actor,"rust.credit.collect",&format!("customer_id={customer}; invoice_id={invoice:?}; amount={amount}")).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, customer: i32, username: &str, password: &str) -> Result<()> {
    use subtle::ConstantTimeEq;
    let mut tx = pool.begin().await?;
    let user: Option<(String,String)> = sqlx::query_as("SELECT password_hash,salt FROM users WHERE username=$1 AND lower(role)='admin' AND is_active=1 FOR UPDATE").bind(username).fetch_optional(&mut *tx).await?;
    let (hash, salt) = user.context("Active administrator credentials required")?;
    let expected = hex::decode(hash)?;
    let salt = hex::decode(salt)?;
    let mut actual = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, 100000, &mut actual);
    ensure!(
        bool::from(actual.as_slice().ct_eq(&expected)),
        "Invalid administrator credentials"
    );
    let balance: f64 = sqlx::query_scalar(
        "SELECT COALESCE(current_balance,0)::float8 FROM customers WHERE id=$1 FOR UPDATE",
    )
    .bind(customer)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        balance == 0.0,
        "Settle this customer's balance before deleting"
    );
    let linked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM credit_sales WHERE customer_id=$1 UNION ALL SELECT 1 FROM sales WHERE customer_id=$1)").bind(customer).fetch_one(&mut *tx).await?;
    ensure!(
        !linked,
        "Customer has transaction history and cannot be deleted"
    );
    sqlx::query("DELETE FROM customers WHERE id=$1")
        .bind(customer)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_money() {
        for value in ["-1", "NaN", "1.001", ""] {
            assert!(amount(value).is_err());
        }
        assert_eq!(amount("10.25").unwrap().to_string(), "10.25");
    }
    #[test]
    fn allocates_oldest_and_rejects_overpayment() {
        let invoices = vec![(1, Decimal::from(30)), (2, Decimal::from(70))];
        assert_eq!(
            allocate(Decimal::from(50), &invoices).unwrap(),
            vec![(1, Decimal::from(30)), (2, Decimal::from(20))]
        );
        assert!(allocate(Decimal::from(101), &invoices).is_err());
        assert!(allocate(Decimal::ZERO, &invoices).is_err());
    }
}
