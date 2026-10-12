use crate::storage as sqlx;
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JournalLine {
    pub account_key: String,
    pub account_code: String,
    pub account_name: String,
    pub debit: Decimal,
    pub credit: Decimal,
    pub memo: String,
}

async fn account(conn: &mut PgConnection, key: &str) -> Result<(String, String)> {
    if let Some(row) = sqlx::query_as(
        "SELECT account_code,account_name FROM rust_erp_account_mappings WHERE mapping_key=$1",
    )
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?
    {
        return Ok(row);
    }
    let fallback = key.split(':').next().unwrap_or(key);
    sqlx::query_as(
        "SELECT account_code,account_name FROM rust_erp_account_mappings WHERE mapping_key=$1",
    )
    .bind(fallback)
    .fetch_optional(&mut *conn)
    .await?
    .with_context(|| format!("Missing ERP account mapping for {key}"))
}

async fn line(
    conn: &mut PgConnection,
    key: &str,
    debit: Decimal,
    credit: Decimal,
    memo: impl Into<String>,
) -> Result<JournalLine> {
    let (account_code, account_name) = account(conn, key).await?;
    Ok(JournalLine {
        account_key: key.into(),
        account_code,
        account_name,
        debit: debit.round_dp(2),
        credit: credit.round_dp(2),
        memo: memo.into(),
    })
}

async fn document(
    conn: &mut PgConnection,
    event_key: String,
    event_type: &str,
    source_type: &str,
    source_id: i32,
    date: &str,
    lines: Vec<JournalLine>,
) -> Result<()> {
    let debit: Decimal = lines.iter().map(|v| v.debit).sum();
    let credit: Decimal = lines.iter().map(|v| v.credit).sum();
    anyhow::ensure!(
        !lines.is_empty() && debit == credit && debit >= Decimal::ZERO,
        "Accounting journal is not balanced"
    );
    anyhow::ensure!(
        lines.iter().all(|v| v.debit >= Decimal::ZERO
            && v.credit >= Decimal::ZERO
            && (v.debit.is_zero() || v.credit.is_zero())),
        "Accounting journal contains an invalid line"
    );
    let currency = sqlx::query_scalar::<_, Option<String>>(
        "SELECT value FROM settings WHERE key='currency_code'",
    )
    .fetch_optional(&mut *conn)
    .await?
    .flatten()
    .filter(|v| !v.trim().is_empty())
    .unwrap_or_else(|| "MMK".into());
    let payload = serde_json::json!({
        "schema_version": 1,
        "event_type": event_type,
        "source_type": source_type,
        "source_id": source_id,
        "document_date": date,
        "currency": currency,
        "total_debit": debit,
        "total_credit": credit,
        "lines": lines
    });
    let id: i64 = sqlx::query_scalar("INSERT INTO rust_erp_accounting_documents(event_key,event_type,source_type,source_id,document_date,currency,total_debit,total_credit,lines) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(event_key) DO UPDATE SET event_key=EXCLUDED.event_key RETURNING id")
        .bind(&event_key).bind(event_type).bind(source_type).bind(source_id).bind(date).bind(&currency).bind(debit).bind(credit).bind(serde_json::to_value(&lines)?).fetch_one(&mut *conn).await
        .context("ERP accounting schema unavailable; apply migrations/013_erp_accounting.sql")?;
    sqlx::query("INSERT INTO rust_erp_outbox(event_key,event_type,aggregate_type,aggregate_id,payload) VALUES($1,'accounting.journal.created','accounting_document',$2,$3) ON CONFLICT(event_key) DO NOTHING")
        .bind(format!("accounting.journal:{event_key}")).bind(id.to_string()).bind(payload).execute(&mut *conn).await
        .context("ERP outbox unavailable; apply migrations/010_erp_outbox.sql")?;
    Ok(())
}

