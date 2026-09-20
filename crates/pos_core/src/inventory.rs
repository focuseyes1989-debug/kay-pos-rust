use anyhow::{ensure, Result};
use sqlx::PgPool;

#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct Movement {
    pub id: i32,
    pub kind: String,
    pub quantity: f64,
    pub old_stock: f64,
    pub new_stock: f64,
    pub location: String,
    pub reason: String,
    pub reference: String,
    pub created_by: String,
    pub created_at: String,
    pub variant_id: Option<i32>,
}
pub async fn movements(pool: &PgPool, id: i32) -> Result<Vec<Movement>> {
    Ok(sqlx::query_as("SELECT id,COALESCE(type,'') AS kind,COALESCE(quantity,0)::float8 AS quantity,COALESCE(old_stock,0)::float8 AS old_stock,COALESCE(new_stock,0)::float8 AS new_stock,COALESCE(location,'') AS location,COALESCE(reason,'') AS reason,COALESCE(reference,'') AS reference,COALESCE(created_by,'') AS created_by,COALESCE(created_at::text,'') AS created_at,variant_id FROM stock_movements WHERE product_id=$1 ORDER BY created_at DESC,id DESC").bind(id).fetch_all(pool).await?)
}

#[derive(Clone)]
pub struct Receipt {
    pub supplier_id: Option<i32>,
    pub product_id: i32,
    pub variant_id: Option<i32>,
    pub quantity: i32,
    pub unit_cost: f64,
    pub location: String,
    pub batch: String,
    pub expiry: String,
    pub reason: String,
    pub reference: String,
    pub received_by: String,
    pub notes: String,
    pub expected_stock: f64,
}
impl Receipt {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.product_id > 0 && self.quantity > 0,
            "Product and positive quantity are required"
        );
        ensure!(
            self.unit_cost.is_finite() && self.unit_cost >= 0.0,
            "Cost must be a non-negative number"
        );
        ensure!(!self.location.trim().is_empty(), "Select a location");
        ensure!(!self.reason.trim().is_empty(), "Reason is required");
        if !self.expiry.is_empty() {
            chrono::NaiveDate::parse_from_str(&self.expiry, "%Y-%m-%d")?;
        }
        Ok(())
    }
}

