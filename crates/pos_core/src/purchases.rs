use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use chrono::NaiveDate;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Line {
    pub product_id: i32,
    pub variant_id: Option<i32>,
    pub quantity: i32,
    pub unit_price: String,
    pub location: String,
    pub batch: String,
    pub expiry: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Draft {
    pub supplier_id: i32,
    pub order_date: String,
    pub discount: String,
    pub tax: String,
    pub notes: String,
    pub lines: Vec<Line>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Payment {
    pub supplier_id: i32,
    pub po_id: Option<i32>,
    pub amount: String,
    pub date: String,
    pub method: String,
    pub reference: String,
    pub notes: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Action {
    Save {
        id: Option<i32>,
        revision: i32,
        draft: Draft,
    },
    Receive {
        id: i32,
        revision: i32,
        date: String,
    },
    Cancel {
        id: i32,
        revision: i32,
        reason: String,
    },
    Pay(Payment),
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Command {
    pub request_id: String,
    pub action: Action,
}

pub fn money(value: &str) -> Result<Decimal> {
    let n = value
        .trim()
        .parse::<Decimal>()
        .context("Enter a valid amount")?;
    ensure!(
        n >= Decimal::ZERO && n <= Decimal::from(1_000_000_000_000i64) && n.scale() <= 2,
        "Amount must be non-negative with at most two decimal places"
    );
    Ok(n)
}
pub fn remaining(total: &str, paid: &str) -> Result<String> {
    let total = total.parse::<Decimal>()?;
    let paid = paid.parse::<Decimal>()?;
    Ok((total - paid).max(Decimal::ZERO).round_dp(2).to_string())
}
pub fn totals(d: &Draft) -> Result<(Decimal, Decimal)> {
    ensure!(d.supplier_id > 0, "Select a supplier");
    NaiveDate::parse_from_str(&d.order_date, "%Y-%m-%d")?;
    ensure!(
        !d.lines.is_empty() && d.lines.len() <= 200,
        "Add between 1 and 200 items"
    );
    ensure!(d.notes.len() <= 4000, "Notes are too long");
    let mut subtotal = Decimal::ZERO;
    for line in &d.lines {
        ensure!(
            line.product_id > 0 && line.quantity > 0 && line.quantity <= 1_000_000,
            "Use a positive whole-unit quantity, at most 1000000"
        );
        ensure!(
            !line.location.trim().is_empty()
                && line.location.len() <= 200
                && line.batch.len() <= 100,
            "Select a location and valid batch"
        );
        if !line.expiry.is_empty() {
            NaiveDate::parse_from_str(&line.expiry, "%Y-%m-%d")?;
        }
        subtotal += Decimal::from(line.quantity) * money(&line.unit_price)?;
    }
    let discount = money(&d.discount)?;
    let tax = money(&d.tax)?;
    ensure!(
        discount <= subtotal && tax <= Decimal::from(100),
        "Discount exceeds subtotal or tax exceeds 100%"
    );
    let total = ((subtotal - discount) * (Decimal::ONE + tax / Decimal::from(100))).round_dp(2);
    ensure!(
        total <= Decimal::from(1_000_000_000_000i64),
        "Order total is too large"
    );
    Ok((subtotal, total))
}

#[derive(Clone, sqlx::FromRow)]
pub struct DraftRow {
    pub id: i32,
    pub number: String,
    pub supplier_id: i32,
    pub supplier: String,
    pub order_date: String,
    pub total: String,
    pub status: String,
    pub revision: i32,
    pub po_id: Option<i32>,
    pub body: String,
}
#[derive(Clone, sqlx::FromRow)]
pub struct Order {
    pub id: i32,
    pub number: String,
    pub supplier_id: Option<i32>,
    pub supplier: String,
    pub order_date: String,
    pub total: String,
    pub status: String,
    pub payment_status: String,
    pub paid: String,
    pub notes: String,
}
#[derive(Clone, sqlx::FromRow)]
pub struct Item {
    pub name: String,
    pub variant: String,
    pub quantity: String,
    pub unit_price: String,
    pub total: String,
    pub location: String,
    pub batch: String,
    pub expiry: String,
}
#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct Supplier {
    pub id: i32,
    pub name: String,
    pub active: bool,
    pub balance: String,
}
#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct Product {
    pub product_id: i32,
    pub variant_id: Option<i32>,
    pub label: String,
    pub cost: String,
}
#[derive(Clone, sqlx::FromRow)]
pub struct LedgerEntry {
    pub date: String,
    pub reference: String,
    pub kind: String,
    pub amount: String,
    pub order_number: String,
    pub notes: String,
}
#[derive(Clone, Default)]
pub struct Data {
    pub drafts: Vec<DraftRow>,
    pub orders: Vec<Order>,
    pub suppliers: Vec<Supplier>,
    pub products: Vec<Product>,
    pub locations: Vec<String>,
    pub loaded_at: String,
}

const DRAFT_SELECT:&str="SELECT d.id,d.po_no AS number,d.supplier_id,s.name AS supplier,d.order_date,d.total_amount::text AS total,d.status,d.revision,d.po_id,d.body FROM rust_purchase_drafts d JOIN suppliers s ON s.id=d.supplier_id";

pub async fn load(pool: &PgPool, actor: &Session, from: &str, to: &str) -> Result<Data> {
    let (start, end) = crate::sale_summary::dates(from, to)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='20s'")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let loaded_at: String =
        sqlx::query_scalar("SELECT to_char(CURRENT_TIMESTAMP,'YYYY-MM-DD HH24:MI:SS TZ')")
            .fetch_one(&mut *tx)
            .await?;
    let drafts = sqlx::query_as(&format!(
        "{DRAFT_SELECT} WHERE d.order_date >= $1 AND d.order_date < $2 ORDER BY d.id DESC"
    ))
    .bind(start.to_string())
    .bind(end.to_string())
    .fetch_all(&mut *tx)
    .await
    .context("Purchases unavailable. Apply migrations/006_purchases.sql on the server")?;
    let orders=sqlx::query_as("SELECT p.id,COALESCE(p.po_no,'#'||p.id::text) AS number,p.supplier_id,COALESCE(s.name,'No supplier') AS supplier,COALESCE(p.order_date,'') AS order_date,COALESCE(p.total_amount,0)::numeric::text AS total,COALESCE(p.status,'') AS status,COALESCE(p.payment_status,'Unpaid') AS payment_status,COALESCE((SELECT SUM(amount::numeric) FROM supplier_payments WHERE purchase_order_id=p.id AND payment_type<>'Purchase'),0)::text AS paid,COALESCE(p.notes,'') AS notes FROM purchase_orders p LEFT JOIN suppliers s ON s.id=p.supplier_id WHERE left(p.order_date,10)>=$1 AND left(p.order_date,10)<$2 ORDER BY p.order_date DESC,p.id DESC")
        .bind(start.to_string()).bind(end.to_string()).fetch_all(&mut *tx).await?;
    let suppliers=sqlx::query_as("SELECT s.id,s.name,lower(COALESCE(NULLIF(trim(s.status),''),'Active'))='active' AS active,COALESCE((SELECT SUM(CASE WHEN payment_type='Purchase' THEN amount::numeric WHEN payment_type<>'Purchase' THEN -amount::numeric ELSE 0 END) FROM supplier_payments WHERE supplier_id=s.id),0)::text AS balance FROM suppliers s ORDER BY lower(s.name),s.id").fetch_all(&mut *tx).await?;
    let products=sqlx::query_as("SELECT p.id AS product_id,NULL::int AS variant_id,concat(p.name,' / ',p.sku) AS label,COALESCE(p.cost,0)::numeric::text AS cost FROM products p WHERE lower(COALESCE(p.sold_by,'Each'))='each' UNION ALL SELECT p.id,v.id,concat(p.name,' / ',v.size,' / ',v.color,' / ',v.sku),COALESCE(v.cost,p.cost,0)::numeric::text FROM products p JOIN product_variants v ON v.product_id=p.id WHERE lower(p.sold_by)='variants' AND COALESCE(v.active,1)=1 ORDER BY label,product_id,variant_id").fetch_all(&mut *tx).await?;
    let locations = sqlx::query_scalar("SELECT name FROM locations ORDER BY name")
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Data {
        drafts,
        orders,
        suppliers,
        products,
        locations,
        loaded_at,
    })
}

pub async fn order_items(pool: &PgPool, actor: &Session, id: i32) -> Result<Vec<Item>> {
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let rows=sqlx::query_as("SELECT COALESCE(p.name,'Deleted product') AS name,CASE WHEN m.po_item_id IS NULL THEN 'Not recorded' ELSE COALESCE(concat_ws(' / ',v.size,v.color,v.sku),'') END AS variant,COALESCE(i.quantity,0)::numeric::text AS quantity,COALESCE(i.unit_price,0)::numeric::text AS unit_price,COALESCE(i.total,0)::numeric::text AS total,COALESCE(m.location,'Not recorded') AS location,COALESCE(m.batch_no,'') AS batch,COALESCE(m.expire_date,'') AS expiry FROM purchase_order_items i LEFT JOIN products p ON p.id=i.product_id LEFT JOIN rust_purchase_items m ON m.po_item_id=i.id LEFT JOIN product_variants v ON v.id=m.variant_id WHERE i.po_id=$1 ORDER BY i.id").bind(id).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}
pub async fn ledger(
    pool: &PgPool,
    actor: &Session,
    supplier: i32,
    from: &str,
    to: &str,
) -> Result<Vec<LedgerEntry>> {
    let (start, end) = crate::sale_summary::dates(from, to)?;
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let rows=sqlx::query_as("SELECT p.payment_date AS date,COALESCE(p.reference_no,'') AS reference,COALESCE(p.payment_type,'Unknown') AS kind,p.amount::numeric::text AS amount,COALESCE(o.po_no,'Unallocated') AS order_number,COALESCE(p.notes,'') AS notes FROM supplier_payments p LEFT JOIN purchase_orders o ON o.id=p.purchase_order_id WHERE p.supplier_id=$1 AND left(p.payment_date,10)>=$2 AND left(p.payment_date,10)<$3 ORDER BY p.payment_date,p.id").bind(supplier).bind(start.to_string()).bind(end.to_string()).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}

async fn supplier_lock(conn: &mut PgConnection, id: i32, active: bool) -> Result<()> {
    let status:String=sqlx::query_scalar("SELECT lower(COALESCE(NULLIF(trim(status),''),'Active')) FROM suppliers WHERE id=$1 FOR UPDATE").bind(id).fetch_optional(&mut *conn).await?.context("Supplier no longer exists")?;
    ensure!(!active || status == "active", "Select an active supplier");
    Ok(())
}
async fn draft_lock(conn: &mut PgConnection, id: i32, revision: i32) -> Result<(DraftRow, Draft)> {
    let supplier: i32 =
        sqlx::query_scalar("SELECT supplier_id FROM rust_purchase_drafts WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
    supplier_lock(conn, supplier, false).await?;
    let row: DraftRow = sqlx::query_as(&format!("{DRAFT_SELECT} WHERE d.id=$1 FOR UPDATE OF d"))
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    ensure!(
        row.supplier_id == supplier,
        "Supplier changed; refresh the order"
    );
    ensure!(
        row.status == "pending" && row.revision == revision,
        "Order changed or is already received/cancelled. Refresh first"
    );
    let draft: Draft = serde_json::from_str(&row.body)?;
    ensure!(
        draft.supplier_id == row.supplier_id,
        "Draft supplier mismatch"
    );
    Ok((row, draft))
}
async fn validate_catalog(conn: &mut PgConnection, draft: &Draft) -> Result<()> {
    for line in &draft.lines {
        let mode: String =
            sqlx::query_scalar("SELECT lower(COALESCE(sold_by,'Each')) FROM products WHERE id=$1")
                .bind(line.product_id)
                .fetch_optional(&mut *conn)
                .await?
                .context("Product no longer exists")?;
        ensure!(
            matches!(mode.as_str(), "each" | "variants")
                && (mode == "variants") == line.variant_id.is_some(),
            "Select a stock item and its correct variant"
        );
        if let Some(id) = line.variant_id {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_variants WHERE id=$1 AND product_id=$2 AND COALESCE(active,1)=1)").bind(id).bind(line.product_id).fetch_one(&mut *conn).await?;
            ensure!(valid, "Variant is inactive or belongs to another product");
        }
        let place: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM locations WHERE name=$1)")
                .bind(line.location.trim())
                .fetch_one(&mut *conn)
                .await?;
        ensure!(place, "Select a registered stock location");
    }
    Ok(())
}