pub async fn sale(conn: &mut PgConnection, sale_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let (total,payment,discount,cogs,payment_type,date):(Decimal,Decimal,Decimal,Decimal,String,String)=sqlx::query_as("SELECT total::numeric,payment::numeric,COALESCE(discount_amount,0)::numeric,COALESCE(cogs,0)::numeric,COALESCE(payment_type,''),created_at::date::text FROM sales WHERE id=$1")
        .bind(sale_id).fetch_one(&mut *conn).await?;
    let subtotal: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total::numeric),0) FROM sale_items WHERE sale_id=$1",
    )
    .bind(sale_id)
    .fetch_one(&mut *conn)
    .await?;
    let tax = (total - (subtotal - discount))
        .max(Decimal::ZERO)
        .round_dp(2);
    let mut lines = vec![];
    if payment_type.eq_ignore_ascii_case("credit") {
        let paid = payment.min(total).max(Decimal::ZERO).round_dp(2);
        if !paid.is_zero() {
            lines.push(line(conn, "cash", paid, Decimal::ZERO, "Credit sale deposit").await?);
        }
        let balance = total - paid;
        if !balance.is_zero() {
            lines.push(
                line(
                    conn,
                    "accounts_receivable",
                    balance,
                    Decimal::ZERO,
                    "Customer receivable",
                )
                .await?,
            );
        }
    } else {
        lines.push(
            line(
                conn,
                &format!("cash:{payment_type}"),
                total,
                Decimal::ZERO,
                "Sale payment",
            )
            .await?,
        );
    }
    if !discount.is_zero() {
        lines.push(
            line(
                conn,
                "sales_discount",
                discount,
                Decimal::ZERO,
                "Sale discount",
            )
            .await?,
        );
    }
    lines.push(
        line(
            conn,
            "sales_revenue",
            Decimal::ZERO,
            subtotal,
            "Sale revenue",
        )
        .await?,
    );
    if !tax.is_zero() {
        lines.push(line(conn, "tax_payable", Decimal::ZERO, tax, "Sales tax").await?);
    }
    if !cogs.is_zero() {
        lines.push(line(conn, "cogs", cogs, Decimal::ZERO, "Cost of goods sold").await?);
        lines.push(line(conn, "inventory", Decimal::ZERO, cogs, "Inventory issued").await?);
    }
    document(
        conn,
        format!("sale:{sale_id}"),
        "sale.completed",
        "sale",
        sale_id,
        &date,
        lines,
    )
    .await
}

pub async fn refund(conn: &mut PgConnection, sale_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let original_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM rust_erp_accounting_documents WHERE event_key=$1)",
    )
    .bind(format!("sale:{sale_id}"))
    .fetch_one(&mut *conn)
    .await?;
    if !original_exists {
        // Establish a baseline for sales completed before Phase 4, then reverse it.
        sale(conn, sale_id).await?;
    }
    let (date, original): (String, serde_json::Value) = sqlx::query_as(
        "SELECT CURRENT_DATE::text,lines FROM rust_erp_accounting_documents WHERE event_key=$1",
    )
    .bind(format!("sale:{sale_id}"))
    .fetch_optional(&mut *conn)
    .await?
    .context("Original sale accounting journal is missing")?;
    let source: Vec<JournalLine> = serde_json::from_value(original)?;
    let reversed = source
        .into_iter()
        .map(|v| JournalLine {
            debit: v.credit,
            credit: v.debit,
            memo: format!("Refund reversal: {}", v.memo),
            ..v
        })
        .collect();
    document(
        conn,
        format!("sale-refund:{sale_id}"),
        "sale.refunded",
        "sale",
        sale_id,
        &date,
        reversed,
    )
    .await
}

pub async fn purchase_receipt(conn: &mut PgConnection, draft_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let (po,total,date):(i32,Decimal,String)=sqlx::query_as("SELECT p.id,p.total_amount::numeric,p.order_date FROM rust_purchase_drafts d JOIN purchase_orders p ON p.id=d.po_id WHERE d.id=$1")
        .bind(draft_id).fetch_one(&mut *conn).await?;
    let lines = vec![
        line(conn, "inventory", total, Decimal::ZERO, "Goods received").await?,
        line(
            conn,
            "accounts_payable",
            Decimal::ZERO,
            total,
            "Supplier payable",
        )
        .await?,
    ];
    document(
        conn,
        format!("purchase-receipt:{po}"),
        "purchase.goods_received",
        "purchase_order",
        po,
        &date,
        lines,
    )
    .await
}

