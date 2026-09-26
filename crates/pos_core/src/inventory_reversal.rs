use anyhow::{ensure, Context, Result};
use sqlx::PgPool;
use crate::auth::{Permission, Session};

pub async fn reverse_stock_in(pool: &PgPool, product: i32, movement: i32, reason: &str, actor: &Session) -> Result<()> {
    ensure!(!reason.trim().is_empty(), "Enter a reversal reason");
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let master: f64 = sqlx::query_scalar("SELECT COALESCE(stock,0)::float8 FROM products WHERE id=$1 FOR UPDATE")
        .bind(product).fetch_one(&mut *tx).await?;
    let (kind, qty, variant, location, notes, reference): (String,f64,Option<i32>,String,String,String) = sqlx::query_as("SELECT type,quantity::float8,variant_id,COALESCE(location,''),COALESCE(notes,''),COALESCE(reference,'') FROM stock_movements WHERE id=$1 AND product_id=$2 FOR UPDATE")
        .bind(movement).bind(product).fetch_one(&mut *tx).await?;
    ensure!(matches!(kind.as_str(), "in" | "stock_in"), "Only Stock In can be reversed here");
    let purchase_links: bool = sqlx::query_scalar("SELECT to_regclass('rust_purchase_movements') IS NOT NULL").fetch_one(&mut *tx).await?;
    if purchase_links {
        let linked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rust_purchase_movements WHERE movement_id=$1)").bind(movement).fetch_one(&mut *tx).await?;
        ensure!(!linked, "This stock receipt belongs to a purchase. A reviewed supplier return is required; stock-only reversal would leave the supplier balance incorrect.");
    }
    ensure!(!notes.contains("[REVERSED]") && !reference.starts_with("REV-") && !reference.ends_with("-REV"), "This movement has already been reversed");
    ensure!(qty.is_finite() && qty > 0.0, "Invalid movement quantity");
    let old: f64 = if let Some(id)=variant {
        sqlx::query_scalar("SELECT stock::float8 FROM product_variants WHERE id=$1 AND product_id=$2 FOR UPDATE").bind(id).bind(product).fetch_optional(&mut *tx).await?.context("Original variant no longer exists; use a reviewed adjustment")?
    } else { master };
    ensure!(old.is_finite() && old >= qty, "Insufficient stock to reverse this receipt");
    // Never guess an old variant or withdraw from unrelated batches.
    let batches: Vec<(String,String,String,f64,i32)> = if let Some(id)=variant {
        sqlx::query_as("SELECT location,batch_no,expire_date,SUM(delta)::float8,MAX(expiry_unknown) FROM variant_batch_changes WHERE movement_id=$1 AND product_id=$2 AND variant_id=$3 GROUP BY location,batch_no,expire_date ORDER BY location,batch_no,expire_date")
            .bind(movement).bind(product).bind(id).fetch_all(&mut *tx).await?
    } else {
        let batch = notes.rsplit_once("[Batch ").and_then(|(_,s)|s.strip_suffix(']')).context("Original batch is not recorded. Use a reviewed adjustment instead")?;
        let rows: Vec<(String,String,String,f64,i32)> = sqlx::query_as("SELECT location,batch_no,expire_date,$4::float8,0::integer FROM product_locations WHERE product_id=$1 AND location=$2 AND batch_no=$3 FOR UPDATE")
            .bind(product).bind(&location).bind(batch).bind(qty).fetch_all(&mut *tx).await?;
        ensure!(rows.len()==1, "Original batch is missing or ambiguous; use a reviewed adjustment");
        rows
    };
    ensure!(!batches.is_empty() && batches.iter().all(|b|b.3.is_finite() && b.3>0.0) && (batches.iter().map(|b|b.3).sum::<f64>()-qty).abs()<0.000001, "Original batch allocation is incomplete; use a reviewed adjustment");
    for (place,batch,expiry,amount,_) in &batches {
        let affected = if let Some(id)=variant {
            ensure!(amount.fract()==0.0 && *amount<=i32::MAX as f64, "Invalid variant allocation");
            sqlx::query("UPDATE variant_stock_batches SET quantity=quantity-$6 WHERE product_id=$1 AND variant_id=$2 AND location=$3 AND batch_no=$4 AND expire_date=$5 AND quantity >= $6")
                .bind(product).bind(id).bind(place).bind(batch).bind(expiry).bind(*amount as i32).execute(&mut *tx).await?.rows_affected()
        } else {
            sqlx::query("UPDATE product_locations SET quantity=quantity-$5::float8,last_updated=CURRENT_TIMESTAMP WHERE product_id=$1 AND location=$2 AND batch_no=$3 AND expire_date=$4 AND quantity >= $5::float8")
                .bind(product).bind(place).bind(batch).bind(expiry).bind(amount).execute(&mut *tx).await?.rows_affected()
        };
        ensure!(affected==1, "Original batch {} at {} has insufficient stock or no longer exists. Nothing was reversed", batch,place);
    }
    if let Some(id)=variant {
        ensure!(qty.fract()==0.0 && qty<=i32::MAX as f64, "Invalid variant quantity");
        sqlx::query("UPDATE product_variants SET stock=stock-$1,updated_at=CURRENT_TIMESTAMP WHERE id=$2").bind(qty as i32).bind(id).execute(&mut *tx).await?;
        sqlx::query("UPDATE products SET stock=(SELECT COALESCE(SUM(stock),0) FROM product_variants WHERE product_id=$1),last_updated=CURRENT_TIMESTAMP WHERE id=$1").bind(product).execute(&mut *tx).await?;
    } else {
        sqlx::query("UPDATE products SET stock=stock-$1,last_updated=CURRENT_TIMESTAMP WHERE id=$2").bind(qty).bind(product).execute(&mut *tx).await?;
    }
    let reversal: i32=sqlx::query_scalar("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,reason,reference,created_by,location,notes) VALUES($1,$2,'stock_out',$3,$4,$5,$6,$7,$8,$9,$10) RETURNING id")
        .bind(product).bind(variant).bind(qty).bind(old).bind(old-qty).bind(reason.trim()).bind(format!("REV-{movement}")).bind(actor.username()).bind(&location).bind(format!("Reversal of Stock In #{movement}; stock only, cost and supplier payments unchanged")).fetch_one(&mut *tx).await?;
    if let Some(id)=variant {
        for (place,batch,expiry,amount,unknown) in batches {
            sqlx::query("INSERT INTO variant_batch_changes(movement_id,product_id,variant_id,location,batch_no,expire_date,delta,expiry_unknown) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(reversal).bind(product).bind(id).bind(place).bind(batch).bind(expiry).bind(-(amount as i32)).bind(unknown).execute(&mut *tx).await?;
        }
    }
    sqlx::query("UPDATE stock_movements SET notes=COALESCE(notes,'') || $1 WHERE id=$2")
        .bind(format!(" [REVERSED] by {}; movement #{reversal}; {}",actor.username(),reason.trim())).bind(movement).execute(&mut *tx).await?;
    crate::activity::record(&mut tx,actor,"rust.inventory.reverse",&format!("product_id={product}; movement_id={movement}")).await?;
    tx.commit().await?;
    Ok(())
}