async fn request_lock(
    conn: &mut PgConnection,
    c: &Command,
    actor: &Session,
) -> Result<Option<i32>> {
    ensure!(
        c.request_id.starts_with("RUST-") && c.request_id.len() <= 80,
        "Invalid request ID"
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,721006))")
        .bind(&c.request_id)
        .execute(&mut *conn)
        .await?;
    let hash = crate::auth::fingerprint(&serde_json::to_vec(c)?);
    let previous:Option<(String,String,Option<i32>,i32)>=sqlx::query_as("SELECT username,payload_hash,result_id,cancelled FROM rust_purchase_requests WHERE request_id=$1").bind(&c.request_id).fetch_optional(&mut *conn).await.context("Apply migrations/006_purchases.sql before using Purchases")?;
    if let Some((username, old, result, cancelled)) = previous {
        ensure!(
            username == actor.username() && old == hash,
            "Request belongs to a different operator or payload"
        );
        ensure!(
            cancelled == 0,
            "This unsaved request was cancelled. Refresh before creating a new request"
        );
        return Ok(result);
    }
    Ok(None)
}

pub async fn execute(pool: &PgPool, actor: &Session, c: &Command) -> Result<i32> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='8s'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='30s'")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    if let Some(id) = request_lock(&mut tx, c, actor).await? {
        tx.commit().await?;
        return Ok(id);
    }
    let result = match &c.action {
        Action::Save {
            id,
            revision,
            draft,
        } => {
            let (_, total) = totals(draft)?;
            let id = if let Some(id) = id {
                let (row, _) = draft_lock(&mut tx, *id, *revision).await?;
                ensure!(
                    row.supplier_id == draft.supplier_id,
                    "Create a new order to change supplier"
                );
                supplier_lock(&mut tx, draft.supplier_id, true).await?;
                validate_catalog(&mut tx, draft).await?;
                sqlx::query("UPDATE rust_purchase_drafts SET order_date=$1,body=$2,total_amount=$3,revision=revision+1,updated_at=CURRENT_TIMESTAMP WHERE id=$4")
                    .bind(&draft.order_date).bind(serde_json::to_string(draft)?).bind(total).bind(id).execute(&mut *tx).await?;
                *id
            } else {
                supplier_lock(&mut tx, draft.supplier_id, true).await?;
                validate_catalog(&mut tx, draft).await?;
                // Random suffix avoids collisions with legacy Main PO numbers and other PCs.
                let number = format!("PO-{}", c.request_id.trim_start_matches("RUST-"));
                sqlx::query_scalar("INSERT INTO rust_purchase_drafts(po_no,supplier_id,order_date,body,total_amount,created_by) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
                    .bind(number).bind(draft.supplier_id).bind(&draft.order_date).bind(serde_json::to_string(draft)?).bind(total).bind(actor.username()).fetch_one(&mut *tx).await?
            };
            id
        }
        Action::Cancel {
            id,
            revision,
            reason,
        } => {
            ensure!(
                !reason.trim().is_empty() && reason.len() <= 1000,
                "Enter a cancellation reason"
            );
            let (_, mut draft) = draft_lock(&mut tx, *id, *revision).await?;
            draft.notes.push_str(&format!(
                "\nCancelled by {}: {}",
                actor.username(),
                reason.trim()
            ));
            sqlx::query("UPDATE rust_purchase_drafts SET status='cancelled',body=$1,revision=revision+1,updated_at=CURRENT_TIMESTAMP WHERE id=$2").bind(serde_json::to_string(&draft)?).bind(id).execute(&mut *tx).await?;
            *id
        }
        Action::Receive { id, revision, date } => {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")?;
            let (row, draft) = draft_lock(&mut tx, *id, *revision).await?;
            supplier_lock(&mut tx, draft.supplier_id, true).await?;
            let (subtotal, total) = totals(&draft)?;
            ensure!(
                total == row.total.parse::<Decimal>()?,
                "Draft total mismatch"
            );
            ensure!(
                date >= &draft.order_date,
                "Receipt date must not precede order date"
            );
            let mut products: Vec<i32> = draft.lines.iter().map(|l| l.product_id).collect();
            products.sort_unstable();
            products.dedup();
            for product in products {
                sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
                    .bind(product)
                    .execute(&mut *tx)
                    .await?;
            }
            validate_catalog(&mut tx, &draft).await?;
            let po:i32=sqlx::query_scalar("INSERT INTO purchase_orders(po_no,supplier_id,order_date,total_amount,status,discount,tax,payment_status,received_by,notes) VALUES($1,$2,$3,$4::numeric::float8,'completed',$5::numeric::float8,$6::numeric::float8,$7,$8,$9) RETURNING id")
                .bind(&row.number).bind(draft.supplier_id).bind(date).bind(total).bind(money(&draft.discount)?).bind(money(&draft.tax)?).bind(if total.is_zero(){"Paid"}else{"Unpaid"}).bind(actor.username()).bind(format!("{}\nOrdered {}; received via Rust purchase #{}",draft.notes,draft.order_date,row.id)).fetch_one(&mut *tx).await?;
            let mut allocated = Decimal::ZERO;
            for (index, line) in draft.lines.iter().enumerate() {
                let price = money(&line.unit_price)?;
                let line_total = price * Decimal::from(line.quantity);
                // Cumulative allocation preserves the exact bill total even for tiny lines.
                let prior: Decimal = draft.lines[..=index]
                    .iter()
                    .map(|l| money(&l.unit_price).unwrap() * Decimal::from(l.quantity))
                    .sum();
                let cumulative = if subtotal.is_zero() {
                    Decimal::ZERO
                } else {
                    (total * prior / subtotal).round_dp(2)
                };
                let landed = (cumulative - allocated) / Decimal::from(line.quantity);
                allocated = cumulative;
                let stock: f64 = sqlx::query_scalar(
                    "SELECT COALESCE(stock,0)::float8 FROM products WHERE id=$1",
                )
                .bind(line.product_id)
                .fetch_one(&mut *tx)
                .await?;
                if line.variant_id.is_none() {
                    let tracked:f64=sqlx::query_scalar("SELECT COALESCE(SUM(quantity),0)::float8 FROM product_locations WHERE product_id=$1").bind(line.product_id).fetch_one(&mut *tx).await?;
                    ensure!(
                        tracked == stock,
                        "Existing product stock and locations differ; reconcile before receiving"
                    );
                }
                let batch = if line.batch.trim().is_empty() {
                    crate::numbers::batch(&mut tx).await?
                } else {
                    line.batch.trim().into()
                };
                let receipt = crate::inventory::Receipt {
                    supplier_id: Some(draft.supplier_id),
                    product_id: line.product_id,
                    variant_id: line.variant_id,
                    quantity: line.quantity,
                    unit_cost: landed.to_f64().context("Cost too large")?,
                    location: line.location.trim().into(),
                    batch: batch.clone(),
                    expiry: line.expiry.clone(),
                    reason: "Purchase receipt".into(),
                    reference: row.number.clone(),
                    received_by: actor.username().into(),
                    notes: format!("Purchase #{po}; {}", draft.notes),
                    expected_stock: stock,
                    expected_location_stock: None,
                };
                let movement = crate::inventory::receive_in_transaction(&mut tx, &receipt).await?;
                let item:i32=sqlx::query_scalar("INSERT INTO purchase_order_items(po_id,product_id,quantity,unit_price,total) VALUES($1,$2,$3::float8,$4::numeric::float8,$5::numeric::float8) RETURNING id").bind(po).bind(line.product_id).bind(f64::from(line.quantity)).bind(price).bind(line_total).fetch_one(&mut *tx).await?;
                sqlx::query("INSERT INTO rust_purchase_items(po_item_id,variant_id,location,batch_no,expire_date) VALUES($1,$2,$3,$4,$5)").bind(item).bind(line.variant_id).bind(line.location.trim()).bind(batch).bind(&line.expiry).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO rust_purchase_movements(movement_id,po_id) VALUES($1,$2)")
                    .bind(movement)
                    .bind(po)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query("INSERT INTO supplier_payments(supplier_id,amount,payment_date,reference_no,payment_type,notes,purchase_order_id) VALUES($1,$2::numeric::float8,$3,$4,'Purchase',$5,$6)").bind(draft.supplier_id).bind(total).bind(date).bind(&row.number).bind(format!("Purchase received by {}",actor.username())).bind(po).execute(&mut *tx).await?;
            sqlx::query("UPDATE rust_purchase_drafts SET status='received',po_id=$1,revision=revision+1,updated_at=CURRENT_TIMESTAMP WHERE id=$2").bind(po).bind(id).execute(&mut *tx).await?;
            *id
        }
        Action::Pay(p) => pay(&mut tx, actor, p).await?,
    };
    sqlx::query("INSERT INTO rust_purchase_requests(request_id,username,payload_hash,result_id) VALUES($1,$2,$3,$4)").bind(&c.request_id).bind(actor.username()).bind(crate::auth::fingerprint(&serde_json::to_vec(c)?)).bind(result).execute(&mut *tx).await?;
    let kind=match &c.action{Action::Save{..}=>"save",Action::Receive{..}=>"receive",Action::Cancel{..}=>"cancel",Action::Pay(_)=>"payment"};
    crate::activity::record(&mut tx,actor,&format!("rust.purchase.{kind}"),&format!("result_id={result}; request={}",c.request_id)).await?;
    tx.commit()
        .await
        .context("Purchase outcome uncertain; retry the same request")?;
    Ok(result)
}

async fn pay(conn: &mut PgConnection, actor: &Session, p: &Payment) -> Result<i32> {
    let amount = money(&p.amount)?;
    ensure!(amount > Decimal::ZERO, "Payment must be positive");
    NaiveDate::parse_from_str(&p.date, "%Y-%m-%d")?;
    ensure!(
        matches!(
            p.method.as_str(),
            "Cash" | "Bank Transfer" | "Mobile Payment"
        ),
        "Select a payment method"
    );
    ensure!(
        p.reference.len() <= 200 && p.notes.len() <= 4000,
        "Reference or notes too long"
    );
    supplier_lock(conn, p.supplier_id, false).await?;
    let mut new_status = None;
    if let Some(po) = p.po_id {
        let (supplier,total,status,date,payment_status):(Option<i32>,Decimal,String,String,String)=sqlx::query_as("SELECT supplier_id,COALESCE(total_amount,0)::numeric,COALESCE(status,''),COALESCE(order_date,''),COALESCE(payment_status,'') FROM purchase_orders WHERE id=$1 FOR UPDATE").bind(po).fetch_one(&mut *conn).await?;
        ensure!(
            supplier == Some(p.supplier_id) && status == "completed",
            "Select a completed order for this supplier"
        );
        ensure!(p.date >= date, "Payment date must not precede receipt date");
        let (debit,paid,wrong):(Decimal,Decimal,i64)=sqlx::query_as("SELECT COALESCE(SUM(amount::numeric) FILTER(WHERE payment_type='Purchase'),0),COALESCE(SUM(amount::numeric) FILTER(WHERE payment_type<>'Purchase'),0),COUNT(*) FILTER(WHERE supplier_id<>$2 OR payment_type IS NULL OR amount<0) FROM supplier_payments WHERE purchase_order_id=$1").bind(po).bind(p.supplier_id).fetch_one(&mut *conn).await?;
        ensure!(
            wrong == 0 && debit.round_dp(2) == total.round_dp(2) && paid >= Decimal::ZERO,
            "Purchase ledger does not match this order; reconcile in Main POS first"
        );
        ensure!(
            !(payment_status.eq_ignore_ascii_case("Paid") && paid.round_dp(2) < total.round_dp(2))
                && !(payment_status.eq_ignore_ascii_case("Partial") && paid <= Decimal::ZERO),
            "Payment status does not match the ledger; reconcile in Main POS first"
        );
        ensure!(
            amount <= (total - paid).round_dp(2),
            "Payment exceeds the remaining order balance; refresh first"
        );
        new_status = Some(if (paid + amount).round_dp(2) >= total.round_dp(2) {
            "Paid"
        } else {
            "Partial"
        });
    }
    let id=sqlx::query_scalar("INSERT INTO supplier_payments(supplier_id,amount,payment_date,reference_no,payment_type,notes,purchase_order_id) VALUES($1,$2::numeric::float8,$3,$4,$5,$6,$7) RETURNING id").bind(p.supplier_id).bind(amount).bind(&p.date).bind(p.reference.trim()).bind(&p.method).bind(format!("{} [Recorded by {}]",p.notes,actor.username())).bind(p.po_id).fetch_one(&mut *conn).await?;
    if let (Some(po), Some(status)) = (p.po_id, new_status) {
        sqlx::query("UPDATE purchase_orders SET payment_status=$1 WHERE id=$2")
            .bind(status)
            .bind(po)
            .execute(&mut *conn)
            .await?;
    }
    Ok(id)
}

pub async fn cancel_unsaved(pool: &PgPool, actor: &Session, c: &Command) -> Result<Option<i32>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='8s'")
        .execute(&mut *tx)
        .await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    // The same advisory lock fences a delayed execute before committing a tombstone.
    ensure!(
        c.request_id.starts_with("RUST-") && c.request_id.len() <= 80,
        "Invalid request ID"
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,721006))")
        .bind(&c.request_id)
        .execute(&mut *tx)
        .await?;
    let hash = crate::auth::fingerprint(&serde_json::to_vec(c)?);
    let existing: Option<(String, String, Option<i32>)> = sqlx::query_as(
        "SELECT username,payload_hash,result_id FROM rust_purchase_requests WHERE request_id=$1",
    )
    .bind(&c.request_id)
    .fetch_optional(&mut *tx)
    .await?;
    let result = if let Some((user, old, id)) = existing {
        ensure!(
            user == actor.username() && old == hash,
            "Request belongs to a different operator or payload"
        );
        id
    } else {
        sqlx::query("INSERT INTO rust_purchase_requests(request_id,username,payload_hash,cancelled) VALUES($1,$2,$3,1)").bind(&c.request_id).bind(actor.username()).bind(hash).execute(&mut *tx).await?;
        None
    };
    tx.commit().await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn totals_validate_money_quantity_tax_and_expiry() {
        let mut d = Draft {
            supplier_id: 1,
            order_date: "2026-09-25".into(),
            discount: "10".into(),
            tax: "5".into(),
            notes: String::new(),
            lines: vec![Line {
                product_id: 1,
                variant_id: None,
                quantity: 2,
                unit_price: "50".into(),
                location: "Shop".into(),
                batch: String::new(),
                expiry: String::new(),
            }],
        };
        assert_eq!(totals(&d).unwrap().1, "94.50".parse::<Decimal>().unwrap());
        d.discount = "101".into();
        assert!(totals(&d).is_err());
        d.discount = "0".into();
        d.lines[0].quantity = 0;
        assert!(totals(&d).is_err());
        d.lines[0].quantity = 1;
        d.lines[0].expiry = "2026-02-30".into();
        assert!(totals(&d).is_err());
        assert!(money("NaN").is_err());
        assert!(money("0.001").is_err());
    }
}