pub async fn supplier_payment(conn: &mut PgConnection, payment_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let (amount,date,method):(Decimal,String,String)=sqlx::query_as("SELECT amount::numeric,payment_date,COALESCE(payment_type,'Cash') FROM supplier_payments WHERE id=$1")
        .bind(payment_id).fetch_one(&mut *conn).await?;
    let lines = vec![
        line(
            conn,
            "accounts_payable",
            amount,
            Decimal::ZERO,
            "Supplier payment",
        )
        .await?,
        line(
            conn,
            &format!("cash:{method}"),
            Decimal::ZERO,
            amount,
            "Payment account",
        )
        .await?,
    ];
    document(
        conn,
        format!("supplier-payment:{payment_id}"),
        "supplier.payment_recorded",
        "supplier_payment",
        payment_id,
        &date,
        lines,
    )
    .await
}

pub async fn customer_payment(conn: &mut PgConnection, payment_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let (amount,date,method):(Decimal,String,String)=sqlx::query_as("SELECT amount::numeric,payment_date,COALESCE(payment_method,'Cash') FROM credit_payments WHERE id=$1")
        .bind(payment_id).fetch_one(&mut *conn).await?;
    let absolute = amount.abs();
    let lines = if amount >= Decimal::ZERO {
        vec![
            line(
                conn,
                &format!("cash:{method}"),
                absolute,
                Decimal::ZERO,
                "Customer credit collection",
            )
            .await?,
            line(
                conn,
                "accounts_receivable",
                Decimal::ZERO,
                absolute,
                "Receivable collected",
            )
            .await?,
        ]
    } else {
        vec![
            line(
                conn,
                "accounts_receivable",
                absolute,
                Decimal::ZERO,
                "Refunded customer collection",
            )
            .await?,
            line(
                conn,
                &format!("cash:{method}"),
                Decimal::ZERO,
                absolute,
                "Customer payment refund",
            )
            .await?,
        ]
    };
    document(
        conn,
        format!("customer-payment:{payment_id}"),
        "customer.payment_recorded",
        "customer_payment",
        payment_id,
        &date,
        lines,
    )
    .await
}

pub async fn expense_saved(conn: &mut PgConnection, expense_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let (amount,date,method,category):(Decimal,String,String,String)=sqlx::query_as("SELECT amount::numeric,expense_date,COALESCE(payment_method,'Cash'),COALESCE(category,'Expense') FROM expenses WHERE id=$1")
        .bind(expense_id).fetch_one(&mut *conn).await?;
    let state = crate::auth::fingerprint(format!("{amount}|{date}|{method}|{category}").as_bytes());
    let event_key = format!("expense:{expense_id}:{state}");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM rust_erp_accounting_documents WHERE event_key=$1)",
    )
    .bind(&event_key)
    .fetch_one(&mut *conn)
    .await?;
    if exists {
        return Ok(());
    }
    let previous: Option<(i64, serde_json::Value)> = sqlx::query_as("SELECT id,lines FROM rust_erp_accounting_documents WHERE source_type='expense' AND source_id=$1 AND event_type='expense.saved' ORDER BY id DESC LIMIT 1")
        .bind(expense_id).fetch_optional(&mut *conn).await?;
    if let Some((document_id, value)) = previous {
        let source: Vec<JournalLine> = serde_json::from_value(value)?;
        let reversed = source
            .into_iter()
            .map(|v| JournalLine {
                debit: v.credit,
                credit: v.debit,
                memo: format!("Expense revision reversal: {}", v.memo),
                ..v
            })
            .collect();
        document(
            conn,
            format!("expense-revision:{expense_id}:{document_id}"),
            "expense.revised",
            "expense",
            expense_id,
            &date,
            reversed,
        )
        .await?;
    }
    let lines = vec![
        line(
            conn,
            &format!("expense:{category}"),
            amount,
            Decimal::ZERO,
            &category,
        )
        .await?,
        line(
            conn,
            &format!("cash:{method}"),
            Decimal::ZERO,
            amount,
            "Expense payment",
        )
        .await?,
    ];
    document(
        conn,
        event_key,
        "expense.saved",
        "expense",
        expense_id,
        &date,
        lines,
    )
    .await
}

