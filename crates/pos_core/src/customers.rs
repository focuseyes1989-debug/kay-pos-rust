use anyhow::{ensure, Context, Result};
use sqlx::PgPool;

#[derive(Clone, Debug, Default, PartialEq, sqlx::FromRow)]
pub struct Customer {
    pub id: i32,
    pub name: String,
    pub phone: String,
    pub email: String,
    pub address: String,
    pub remarks: String,
    pub total_visit: i32,
    pub total_spent: f64,
    pub points: i32,
    pub credit_limit: f64,
    pub current_balance: f64,
}

pub async fn list(pool: &PgPool) -> Result<Vec<Customer>> {
    sqlx::query_as("SELECT id, name, COALESCE(phone,'') AS phone, COALESCE(email,'') AS email, COALESCE(address,'') AS address, COALESCE(to_jsonb(c)->>'remarks','') AS remarks, COALESCE(total_visit,0)::int AS total_visit, COALESCE(total_spent,0)::float8 AS total_spent, COALESCE(points,0)::int AS points, COALESCE(credit_limit,0)::float8 AS credit_limit, COALESCE(current_balance,0)::float8 AS current_balance FROM customers c ORDER BY lower(name), id")
        .fetch_all(pool).await.context("Could not load customers")
}

pub async fn save(pool: &PgPool, customer: &Customer) -> Result<()> {
    ensure!(
        !customer.name.trim().is_empty(),
        "Customer name is required"
    );
    ensure!(
        customer.credit_limit.is_finite() && customer.credit_limit >= 0.0,
        "Credit limit must be a non-negative number"
    );
    let result = if customer.id == 0 {
        sqlx::query("INSERT INTO customers (name, phone, email, address, remarks, credit_limit, total_visit, total_spent, points, current_balance) VALUES ($1,$2,$3,$4,$5,$6,0,0,0,0)")
            .bind(customer.name.trim()).bind(customer.phone.trim()).bind(customer.email.trim()).bind(customer.address.trim()).bind(&customer.remarks).bind(customer.credit_limit).execute(pool).await
    } else {
        sqlx::query("UPDATE customers SET name=$1, phone=$2, email=$3, address=$4, remarks=$5, credit_limit=$6 WHERE id=$7")
            .bind(customer.name.trim()).bind(customer.phone.trim()).bind(customer.email.trim()).bind(customer.address.trim()).bind(&customer.remarks).bind(customer.credit_limit).bind(customer.id).execute(pool).await
    }.context("Could not save customer")?;
    ensure!(
        result.rows_affected() == 1,
        "Customer no longer exists. Refresh the list."
    );
    Ok(())
}
