use anyhow::{bail, Context, Result};
use sqlx::PgPool;

use crate::models::{CompletedSale, ReceiptDetail, ReceiptItemRow, SaleDraft, SaleSummary};

pub async fn complete_sale(
    pool: &PgPool,
    draft: &SaleDraft,
    actor: &crate::auth::Session,
) -> Result<CompletedSale> {
    if draft.items.is_empty() {
        bail!("sale has no items");
    }

    anyhow::ensure!(
        draft.created_by == actor.username(),
        "Sale operator does not match session"
    );
    anyhow::ensure!(
        !draft.invoice_no.trim().is_empty(),
        "Sale request ID is required"
    );
    for item in &draft.items {
        anyhow::ensure!(
            item.qty.is_finite()
                && item.qty > 0.0
                && item.price.is_finite()
                && item.price >= 0.0
                && item.cost.is_finite()
                && item.cost >= 0.0,
            "Invalid sale quantity, price or cost"
        );
    }
    let fingerprint = crate::auth::fingerprint(&serde_json::to_vec(draft)?);
    let mut tx = pool
        .begin()
        .await
        .context("failed to begin sale transaction")?;
    sqlx::query("SET LOCAL lock_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    actor
        .authorize(&mut tx, crate::auth::Permission::Sell)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(&draft.invoice_no)
        .execute(&mut *tx)
        .await?;
    let previous: Option<(String, Option<i32>)> = sqlx::query_as("SELECT fingerprint,sale_id FROM rust_checkout_requests WHERE request_id=$1")
        .bind(&draft.invoice_no).fetch_optional(&mut *tx).await
        .context("Checkout request ledger unavailable. Apply migrations/001_checkout_requests.sql before using this client")?;
    if let Some((saved_fingerprint, id)) = previous {
        anyhow::ensure!(
            saved_fingerprint == fingerprint,
            "This sale request was already used with different contents"
        );
        let id = id.context("This checkout was cancelled; create a new checkout")?;
        let sale = sqlx::query_as::<_, CompletedSale>(
            "SELECT id,invoice_no,total,payment,change_amount,created_at FROM sales WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(sale);
    }

    let subtotal = draft
        .items
        .iter()
        .map(|item| item.qty * item.price)
        .sum::<f64>();
    let cogs = draft
        .items
        .iter()
        .map(|item| item.qty * item.cost)
        .sum::<f64>();
    anyhow::ensure!(
        subtotal.is_finite()
            && cogs.is_finite()
            && draft.discount_amount.is_finite()
            && draft.discount_amount >= 0.0
            && draft.discount_amount <= subtotal,
        "Invalid sale totals"
    );
    let discount_amount = draft.discount_amount.clamp(0.0, subtotal);
    let settings = sqlx::query_as::<_, crate::models::AppSetting>("SELECT key,value FROM settings")
        .fetch_all(&mut *tx)
        .await?;
    let setting = |key: &str, default: &str| {
        settings
            .iter()
            .find(|s| s.key == key)
            .and_then(|s| s.value.clone())
            .unwrap_or_else(|| default.to_owned())
    };
    let discount_enabled = matches!(
        setting("discount_enabled", "0").to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    );
    let kind = setting("discount_type", "manual").to_lowercase();
    let expected_discount = if !discount_enabled {
        0.0
    } else {
        let value = setting("discount_value", "0")
            .parse::<f64>()
            .context("Invalid discount setting")?;
        anyhow::ensure!(
            value.is_finite() && value >= 0.0,
            "Invalid discount setting"
        );
        match kind.as_str() {
            "manual" => draft.discount_amount,
            "percentage" => (subtotal * value / 100.0).clamp(0.0, subtotal),
            "fixed" => value.clamp(0.0, subtotal),
            _ => 0.0,
        }
    };
    anyhow::ensure!(
        (expected_discount - draft.discount_amount).abs() < 0.005,
        "Discount settings changed. Cancel this unsaved checkout and reopen it"
    );
    let enabled = settings
        .iter()
        .any(|s| s.key == "tax_enabled" && s.value.as_deref() == Some("1"));
    let rate = settings
        .iter()
        .find(|s| s.key == "tax_rate")
        .and_then(|s| s.value.as_deref())
        .unwrap_or("0")
        .parse::<f64>()
        .context("Invalid tax rate")?;
    let after_discount = (subtotal - discount_amount).max(0.0);
    let total = after_discount + receipt_tax(after_discount, enabled, rate)?;
    anyhow::ensure!(
        total.is_finite()
            && draft.expected_total.is_finite()
            && (total - draft.expected_total).abs() < 0.005,
        "Sale total changed. Cancel this unsaved checkout and reopen it"
    );
    let credit = draft.payment_type.eq_ignore_ascii_case("Credit");
    anyhow::ensure!(
        draft.payment.is_finite() && draft.payment >= 0.0,
        "Invalid payment"
    );
    anyhow::ensure!(
        if credit {
            draft.customer_id.is_some() && draft.payment <= total
        } else {
            draft.payment >= total
        },
        "Select a customer for Credit or receive the full payment"
    );
    let change_amount = (draft.payment - total).max(0.0);
    let gross_profit = subtotal - cogs;
    let net_profit = total - cogs;
    if let Some(customer) = draft.customer_id {
        let (limit,balance):(f64,f64)=sqlx::query_as("SELECT COALESCE(credit_limit,0)::float8,COALESCE(current_balance,0)::float8 FROM customers WHERE id=$1 FOR UPDATE").bind(customer).fetch_one(&mut *tx).await?;
        if credit {
            let enabled: Option<String> = sqlx::query_scalar(
                "SELECT COALESCE(value,'1') FROM settings WHERE key='credit_limit_enabled'",
            )
            .fetch_optional(&mut *tx)
            .await?;
            let enforce = enabled
                .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"))
                .unwrap_or(true);
            anyhow::ensure!(
                !enforce || limit <= 0.0 || balance + total - draft.payment <= limit,
                "Customer credit limit would be exceeded"
            );
        }
    }

    // Main POS stores local wall time in this timestamp-without-time-zone column.
    let created_at = chrono::Local::now().naive_local();
    let invoice_no = crate::numbers::invoice(&mut tx).await?;
    let sale = sqlx::query_as::<_, CompletedSale>(
        r#"
        INSERT INTO sales (
            invoice_no, total, payment, change_amount, customer_id,
            status, payment_type, discount_amount, cogs, gross_profit, net_profit, created_by, created_at
        )
        VALUES ($1, $2, $3, $4, $5, 'completed', $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, invoice_no, total, payment, change_amount, created_at
        "#,
    )
    .bind(&invoice_no)
    .bind(total)
    .bind(draft.payment)
    .bind(change_amount)
    .bind(draft.customer_id)
    .bind(&draft.payment_type)
    .bind(discount_amount)
    .bind(cogs)
    .bind(gross_profit)
    .bind(net_profit)
    .bind(&draft.created_by)
    .bind(created_at)
    .fetch_one(&mut *tx)
    .await
    .context("failed to insert sale")?;

    if let Some(customer) = draft.customer_id {
        if credit {
            let balance = total - draft.payment;
            let date = chrono::Local::now().date_naive().to_string();
            sqlx::query("INSERT INTO credit_sales(invoice_no,customer_id,total_amount,paid_amount,balance_amount,sale_date,due_date,status,notes,sale_id) VALUES($1,$2,$3::float8,$4::float8,$5::float8,$6,'',$7,'Rust POS checkout',$8)").bind(&invoice_no).bind(customer).bind(total).bind(draft.payment).bind(balance).bind(date).bind(if balance==0.0{"paid"}else if draft.payment>0.0{"partial"}else{"pending"}).bind(sale.id).execute(&mut *tx).await?;
            sqlx::query(
                "UPDATE customers SET current_balance=COALESCE(current_balance,0)+$1 WHERE id=$2",
            )
            .bind(balance)
            .bind(customer)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("UPDATE customers SET total_visit=COALESCE(total_visit,0)+1,total_spent=COALESCE(total_spent,0)+$1 WHERE id=$2").bind(total).bind(customer).execute(&mut *tx).await?;
    }
    crate::sale_stock::lock_products(&mut tx, &draft.items).await?;
    for item in &draft.items {
        let allocations =
            crate::sale_stock::deduct(&mut tx, item, &invoice_no, actor.username()).await?;
        for allocation in allocations {
            sqlx::query(
            r#"
            INSERT INTO sale_items (
                sale_id, product_id, variant_id, product_name, qty, price, total, cost,
                wholesale_regular_price, wholesale_savings, wholesale_tier_min_qty, wholesale_unit_label,
                location_id, location, batch_no, expire_date
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            "#,
        )
        .bind(sale.id)
        .bind(item.product_id)
        .bind(item.variant_id)
        .bind(&item.product_name)
        .bind(allocation.quantity)
        .bind(item.price)
        .bind(allocation.quantity * item.price)
        .bind(item.cost)
        .bind(item.wholesale_regular_price)
        .bind(item.wholesale_savings * allocation.quantity / item.qty)
        .bind(item.wholesale_tier_min_qty)
        .bind(&item.wholesale_unit_label)
        .bind(allocation.location_id)
        .bind(allocation.location)
        .bind(allocation.batch)
        .bind(allocation.expiry)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("failed to insert sale item {}", item.product_id))?;
        }
    }

    sqlx::query(
        "INSERT INTO rust_checkout_requests(request_id,fingerprint,sale_id) VALUES($1,$2,$3)",
    )
    .bind(&draft.invoice_no)
    .bind(fingerprint)
    .bind(sale.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await.context("failed to commit sale")?;
    Ok(sale)
}

pub fn receipt_tax(amount: f64, enabled: bool, rate: f64) -> Result<f64> {
    if !enabled {
        return Ok(0.0);
    }
    anyhow::ensure!(
        rate.is_finite() && (0.0..=100.0).contains(&rate),
        "Invalid tax rate"
    );
    Ok((amount.max(0.0) * rate).round() / 100.0)
}

// Serialize cancellation with checkout, including a request still in flight.
pub async fn cancel_uncommitted(
    pool: &PgPool,
    draft: &SaleDraft,
    actor: &crate::auth::Session,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    actor
        .authorize(&mut tx, crate::auth::Permission::Sell)
        .await?;
    anyhow::ensure!(
        actor.username() == draft.created_by,
        "Sign in as the original sale operator to resolve this checkout"
    );
    let fingerprint = crate::auth::fingerprint(&serde_json::to_vec(draft)?);
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(&draft.invoice_no)
        .execute(&mut *tx)
        .await?;
    let previous: Option<(String, Option<i32>)> = sqlx::query_as(
        "SELECT fingerprint,sale_id FROM rust_checkout_requests WHERE request_id=$1",
    )
    .bind(&draft.invoice_no)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((saved, sale_id)) = previous {
        anyhow::ensure!(saved == fingerprint, "Checkout request contents differ");
        tx.commit().await?;
        return Ok(sale_id.is_none());
    }
    sqlx::query(
        "INSERT INTO rust_checkout_requests(request_id,fingerprint,sale_id) VALUES($1,$2,NULL)",
    )
    .bind(&draft.invoice_no)
    .bind(fingerprint)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

#[test]
fn tax_after_discount() {
    assert_eq!(receipt_tax(900.0, true, 5.0).unwrap(), 45.0);
    assert_eq!(receipt_tax(900.0, false, 5.0).unwrap(), 0.0);
    assert!(receipt_tax(900.0, true, 101.0).is_err());
}

pub async fn list_receipts(
    pool: &PgPool,
    search: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<SaleSummary>> {
    let pattern = format!("%{}%", search.trim());
    sqlx::query_as::<_, SaleSummary>(
        r#"
        SELECT
            s.id,
            s.invoice_no,
            s.created_at,
            s.total,
            s.payment,
            s.change_amount,
            COALESCE(s.discount_amount, 0)::DOUBLE PRECISION AS discount_amount,
            s.payment_type,
            s.status,
            COALESCE(c.name, 'Walk-in Customer') AS customer_name,
            COUNT(si.id)::BIGINT AS item_count
        FROM sales s
        LEFT JOIN customers c ON s.customer_id = c.id
        LEFT JOIN sale_items si ON si.sale_id = s.id
        WHERE
            COALESCE(s.status, 'completed') != 'deleted'
            AND (
                $1 = '%%'
                OR COALESCE(s.invoice_no, '') ILIKE $1
                OR COALESCE(c.name, '') ILIKE $1
                OR COALESCE(s.payment_type, '') ILIKE $1
            )
        GROUP BY s.id, c.name
        ORDER BY s.created_at DESC, s.id DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(pattern)
    .bind(limit.clamp(1, 1000))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await
    .context("failed to load receipts")
}

pub async fn get_receipt_detail(pool: &PgPool, sale_id: i32) -> Result<ReceiptDetail> {
    let summary = sqlx::query_as::<_, SaleSummary>(
        r#"
        SELECT
            s.id,
            s.invoice_no,
            s.created_at,
            s.total,
            s.payment,
            s.change_amount,
            COALESCE(s.discount_amount, 0)::DOUBLE PRECISION AS discount_amount,
            s.payment_type,
            s.status,
            COALESCE(c.name, 'Walk-in Customer') AS customer_name,
            COUNT(si.id)::BIGINT AS item_count
        FROM sales s
        LEFT JOIN customers c ON s.customer_id = c.id
        LEFT JOIN sale_items si ON si.sale_id = s.id
        WHERE s.id = $1
        GROUP BY s.id, c.name
        "#,
    )
    .bind(sale_id)
    .fetch_one(pool)
    .await
    .context("failed to load receipt summary")?;

    let items = sqlx::query_as::<_, ReceiptItemRow>(
        r#"
        SELECT
            COALESCE(si.product_name, p.name) AS product_name,
            si.qty,
            si.price,
            si.total
        FROM sale_items si
        LEFT JOIN products p ON p.id = si.product_id
        WHERE si.sale_id = $1
        ORDER BY si.id
        "#,
    )
    .bind(sale_id)
    .fetch_all(pool)
    .await
    .context("failed to load receipt items")?;

    Ok(ReceiptDetail { summary, items })
}

pub async fn refund_sale(pool: &PgPool, sale_id: i32, actor: &crate::auth::Session) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='10s'")
        .execute(&mut *tx)
        .await?;
    actor
        .authorize(&mut tx, crate::auth::Permission::Manage)
        .await?;
    let (status, payment, invoice, customer): (String, String, String, Option<i32>) =
        sqlx::query_as("SELECT COALESCE(status,'completed'),COALESCE(payment_type,''),COALESCE(invoice_no,''),customer_id FROM sales WHERE id=$1 FOR UPDATE")
            .bind(sale_id).fetch_optional(&mut *tx).await?.context("Receipt not found")?;
    anyhow::ensure!(
        status == "completed",
        "Only completed receipts can be refunded"
    );
    let now = chrono::Local::now().naive_local();
    if payment.eq_ignore_ascii_case("credit") {
        let customer = customer.context("Credit receipt has no customer")?;
        // Match the customer-before-invoice locking order used by credit collection.
        sqlx::query("SELECT id FROM customers WHERE id=$1 FOR UPDATE")
            .bind(customer)
            .fetch_one(&mut *tx)
            .await?;
        let credits: Vec<(i32, f64, f64, f64, String)> = sqlx::query_as(
            "SELECT id,total_amount::float8,COALESCE(paid_amount,0)::float8,COALESCE(balance_amount,0)::float8,COALESCE(status,'pending') FROM credit_sales WHERE customer_id=$1 AND (sale_id=$2 OR (sale_id IS NULL AND invoice_no=$3)) FOR UPDATE")
            .bind(customer).bind(sale_id).bind(&invoice).fetch_all(&mut *tx).await?;
        anyhow::ensure!(
            credits.len() == 1,
            "Credit invoice is missing or ambiguous; reconcile it before refunding"
        );
        let (credit_id, total, paid, balance, state) = &credits[0];
        anyhow::ensure!(
            state != "refunded",
            "Credit invoice was already refunded; reconcile the receipt first"
        );
        anyhow::ensure!(
            [*total, *paid, *balance]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0),
            "Invalid credit amounts"
        );
        sqlx::query("UPDATE customers SET current_balance=GREATEST(0,COALESCE(current_balance,0)-$1::float8) WHERE id=$2")
            .bind(balance).bind(customer).execute(&mut *tx).await?;
        sqlx::query("UPDATE credit_sales SET status='refunded',balance_amount=0,notes=COALESCE(notes,'') || ' [REFUNDED: Rust POS receipt refund]' WHERE id=$1")
            .bind(credit_id).execute(&mut *tx).await?;
        if *paid > 0.0 {
            sqlx::query("INSERT INTO credit_payments(credit_sale_id,customer_id,amount,payment_date,payment_method,note) VALUES($1,$2,$3::float8,$4,'refund','Rust POS receipt refund')")
                .bind(credit_id).bind(customer).bind(-paid).bind(now.to_string()).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO credit_adjustments(customer_id,credit_sale_id,amount,adjustment_type,reason,created_by) VALUES($1,$2,$3::float8,'refund','Receipt refund',$4)")
            .bind(customer).bind(credit_id).bind(-total).bind(actor.username()).execute(&mut *tx).await?;
    }
    let items: Vec<RefundStockItem> = sqlx::query_as("SELECT product_id,variant_id,COALESCE(qty,0)::float8 AS qty,COALESCE(refunded_qty,0)::float8 AS refunded,location_id,location,COALESCE(batch_no,'') AS batch,COALESCE(expire_date,'') AS expiry FROM sale_items WHERE sale_id=$1 ORDER BY product_id,variant_id,id FOR UPDATE")
        .bind(sale_id).fetch_all(&mut *tx).await?;
    for item in items {
        let qty = remaining_refund_quantity(item.qty, item.refunded)?;
        if qty == 0.0 {
            continue;
        }
        let product = item.product_id.context("Refund product no longer exists")?;
        let (stock, mode): (f64, String) = sqlx::query_as("SELECT COALESCE(stock,0)::float8,COALESCE(sold_by,'Each') FROM products WHERE id=$1 FOR UPDATE")
            .bind(product).fetch_optional(&mut *tx).await?.context("Refund product no longer exists")?;
        if mode.eq_ignore_ascii_case("service") || mode.eq_ignore_ascii_case("restaurant") {
            continue;
        }
        let mut old = stock;
        if let Some(variant) = item.variant_id {
            old = sqlx::query_scalar::<_, f64>("SELECT COALESCE(stock,0)::float8 FROM product_variants WHERE id=$1 AND product_id=$2 FOR UPDATE")
                .bind(variant).bind(product).fetch_optional(&mut *tx).await?.context("Refund variant no longer exists")?;
            anyhow::ensure!(
                qty.fract() == 0.0 && qty <= i32::MAX as f64,
                "Invalid variant refund quantity"
            );
            sqlx::query(
                "UPDATE product_variants SET stock=COALESCE(stock,0)+$1,updated_at=$2 WHERE id=$3",
            )
            .bind(qty as i32)
            .bind(now)
            .bind(variant)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("UPDATE products SET stock=COALESCE(stock,0)+$1,last_updated=$2 WHERE id=$3")
            .bind(qty)
            .bind(now)
            .bind(product)
            .execute(&mut *tx)
            .await?;
        // Legacy Rust checkout deducted master/variant stock only. Restore batches only
        // when the sale recorded a source location, avoiding double-counting those sales.
        if item.location_id.is_some()
            || item.location.as_ref().is_some_and(|v| !v.trim().is_empty())
        {
            let location = item
                .location
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or("Shop");
            if let Some(variant) = item.variant_id {
                sqlx::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES($1,$2,$3,$4,$5,$6,0) ON CONFLICT(product_id,variant_id,location,batch_no,expire_date) DO UPDATE SET quantity=variant_stock_batches.quantity+EXCLUDED.quantity")
                    .bind(product).bind(variant).bind(location).bind(&item.batch).bind(&item.expiry).bind(qty as i32).execute(&mut *tx).await?;
            } else {
                let updated = sqlx::query("UPDATE product_locations SET quantity=COALESCE(quantity,0)+$1,last_updated=$2 WHERE id=$3 AND product_id=$4")
                    .bind(qty).bind(now).bind(item.location_id).bind(product).execute(&mut *tx).await?.rows_affected();
                if updated == 0 {
                    sqlx::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES($1,$2,$3,$4,$5) ON CONFLICT(product_id,location,batch_no,expire_date) DO UPDATE SET quantity=product_locations.quantity+EXCLUDED.quantity,last_updated=EXCLUDED.last_updated")
                        .bind(product).bind(location).bind(&item.batch).bind(&item.expiry).bind(qty).execute(&mut *tx).await?;
                }
            }
        }
        sqlx::query("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,reason,reference,created_by,location,created_at) VALUES($1,$2,'refund',$3,$4,$5,'Receipt refund',$6,$9,$7,$8)")
            .bind(product).bind(item.variant_id).bind(qty).bind(old).bind(old+qty).bind(format!("REFUND-{sale_id}")).bind(item.location.as_deref().unwrap_or("Refund")).bind(now).bind(actor.username()).execute(&mut *tx).await?;
    }
    sqlx::query(
        "UPDATE sale_items SET refunded_qty=qty,refund_reason='Receipt refund' WHERE sale_id=$1",
    )
    .bind(sale_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE sales SET status='refunded' WHERE id=$1")
        .bind(sale_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await.context("Failed to commit refund")?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct RefundStockItem {
    product_id: Option<i32>,
    variant_id: Option<i32>,
    qty: f64,
    refunded: f64,
    location_id: Option<i32>,
    location: Option<String>,
    batch: String,
    expiry: String,
}

fn remaining_refund_quantity(qty: f64, refunded: f64) -> Result<f64> {
    anyhow::ensure!(
        qty.is_finite() && refunded.is_finite() && qty >= 0.0 && refunded >= 0.0 && refunded <= qty,
        "Invalid refunded quantity"
    );
    Ok(qty - refunded)
}

#[test]
fn refund_only_remaining_quantity() {
    assert_eq!(remaining_refund_quantity(5.0, 2.0).unwrap(), 3.0);
    assert_eq!(remaining_refund_quantity(5.0, 5.0).unwrap(), 0.0);
    assert!(remaining_refund_quantity(1.0, 2.0).is_err());
    assert!(remaining_refund_quantity(f64::NAN, 0.0).is_err());
}