pub async fn expense_deleted(conn: &mut PgConnection, expense_id: i32) -> Result<()> {
    if !crate::ERP_ENABLED { return Ok(()); }
    let (document_id,date, original): (i64,String, serde_json::Value) = sqlx::query_as("SELECT id,CURRENT_DATE::text,lines FROM rust_erp_accounting_documents WHERE source_type='expense' AND source_id=$1 AND event_type='expense.saved' ORDER BY id DESC LIMIT 1")
        .bind(expense_id).fetch_optional(&mut *conn).await?.context("Original expense accounting journal is missing")?;
    let source: Vec<JournalLine> = serde_json::from_value(original)?;
    let reversed = source
        .into_iter()
        .map(|v| JournalLine {
            debit: v.credit,
            credit: v.debit,
            memo: format!("Expense deletion reversal: {}", v.memo),
            ..v
        })
        .collect();
    document(
        conn,
        format!("expense-delete:{expense_id}:{document_id}"),
        "expense.deleted",
        "expense",
        expense_id,
        &date,
        reversed,
    )
    .await
}

#[derive(Clone, Debug, PartialEq, Serialize, sqlx::FromRow)]
pub struct AccountMapping {
    pub mapping_key: String,
    pub account_code: String,
    pub account_name: String,
}

pub async fn list_mappings(
    pool: &sqlx::PgPool,
    actor: &crate::auth::Session,
) -> Result<Vec<AccountMapping>> {
    let mut tx = pool.begin().await?;
    actor
        .authorize(&mut tx, crate::auth::Permission::Manage)
        .await?;
    let rows = sqlx::query_as("SELECT mapping_key,account_code,account_name FROM rust_erp_account_mappings ORDER BY mapping_key")
        .fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn save_mapping(
    pool: &sqlx::PgPool,
    actor: &crate::auth::Session,
    mapping: &AccountMapping,
) -> Result<()> {
    anyhow::ensure!(
        !mapping.mapping_key.trim().is_empty()
            && mapping.mapping_key.len() <= 200
            && mapping
                .mapping_key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '-' | ' ')),
        "Invalid account mapping key"
    );
    anyhow::ensure!(
        !mapping.account_code.trim().is_empty()
            && mapping.account_code.len() <= 100
            && !mapping.account_name.trim().is_empty()
            && mapping.account_name.len() <= 300,
        "Account code and name are required"
    );
    let mut tx = pool.begin().await?;
    actor
        .authorize(&mut tx, crate::auth::Permission::Manage)
        .await?;
    sqlx::query("INSERT INTO rust_erp_account_mappings(mapping_key,account_code,account_name) VALUES($1,$2,$3) ON CONFLICT(mapping_key) DO UPDATE SET account_code=EXCLUDED.account_code,account_name=EXCLUDED.account_name,updated_at=CURRENT_TIMESTAMP")
        .bind(mapping.mapping_key.trim()).bind(mapping.account_code.trim()).bind(mapping.account_name.trim()).execute(&mut *tx).await?;
    crate::activity::record(
        &mut tx,
        actor,
        "rust.erp.account_mapping",
        &format!("mapping_key={}", mapping.mapping_key.trim()),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reversed_lines_remain_balanced() {
        let lines = vec![
            JournalLine {
                account_key: "a".into(),
                account_code: "1".into(),
                account_name: "A".into(),
                debit: 10.into(),
                credit: 0.into(),
                memo: "x".into(),
            },
            JournalLine {
                account_key: "b".into(),
                account_code: "2".into(),
                account_name: "B".into(),
                debit: 0.into(),
                credit: 10.into(),
                memo: "y".into(),
            },
        ];
        let debit: Decimal = lines.iter().map(|v| v.credit).sum();
        let credit: Decimal = lines.iter().map(|v| v.debit).sum();
        assert_eq!(debit, credit);
    }
}
