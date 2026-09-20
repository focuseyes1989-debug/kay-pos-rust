use crate::models::SaleItemDraft;
use anyhow::{ensure, Context, Result};
use sqlx::{PgConnection, Postgres, Transaction};

pub struct Allocation {
    pub quantity: f64,
    pub location_id: Option<i32>,
    pub location: Option<String>,
    pub batch: Option<String>,
    pub expiry: Option<String>,
}

pub async fn lock_products(
    tx: &mut Transaction<'_, Postgres>,
    items: &[SaleItemDraft],
) -> Result<()> {
    let mut ids: Vec<i32> = items.iter().map(|item| item.product_id).collect();
    ids.sort_unstable();
    ids.dedup();
    // All stock writers acquire the product before variants/batches, in ID order.
    for id in ids {
        sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?
            .context("Product no longer exists")?;
    }
    Ok(())
}

pub async fn deduct(
    connection: &mut PgConnection,
    item: &SaleItemDraft,
    invoice: &str,
    actor: &str,
) -> Result<Vec<Allocation>> {
    let (master, mode): (f64, String) = sqlx::query_as(
        "SELECT COALESCE(stock,0)::float8,COALESCE(sold_by,'Each') FROM products WHERE id=$1",
    )
    .bind(item.product_id)
    .fetch_one(&mut *connection)
    .await?;
    let service = mode.eq_ignore_ascii_case("service") || mode.eq_ignore_ascii_case("restaurant");
    ensure!(
        service == item.is_service,
        "Product type changed. Reload the cart"
    );
    if service {
        ensure!(
            item.variant_id.is_none(),
            "Service cannot have a stock variant"
        );
        return Ok(vec![Allocation {
            quantity: item.qty,
            location_id: None,
            location: None,
            batch: None,
            expiry: None,
        }]);
    }
    ensure!(
        mode.eq_ignore_ascii_case("variants") == item.variant_id.is_some(),
        "Product variant selection changed"
    );
    let old = if let Some(id) = item.variant_id {
        ensure!(
            item.qty.fract() == 0.0 && item.qty <= i32::MAX as f64,
            "Variant quantity must be a positive integer"
        );
        let stock: f64 = sqlx::query_scalar("SELECT COALESCE(stock,0)::float8 FROM product_variants WHERE id=$1 AND product_id=$2 AND COALESCE(active,1)=1 FOR UPDATE")
            .bind(id).bind(item.product_id).fetch_optional(&mut *connection).await?.context("Variant is unavailable")?;
        let sum: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(stock),0)::float8 FROM product_variants WHERE product_id=$1",
        )
        .bind(item.product_id)
        .fetch_one(&mut *connection)
        .await?;
        ensure!(
            (sum - master).abs() < 0.000001,
            "Reconcile master and variant stock in Main POS first"
        );
        stock
    } else {
        master
    };
    ensure!(
        old >= item.qty && master >= item.qty,
        "Insufficient stock for {}",
        item.product_name
    );
    let table = if item.variant_id.is_some() {
        "variant_stock_batches"
    } else {
        "product_locations"
    };
    let variant = item
        .variant_id
        .map(|id| format!(" AND variant_id={id}"))
        .unwrap_or_default();
    let location_id = if item.variant_id.is_some() {
        "NULL::integer"
    } else {
        "id"
    };
    let rows: Vec<(String, Option<i32>, String, String, String, f64)> = sqlx::query_as(&format!(
        "SELECT ctid::text,{location_id},COALESCE(location,'Shop'),COALESCE(batch_no,''),COALESCE(expire_date,''),COALESCE(quantity,0)::float8 FROM {table} WHERE product_id=$1{variant} ORDER BY COALESCE(NULLIF(expire_date,''),'9999-12-31'),location,batch_no,ctid FOR UPDATE"
    )).bind(item.product_id).fetch_all(&mut *connection).await?;
    ensure!(
        rows.iter().all(|r| r.5.is_finite() && r.5 >= 0.0)
            && (rows.iter().map(|r| r.5).sum::<f64>() - old).abs() < 0.000001,
        "Batch/location stock differs from total. Reconcile in Main POS first"
    );
    let mut remaining = item.qty;
    let mut allocations = Vec::new();
    for (ctid, id, location, batch, expiry, quantity) in rows {
        let take = remaining.min(quantity);
        if take <= 0.0 {
            continue;
        }
        let timestamp = if item.variant_id.is_some() {
            ""
        } else {
            ",last_updated=CURRENT_TIMESTAMP"
        };
        sqlx::query(&format!(
            "UPDATE {table} SET quantity=quantity-$1::float8{timestamp} WHERE ctid=$2::tid"
        ))
        .bind(take)
        .bind(ctid)
        .execute(&mut *connection)
        .await?;
        allocations.push(Allocation {
            quantity: take,
            location_id: id,
            location: Some(location),
            batch: Some(batch),
            expiry: Some(expiry),
        });
        remaining -= take;
        if remaining <= 0.0 {
            break;
        }
    }
    ensure!(remaining <= 0.000001, "Insufficient batch stock");
    if let Some(id) = item.variant_id {
        sqlx::query(
            "UPDATE product_variants SET stock=stock-$1,updated_at=CURRENT_TIMESTAMP WHERE id=$2",
        )
        .bind(item.qty as i32)
        .bind(id)
        .execute(&mut *connection)
        .await?;
    }
    sqlx::query("UPDATE products SET stock=stock-$1,last_updated=CURRENT_TIMESTAMP WHERE id=$2")
        .bind(item.qty)
        .bind(item.product_id)
        .execute(&mut *connection)
        .await?;
    sqlx::query("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,reason,reference,created_by,location) VALUES($1,$2,'sale',$3,$4,$5,'POS sale',$6,$7,'Multiple batches')")
        .bind(item.product_id).bind(item.variant_id).bind(item.qty).bind(old).bind(old-item.qty).bind(invoice).bind(actor).execute(&mut *connection).await?;
    Ok(allocations)
}