pub async fn change(pool: &PgPool, r: &Receipt, adjustment: bool) -> Result<()> {
    ensure!(
        r.quantity >= 0 && (adjustment || r.quantity > 0),
        "Enter a valid quantity"
    );
    ensure!(
        !r.reason.trim().is_empty() && !r.location.trim().is_empty(),
        "Location and reason are required"
    );
    let mut tx = pool.begin().await?;
    let (master,mode):(f64,String)=sqlx::query_as("SELECT COALESCE(stock,0)::float8,COALESCE(sold_by,'Each') FROM products WHERE id=$1 FOR UPDATE").bind(r.product_id).fetch_one(&mut *tx).await?;
    ensure!(
        master == r.expected_stock,
        "Stock changed. Refresh before saving"
    );
    ensure!(
        !mode.eq_ignore_ascii_case("Service") && !mode.eq_ignore_ascii_case("Restaurant"),
        "This item does not track stock"
    );
    ensure!(
        mode.eq_ignore_ascii_case("Variants") == r.variant_id.is_some(),
        "Select the correct variant"
    );
    let old = if let Some(id) = r.variant_id {
        let sum: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(stock),0)::float8 FROM product_variants WHERE product_id=$1",
        )
        .bind(r.product_id)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(sum == master, "Reconcile master and variant stock first");
        sqlx::query_scalar::<_,f64>("SELECT COALESCE(stock,0)::float8 FROM product_variants WHERE id=$1 AND product_id=$2 AND COALESCE(active,1)=1 FOR UPDATE").bind(id).bind(r.product_id).fetch_one(&mut *tx).await?
    } else {
        master
    };
    let delta = if adjustment {
        f64::from(r.quantity) - old
    } else {
        -f64::from(r.quantity)
    };
    ensure!(
        old + delta >= 0.0 && master + delta >= 0.0 && delta != 0.0,
        "Quantity must change stock without making it negative"
    );
    let table = if r.variant_id.is_some() {
        "variant_stock_batches"
    } else {
        "product_locations"
    };
    let variant_clause = if let Some(id) = r.variant_id {
        format!(" AND variant_id={id}")
    } else {
        String::new()
    };
    let tracked: f64 = sqlx::query_scalar(&format!(
        "SELECT COALESCE(SUM(quantity),0)::float8 FROM {table} WHERE product_id=$1{variant_clause}"
    ))
    .bind(r.product_id)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        tracked == old,
        "Batch/location stock differs from total. Reconcile in Main POS first"
    );
    let mut changes: Vec<(String, String, f64, i32)> = vec![];
    if delta > 0.0 {
        let batch = format!("ADJ-{}", chrono::Utc::now().format("%Y%m%d%H%M%S%f"));
        if let Some(vid) = r.variant_id {
            sqlx::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES($1,$2,$3,$4,'',$5,1)").bind(r.product_id).bind(vid).bind(r.location.trim()).bind(&batch).bind(delta as i32).execute(&mut *tx).await?;
        } else {
            sqlx::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES($1,$2,$3,'',$4)").bind(r.product_id).bind(r.location.trim()).bind(&batch).bind(delta).execute(&mut *tx).await?;
        }
        changes.push((batch, String::new(), delta, 1));
    } else {
        let unknown = if r.variant_id.is_some() {
            "expiry_unknown"
        } else {
            "0::integer"
        };
        let rows:Vec<(String,String,f64,String,i32)>=sqlx::query_as(&format!("SELECT COALESCE(batch_no,''),COALESCE(expire_date,''),quantity::float8,ctid::text,{unknown} FROM {table} WHERE product_id=$1{variant_clause} AND location=$2 AND quantity>0 ORDER BY COALESCE(NULLIF(expire_date,''),'9999-12-31'),batch_no FOR UPDATE")).bind(r.product_id).bind(r.location.trim()).fetch_all(&mut *tx).await?;
        ensure!(
            rows.iter().map(|x| x.2).sum::<f64>() >= -delta,
            "Insufficient stock at this location"
        );
        let mut remaining = -delta;
        for (batch, expiry, quantity, row_id, unknown) in rows {
            let take = remaining.min(quantity);
            if take <= 0.0 {
                break;
            }
            sqlx::query(&format!(
                "UPDATE {table} SET quantity=quantity-$1::float8 WHERE ctid=$2::tid"
            ))
            .bind(take)
            .bind(row_id)
            .execute(&mut *tx)
            .await?;
            changes.push((batch, expiry, -take, unknown));
            remaining -= take;
        }
    }
    if let Some(id) = r.variant_id {
        sqlx::query(
            "UPDATE product_variants SET stock=$1,updated_at=CURRENT_TIMESTAMP WHERE id=$2",
        )
        .bind((old + delta) as i32)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE products SET stock=$1,last_updated=CURRENT_TIMESTAMP WHERE id=$2")
        .bind(master + delta)
        .bind(r.product_id)
        .execute(&mut *tx)
        .await?;
    let id:i32=sqlx::query_scalar("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,reason,reference,created_by,location,notes) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id").bind(r.product_id).bind(r.variant_id).bind(if adjustment{"adjustment"}else{"stock_out"}).bind(delta.abs()).bind(old).bind(old+delta).bind(&r.reason).bind(&r.reference).bind(if r.received_by.trim().is_empty(){"Rust POS"}else{r.received_by.trim()}).bind(r.location.trim()).bind(&r.notes).fetch_one(&mut *tx).await?;
    if let Some(vid) = r.variant_id {
        for (batch, expiry, delta, unknown) in changes {
            sqlx::query("INSERT INTO variant_batch_changes(movement_id,product_id,variant_id,location,batch_no,expire_date,delta,expiry_unknown) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(r.product_id).bind(vid).bind(r.location.trim()).bind(batch).bind(expiry).bind(delta as i32).bind(unknown).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

pub async fn receive(pool: &PgPool, r: &Receipt) -> Result<()> {
    r.validate()?;
    let mut tx = pool.begin().await?;
    let (stock,cost,mode):(f64,f64,String)=sqlx::query_as("SELECT COALESCE(stock,0)::float8,COALESCE(cost,0)::float8,COALESCE(sold_by,'Each') FROM products WHERE id=$1 FOR UPDATE").bind(r.product_id).fetch_one(&mut *tx).await?;
    ensure!(
        !mode.eq_ignore_ascii_case("Service") && !mode.eq_ignore_ascii_case("Restaurant"),
        "This product does not track stock"
    );
    ensure!(
        stock == r.expected_stock,
        "Stock changed. Refresh the product before receiving stock"
    );
    ensure!(
        stock >= 0.0 && stock.is_finite() && cost.is_finite(),
        "Review invalid existing stock or cost in Main POS"
    );
    let variant = mode.eq_ignore_ascii_case("Variants");
    ensure!(
        variant == r.variant_id.is_some(),
        "Select an active variant for variant products only"
    );
    let batch = if r.batch.trim().is_empty() {
        crate::numbers::batch(&mut tx).await?
    } else {
        r.batch.trim().to_string()
    };
    let location = r.location.trim();
    let qty = f64::from(r.quantity);
    let mut old = stock;
    if let Some(vid) = r.variant_id {
        let sum: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(stock),0)::float8 FROM product_variants WHERE product_id=$1",
        )
        .bind(r.product_id)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(
            sum == stock,
            "Master and variant stock differ. Reconcile in Main POS first"
        );
        let vstock:i32=sqlx::query_scalar("SELECT COALESCE(stock,0) FROM product_variants WHERE id=$1 AND product_id=$2 AND COALESCE(active,1)=1 FOR UPDATE").bind(vid).bind(r.product_id).fetch_one(&mut *tx).await?;
        old = f64::from(vstock);
        let new = vstock
            .checked_add(r.quantity)
            .ok_or_else(|| anyhow::anyhow!("Variant quantity is too large"))?;
        let tracked:i64=sqlx::query_scalar("SELECT COALESCE(SUM(quantity),0)::bigint FROM variant_stock_batches WHERE product_id=$1 AND variant_id=$2").bind(r.product_id).bind(vid).fetch_one(&mut *tx).await?;
        ensure!(
            tracked <= i64::from(vstock),
            "Variant batches exceed stock. Reconcile in Main POS first"
        );
        if tracked < i64::from(vstock) {
            sqlx::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES($1,$2,'Legacy / unassigned','LEGACY-OPENING','',$3,1) ON CONFLICT(product_id,variant_id,location,batch_no,expire_date) DO UPDATE SET quantity=variant_stock_batches.quantity+EXCLUDED.quantity").bind(r.product_id).bind(vid).bind((i64::from(vstock)-tracked) as i32).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES($1,$2,$3,$4,$5,$6,0) ON CONFLICT(product_id,variant_id,location,batch_no,expire_date) DO UPDATE SET quantity=variant_stock_batches.quantity+EXCLUDED.quantity").bind(r.product_id).bind(vid).bind(location).bind(&batch).bind(&r.expiry).bind(r.quantity).execute(&mut *tx).await?;
        sqlx::query("UPDATE product_variants SET stock=$1,cost=$2::float8,updated_at=CURRENT_TIMESTAMP WHERE id=$3").bind(new).bind(r.unit_cost).bind(vid).execute(&mut *tx).await?;
    } else {
        sqlx::query("INSERT INTO product_locations(product_id,location,quantity,batch_no,expire_date) VALUES($1,$2,$3,$4,$5) ON CONFLICT(product_id,location,batch_no,expire_date) DO UPDATE SET quantity=product_locations.quantity+EXCLUDED.quantity,last_updated=CURRENT_TIMESTAMP").bind(r.product_id).bind(location).bind(qty).bind(&batch).bind(&r.expiry).execute(&mut *tx).await?;
    }
    let next_cost = (stock * cost + qty * r.unit_cost) / (stock + qty);
    ensure!(next_cost.is_finite(), "Resulting cost is too large");
    sqlx::query("UPDATE products SET stock=$1,cost=$2,last_updated=CURRENT_TIMESTAMP WHERE id=$3")
        .bind(stock + qty)
        .bind(next_cost)
        .bind(r.product_id)
        .execute(&mut *tx)
        .await?;
    let reference = if r.reference.trim().is_empty() {
        format!("RUST-{}", chrono::Utc::now().format("%Y%m%d%H%M%S%f"))
    } else {
        r.reference.clone()
    };
    let actor = if r.received_by.trim().is_empty() {
        "Rust POS"
    } else {
        r.received_by.trim()
    };
    let movement:i32=sqlx::query_scalar("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,reason,reference,created_by,location,notes) VALUES($1,$2,'stock_in',$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id").bind(r.product_id).bind(r.variant_id).bind(qty).bind(old).bind(old+qty).bind(&r.reason).bind(reference).bind(actor).bind(location).bind(format!("{} [Batch {}]",r.notes,batch)).fetch_one(&mut *tx).await?;
    if let Some(supplier_id) = r.supplier_id {
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM suppliers WHERE id=$1 AND LOWER(COALESCE(status,'Active'))='active')")
            .bind(supplier_id).fetch_one(&mut *tx).await?;
        ensure!(active, "Select an active supplier");
        sqlx::query("UPDATE stock_movements SET supplier_id=$1 WHERE id=$2")
            .bind(supplier_id).bind(movement).execute(&mut *tx).await?;
    }
    if let Some(vid) = r.variant_id {
        sqlx::query("INSERT INTO variant_batch_changes(movement_id,product_id,variant_id,location,batch_no,expire_date,delta,expiry_unknown) VALUES($1,$2,$3,$4,$5,$6,$7,0)").bind(movement).bind(r.product_id).bind(vid).bind(location).bind(batch).bind(&r.expiry).bind(r.quantity).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_receipts() {
        let mut r = Receipt {
            supplier_id: None,
            product_id: 1,
            variant_id: None,
            quantity: 1,
            unit_cost: 0.0,
            location: "Shop".into(),
            batch: String::new(),
            expiry: String::new(),
            reason: "Purchase".into(),
            reference: String::new(),
            received_by: String::new(),
            notes: String::new(),
            expected_stock: 0.0,
        };
        assert!(r.validate().is_ok());
        r.quantity = 0;
        assert!(r.validate().is_err());
        r.quantity = 1;
        r.unit_cost = f64::NAN;
        assert!(r.validate().is_err());
        r.unit_cost = 1.0;
        r.expiry = "2026-02-30".into();
        assert!(r.validate().is_err());
    }
}
