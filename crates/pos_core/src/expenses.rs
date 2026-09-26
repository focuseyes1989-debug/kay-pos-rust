use anyhow::{ensure, Result};
use rust_decimal::Decimal;
use sqlx::PgPool;

#[derive(Clone, Debug, Default, PartialEq, sqlx::FromRow)]
pub struct Expense {
    pub id: i32,
    pub expense_no: String,
    pub category: String,
    pub description: String,
    pub amount: Decimal,
    pub expense_date: String,
    pub payment_method: String,
    pub reference_no: String,
    pub notes: String,
}

pub async fn list(pool: &PgPool, actor:&crate::auth::Session) -> Result<Vec<Expense>> {
    let mut tx=pool.begin().await?;
    actor.authorize(&mut tx,crate::auth::Permission::Manage).await?;
    let rows=sqlx::query_as("SELECT id, COALESCE(expense_no,'') AS expense_no, category, COALESCE(description,'') AS description, COALESCE(amount,0)::numeric AS amount, COALESCE(expense_date,'') AS expense_date, COALESCE(payment_method,'') AS payment_method, COALESCE(reference_no,'') AS reference_no, COALESCE(notes,'') AS notes FROM expenses ORDER BY expense_date DESC, id DESC").fetch_all(&mut *tx).await?;
    tx.commit().await?;Ok(rows)
}

pub async fn categories(pool: &PgPool) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT name FROM expense_categories WHERE COALESCE(is_active,1)=1 ORDER BY name",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn delete(pool: &PgPool, id: i32,actor:&crate::auth::Session) -> Result<()> {
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx,crate::auth::Permission::Manage).await?;
    sqlx::query("SELECT id FROM expenses WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let payroll_exists: bool = sqlx::query_scalar("SELECT to_regclass('payrolls') IS NOT NULL")
        .fetch_one(&mut *tx)
        .await?;
    if payroll_exists {
        let linked: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payrolls WHERE expense_id=$1)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            !linked,
            "This expense belongs to payroll and cannot be deleted here"
        );
    }
    sqlx::query("DELETE FROM expense_attachments WHERE expense_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM expenses WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    crate::activity::record(&mut tx,actor,"rust.expense.delete",&format!("expense_id={id}")).await?;
    tx.commit().await?;
    Ok(())
}

pub fn validate(expense: &Expense) -> Result<()> {
    ensure!(
        !expense.category.trim().is_empty(),
        "Select an expense category"
    );
    ensure!(
        expense.amount > Decimal::ZERO && expense.amount.scale() <= 2,
        "Amount must be greater than zero with at most two decimal places"
    );
    chrono::NaiveDate::parse_from_str(&expense.expense_date, "%Y-%m-%d")?;
    ensure!(
        !expense.payment_method.trim().is_empty(),
        "Select a payment method"
    );
    Ok(())
}

pub async fn save(pool: &PgPool, expense: &Expense,actor:&crate::auth::Session) -> Result<()> {
    validate(expense)?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx,crate::auth::Permission::Manage).await?;
    if expense.id!=0 {
        sqlx::query("SELECT id FROM expenses WHERE id=$1 FOR UPDATE").bind(expense.id).fetch_one(&mut *tx).await?;
        let has_payroll:bool=sqlx::query_scalar("SELECT to_regclass('payrolls') IS NOT NULL").fetch_one(&mut *tx).await?;
        if has_payroll {
            let linked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payrolls WHERE expense_id=$1)").bind(expense.id).fetch_one(&mut *tx).await?;
            ensure!(!linked,"Edit payroll expenses through Payroll, not Expenses");
        }
    }
    let valid: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM expense_categories WHERE name=$1)")
            .bind(&expense.category)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(valid, "Expense category no longer exists");
    let result = if expense.id == 0 {
        sqlx::query("INSERT INTO expenses (expense_no,category,description,amount,expense_date,payment_method,reference_no,notes) VALUES ($1,$2,$3,$4::numeric,$5,$6,$7,$8)")
            .bind(&expense.expense_no).bind(&expense.category).bind(&expense.description).bind(expense.amount).bind(&expense.expense_date).bind(&expense.payment_method).bind(&expense.reference_no).bind(&expense.notes).execute(&mut *tx).await?
    } else {
        sqlx::query("UPDATE expenses SET category=$2,description=$3,amount=$4::numeric,expense_date=$5,payment_method=$6,reference_no=$7,notes=$8 WHERE id=$1")
            .bind(expense.id).bind(&expense.category).bind(&expense.description).bind(expense.amount).bind(&expense.expense_date).bind(&expense.payment_method).bind(&expense.reference_no).bind(&expense.notes).execute(&mut *tx).await?
    };
    ensure!(
        result.rows_affected() == 1,
        "Expense no longer exists. Refresh the list."
    );
    crate::activity::record(&mut tx,actor,"rust.expense.save",&format!("expense_id={}; expense_no={}",expense.id,expense.expense_no)).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expense_validation() {
        let mut e = Expense {
            category: "Rent".into(),
            amount: Decimal::from(100),
            expense_date: "2026-09-16".into(),
            payment_method: "Cash".into(),
            ..Default::default()
        };
        assert!(validate(&e).is_ok());
        e.amount = Decimal::ZERO;
        assert!(validate(&e).is_err());
        e.amount = Decimal::from(1);
        e.expense_date = "2026-02-30".into();
        assert!(validate(&e).is_err());
    }
}
