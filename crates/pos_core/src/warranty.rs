//! Serial registration is linked to an existing completed receipt, never a second sale.
use crate::{
    auth::{Permission, Session},
    storage as sqlx,
};
use anyhow::{ensure, Context, Result};
use chrono::{Months, NaiveDate};
use sqlx::PgPool;

pub const SCHEMA: &str = include_str!("../../../migrations/warranty.sql");

pub fn is_addon_table(name: &str) -> bool {
    matches!(
        name,
        "rust_serial_units" | "rust_warranty_claims" | "rust_warranty_events"
    )
}

pub async fn ensure_schema(pool: &PgPool) -> Result<()> {
    let ready: bool = sqlx::query_scalar("SELECT to_regclass('rust_serial_units') IS NOT NULL AND to_regclass('rust_warranty_claims') IS NOT NULL AND to_regclass('rust_warranty_events') IS NOT NULL").fetch_one(pool).await?;
    if ready {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE sales IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    for statement in SCHEMA.split(';').filter(|s| !s.trim().is_empty()) {
        sqlx::query(statement.trim()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct Unit {
    pub id: String,
    pub serial: String,
    pub invoice: String,
    pub customer: String,
    pub product: String,
    pub starts: String,
    pub ends: String,
    pub sale_status: String,
}
#[derive(Clone, Debug, serde::Serialize, sqlx::FromRow)]
pub struct Certificate {
    pub id:String, pub serial:String, pub invoice:String, pub customer:String,
    pub phone:String, pub address:String, pub product:String,
    pub starts:String, pub ends:String, pub sale_status:String,
}
pub async fn certificate(pool:&PgPool,actor:&Session,id:&str)->Result<Certificate> {
    ensure_schema(pool).await?;
    let mut tx=pool.begin().await?;
    actor.authorize(&mut tx,Permission::Sell).await?;
    let row=sqlx::query_as("SELECT u.id,u.serial,COALESCE(s.invoice_no,'') AS invoice,COALESCE(c.name,'Customer') AS customer,COALESCE(c.phone,'') AS phone,COALESCE(c.address,'') AS address,COALESCE(si.product_name,'Product') AS product,u.starts,u.ends,COALESCE(s.status,'') AS sale_status FROM rust_serial_units u JOIN sales s ON s.id=u.sale_id JOIN sale_items si ON si.id=u.sale_item_id JOIN customers c ON c.id=u.customer_id WHERE u.id=$1")
        .bind(id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(row)
}
#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct ReceiptLine {
    pub id: i32,
    pub product: String,
    pub quantity: f64,
    pub registered: i64,
    pub invoice: String,
    pub customer: String,
}
#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct Claim {
    pub id: String,
    pub unit_id: String,
    pub issue: String,
    pub status: String,
    pub covered: i32,
    pub created_at: String,
}
#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct Event {
    pub status: String,
    pub note: String,
    pub actor: String,
    pub created_at: String,
}

pub async fn receipt_lines(
    pool: &PgPool,
    actor: &Session,
    invoice: &str,
) -> Result<Vec<ReceiptLine>> {
    ensure_schema(pool).await?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    let rows=sqlx::query_as("SELECT si.id,COALESCE(si.product_name,'Product') AS product,(si.qty-COALESCE(si.refunded_qty,0))::float8 AS quantity,(SELECT COUNT(*) FROM rust_serial_units u WHERE u.sale_item_id=si.id) AS registered,COALESCE(s.invoice_no,'') AS invoice,COALESCE(c.name,'Customer') AS customer FROM sale_items si JOIN sales s ON s.id=si.sale_id JOIN customers c ON c.id=s.customer_id WHERE s.invoice_no=$1 AND s.status='completed' ORDER BY si.id")
        .bind(invoice.trim()).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}

pub async fn register(
    pool: &PgPool,
    actor: &Session,
    item_id: i32,
    serial: &str,
    months: u32,
) -> Result<String> {
    let serial = validate_serial(serial, months)?;
    ensure_schema(pool).await?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    // Lock receipt before item to match refund locking and serialize unit allocation.
    let sale_id: i32 = sqlx::query_scalar("SELECT sale_id FROM sale_items WHERE id=$1")
        .bind(item_id)
        .fetch_one(&mut *tx)
        .await?;
    let (status, customer, stamp): (String, Option<i32>, String) = sqlx::query_as(
        "SELECT COALESCE(status,''),customer_id,created_at::text FROM sales WHERE id=$1 FOR UPDATE",
    )
    .bind(sale_id)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        status == "completed",
        "Only completed, non-refunded receipts can register a warranty"
    );
    let customer =
        customer.context("Select a customer on the sale before registering its warranty")?;
    let qty: f64 = sqlx::query_scalar(
        "SELECT (qty-COALESCE(refunded_qty,0))::float8 FROM sale_items WHERE id=$1 FOR UPDATE",
    )
    .bind(item_id)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        qty.is_finite() && qty > 0.0 && qty.fract() == 0.0,
        "Serial numbers require whole-unit receipt quantities"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM rust_serial_units WHERE sale_item_id=$1")
            .bind(item_id)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(
        (count as f64) < qty,
        "All units on this receipt line already have serial numbers"
    );
    let starts = NaiveDate::parse_from_str(
        stamp.get(..10).context("Receipt has no sale date")?,
        "%Y-%m-%d",
    )?;
    let ends = starts
        .checked_add_months(Months::new(months))
        .context("Invalid warranty end date")?;
    let id = crate::auth::new_request_id();
    sqlx::query("INSERT INTO rust_serial_units(id,serial,sale_item_id,sale_id,customer_id,starts,ends,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(&id).bind(&serial).bind(item_id).bind(sale_id).bind(customer).bind(starts.to_string()).bind(ends.to_string()).bind(actor.username()).bind(now()).execute(&mut *tx).await.context("Serial already registered or receipt data is invalid")?;
    crate::activity::record(
        &mut tx,
        actor,
        "rust.warranty.register",
        &format!("unit_id={id}; sale_id={sale_id}"),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
fn now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.6f")
        .to_string()
}
pub fn coverage(unit: &Unit, today: NaiveDate) -> bool {
    unit.sale_status == "completed"
        && today.to_string() >= unit.starts
        && today.to_string() <= unit.ends
}
pub async fn search(pool: &PgPool, actor: &Session, query: &str) -> Result<Vec<Unit>> {
    ensure_schema(pool).await?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    let rows=sqlx::query_as("SELECT u.id,u.serial,COALESCE(s.invoice_no,'') AS invoice,COALESCE(c.name,'Customer') AS customer,COALESCE(si.product_name,'Product') AS product,u.starts,u.ends,COALESCE(s.status,'') AS sale_status FROM rust_serial_units u JOIN sales s ON s.id=u.sale_id JOIN sale_items si ON si.id=u.sale_item_id JOIN customers c ON c.id=u.customer_id WHERE $1='' OR u.serial ILIKE $2 OR s.invoice_no ILIKE $2 OR c.name ILIKE $2 OR si.product_name ILIKE $2 ORDER BY u.created_at DESC,u.id LIMIT 200")
        .bind(query.trim()).bind(format!("%{}%",query.trim())).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}
pub async fn claims(pool: &PgPool, actor: &Session, unit: &str) -> Result<Vec<Claim>> {
    ensure_schema(pool).await?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    let rows=sqlx::query_as("SELECT id,unit_id,issue,status,covered,created_at FROM rust_warranty_claims WHERE unit_id=$1 ORDER BY created_at DESC,id").bind(unit).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}
pub async fn events(pool: &PgPool, actor: &Session, claim: &str) -> Result<Vec<Event>> {
    ensure_schema(pool).await?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    let rows=sqlx::query_as("SELECT status,note,actor,created_at FROM rust_warranty_events WHERE claim_id=$1 ORDER BY created_at,id").bind(claim).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}
pub async fn open_claim(pool: &PgPool, actor: &Session, unit: &str, issue: &str) -> Result<String> {
    ensure!(
        !issue.trim().is_empty() && issue.chars().count() <= 2000,
        "Enter the claim issue (up to 2000 characters)"
    );
    ensure_schema(pool).await?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Sell).await?;
    let sale: i32 = sqlx::query_scalar("SELECT sale_id FROM rust_serial_units WHERE id=$1")
        .bind(unit)
        .fetch_one(&mut *tx)
        .await?;
    let status: String =
        sqlx::query_scalar("SELECT COALESCE(status,'') FROM sales WHERE id=$1 FOR UPDATE")
            .bind(sale)
            .fetch_one(&mut *tx)
            .await?;
    let (starts, ends): (String, String) =
        sqlx::query_as("SELECT starts,ends FROM rust_serial_units WHERE id=$1 FOR UPDATE")
            .bind(unit)
            .fetch_one(&mut *tx)
            .await?;
    let active:i64=sqlx::query_scalar("SELECT COUNT(*) FROM rust_warranty_claims WHERE unit_id=$1 AND status IN ('open','in_progress')").bind(unit).fetch_one(&mut *tx).await?;
    ensure!(active == 0, "This serial already has an active claim");
    let today = chrono::Local::now().date_naive().to_string();
    let covered = i32::from(status == "completed" && today >= starts && today <= ends);
    let state = if covered == 1 { "open" } else { "rejected" };
    let id = crate::auth::new_request_id();
    sqlx::query("INSERT INTO rust_warranty_claims(id,unit_id,issue,status,covered,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(&id).bind(unit).bind(issue.trim()).bind(state).bind(covered).bind(now()).execute(&mut *tx).await?;
    let note = if covered == 1 {
        "Claim received"
    } else {
        "Not covered: warranty expired, not started, or receipt refunded"
    };
    add_event(&mut tx, &id, state, note, actor).await?;
    crate::activity::record(
        &mut tx,
        actor,
        "rust.warranty.claim",
        &format!("claim_id={id}; covered={covered}"),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}
async fn add_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    claim: &str,
    status: &str,
    note: &str,
    actor: &Session,
) -> Result<()> {
    sqlx::query("INSERT INTO rust_warranty_events(id,claim_id,status,note,actor,created_at) VALUES($1,$2,$3,$4,$5,$6)").bind(crate::auth::new_request_id()).bind(claim).bind(status).bind(note).bind(actor.username()).bind(now()).execute(&mut **tx).await?;
    Ok(())
}
pub async fn update_claim(
    pool: &PgPool,
    actor: &Session,
    claim: &str,
    status: &str,
    note: &str,
) -> Result<()> {
    ensure!(
        matches!(status, "in_progress" | "resolved" | "rejected"),
        "Invalid claim status"
    );
    ensure!(
        !note.trim().is_empty() && note.chars().count() <= 2000,
        "Enter a service or resolution note"
    );
    ensure_schema(pool).await?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let sale:i32=sqlx::query_scalar("SELECT u.sale_id FROM rust_warranty_claims c JOIN rust_serial_units u ON u.id=c.unit_id WHERE c.id=$1").bind(claim).fetch_one(&mut *tx).await?;
    let sale_status: String =
        sqlx::query_scalar("SELECT COALESCE(status,'') FROM sales WHERE id=$1 FOR UPDATE")
            .bind(sale)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(
        status == "rejected" || sale_status == "completed",
        "Receipt has been refunded; reject this claim instead of processing it"
    );
    let old: String =
        sqlx::query_scalar("SELECT status FROM rust_warranty_claims WHERE id=$1 FOR UPDATE")
            .bind(claim)
            .fetch_one(&mut *tx)
            .await?;
    ensure!(
        matches!(old.as_str(), "open" | "in_progress"),
        "This claim is already closed"
    );
    sqlx::query("UPDATE rust_warranty_claims SET status=$1 WHERE id=$2")
        .bind(status)
        .bind(claim)
        .execute(&mut *tx)
        .await?;
    add_event(&mut tx, claim, status, note.trim(), actor).await?;
    crate::activity::record(
        &mut tx,
        actor,
        "rust.warranty.status",
        &format!("claim_id={claim}; status={status}"),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub fn validate_serial(serial: &str, months: u32) -> Result<String> {
    let serial = serial.trim().to_uppercase();
    ensure!(
        !serial.is_empty()
            && serial.chars().count() <= 120
            && !serial.chars().any(char::is_control),
        "Enter a serial number (up to 120 characters)"
    );
    ensure!((1..=120).contains(&months), "Warranty must be 1–120 months");
    Ok(serial)
}

pub(crate) async fn register_checkout(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Session,
    item_id: i32,
    sale_id: i32,
    customer: i32,
    starts: NaiveDate,
    units: &[crate::models::SerialWarranty],
) -> Result<()> {
    for unit in units {
        let serial = validate_serial(&unit.serial, unit.months)?;
        let ends = starts
            .checked_add_months(Months::new(unit.months))
            .context("Invalid warranty end date")?;
        let id = crate::auth::new_request_id();
        sqlx::query("INSERT INTO rust_serial_units(id,serial,sale_item_id,sale_id,customer_id,starts,ends,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&id).bind(serial).bind(item_id).bind(sale_id).bind(customer).bind(starts.to_string()).bind(ends.to_string()).bind(actor.username()).bind(now()).execute(&mut **tx).await.context("Serial number already registered. Sale was not saved")?;
        crate::activity::record(
            tx,
            actor,
            "rust.warranty.register",
            &format!("unit_id={id}; sale_id={sale_id}"),
        )
        .await?;
    }
    Ok(())
}

pub async fn receipt_serials(
    pool: &PgPool,
    sale_id: i32,
) -> Result<Vec<crate::models::ReceiptSerial>> {
    let available: bool = sqlx::query_scalar("SELECT to_regclass('rust_serial_units') IS NOT NULL")
        .fetch_one(pool)
        .await?;
    if !available {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as("SELECT COALESCE(si.product_name,'Product') AS product_name,u.serial,u.starts,u.ends FROM rust_serial_units u JOIN sale_items si ON si.id=u.sale_item_id WHERE u.sale_id=$1 ORDER BY si.id,u.serial").bind(sale_id).fetch_all(pool).await?)
}

pub fn validate_sale_serials(draft: &crate::models::SaleDraft) -> Result<()> {
    if !draft.items.iter().any(|i| !i.serials.is_empty()) {
        return Ok(());
    }
    ensure!(
        draft.customer_id.is_some(),
        "Select a customer for serial/warranty sales"
    );
    let mut seen = std::collections::HashSet::new();
    for item in &draft.items {
        if item.serials.is_empty() {
            continue;
        }
        ensure!(
            !item.is_service && item.qty.fract() == 0.0 && item.qty == item.serials.len() as f64,
            "Enter one serial number per whole unit for {}",
            item.product_name
        );
        for unit in &item.serials {
            ensure!(
                seen.insert(validate_serial(&unit.serial, unit.months)?),
                "Duplicate serial number in this sale"
            );
        }
    }
    Ok(())
}
