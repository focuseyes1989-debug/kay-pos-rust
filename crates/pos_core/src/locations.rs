use anyhow::{ensure, Result};
use sqlx::PgPool;

#[derive(Clone, Default, PartialEq, sqlx::FromRow)]
pub struct Location {
    pub id: i32,
    pub name: String,
}
#[derive(Clone, PartialEq, sqlx::FromRow)]
pub struct Stock {
    pub id: i32,
    pub name: String,
    pub sku: String,
    pub category: String,
    pub location: String,
    pub quantity: f64,
    pub variant_id:Option<i32>,
}
pub async fn list(pool: &PgPool) -> Result<(Vec<Location>, Vec<Stock>)> {
    let locations = list_names(pool).await?;
    let stock=sqlx::query_as(STOCK_BY_LOCATION_SQL).fetch_all(pool).await?;
    Ok((locations, stock))
}
pub async fn list_names(pool: &PgPool) -> Result<Vec<Location>> {
    Ok(
        sqlx::query_as("SELECT id,name FROM locations ORDER BY LOWER(name),id")
            .fetch_all(pool)
            .await?,
    )
}
pub async fn save(pool: &PgPool, location: &Location, delete: bool) -> Result<()> {
    let name = location.name.trim();
    ensure!(!name.is_empty(), "Location name is required");
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE locations,product_locations,products,stock_movements IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await?;
    let variants: bool =
        sqlx::query_scalar("SELECT to_regclass('variant_stock_batches') IS NOT NULL")
            .fetch_one(&mut *tx)
            .await?;
    if variants {
        sqlx::query("LOCK TABLE variant_stock_batches IN SHARE ROW EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;
    }
    if !delete {
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM locations WHERE LOWER(TRIM(name))=LOWER($1) AND id<>$2)",
        )
        .bind(name)
        .bind(location.id)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(!duplicate, "Location already exists");
    }
    if location.id == 0 {
        ensure!(!delete, "Select a location");
        sqlx::query("INSERT INTO locations(name) VALUES($1)")
            .bind(name)
            .execute(&mut *tx)
            .await?;
    } else {
        let old: String = sqlx::query_scalar("SELECT name FROM locations WHERE id=$1")
            .bind(location.id)
            .fetch_one(&mut *tx)
            .await?;
        if delete {
            let used: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_locations WHERE LOWER(TRIM(location))=LOWER(TRIM($1)))").bind(&old).fetch_one(&mut *tx).await?;
            ensure!(
                !used,
                "Move or remove all stock records from this location first"
            );
            if variants {
                let used: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM variant_stock_batches WHERE LOWER(TRIM(location))=LOWER(TRIM($1)))").bind(&old).fetch_one(&mut *tx).await?;
                ensure!(!used, "Move or remove variant stock records first");
            }
        }
        let replacement = if delete { "" } else { name };
        for (table, column) in [
            ("products", "warehouse"),
            ("stock_movements", "location"),
            ("product_locations", "location"),
        ] {
            sqlx::query(&format!(
                "UPDATE {table} SET {column}=$1 WHERE LOWER(TRIM({column}))=LOWER(TRIM($2))"
            ))
            .bind(replacement)
            .bind(&old)
            .execute(&mut *tx)
            .await?;
        }
        if variants && !delete {
            sqlx::query("UPDATE variant_stock_batches SET location=$1 WHERE LOWER(TRIM(location))=LOWER(TRIM($2))").bind(name).bind(&old).execute(&mut *tx).await?;
        }
        if delete {
            sqlx::query("DELETE FROM locations WHERE id=$1")
                .bind(location.id)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("UPDATE locations SET name=$1 WHERE id=$2")
                .bind(name)
                .bind(location.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

const STOCK_BY_LOCATION_SQL:&str=r"WITH stock AS (
 SELECT p.id,p.name,COALESCE(p.sku,'') AS sku,COALESCE(p.category,'') AS category,
 COALESCE(pl.location,'') AS location,COALESCE(SUM(pl.quantity),0)::float8 AS quantity,NULL::integer AS variant_id
 FROM product_locations pl JOIN products p ON p.id=pl.product_id
 WHERE lower(COALESCE(p.sold_by,'Each'))<>'variants' GROUP BY p.id,p.name,p.sku,p.category,pl.location
 UNION ALL
 SELECT p.id,p.name||' / '||concat_ws(' / ',NULLIF(v.size,''),NULLIF(v.color,''),NULLIF(v.sku,'')),
 COALESCE(v.sku,p.sku,''),COALESCE(p.category,''),b.location,SUM(b.quantity)::float8,v.id
 FROM variant_stock_batches b JOIN products p ON p.id=b.product_id JOIN product_variants v ON v.id=b.variant_id AND v.product_id=p.id
 GROUP BY p.id,p.name,p.sku,p.category,v.id,v.size,v.color,v.sku,b.location
 UNION ALL
 SELECT p.id,p.name||' / '||concat_ws(' / ',NULLIF(v.size,''),NULLIF(v.color,''),NULLIF(v.sku,'')),
 COALESCE(v.sku,p.sku,''),COALESCE(p.category,''),'Legacy / unassigned',
 (v.stock-COALESCE((SELECT SUM(b.quantity) FROM variant_stock_batches b WHERE b.product_id=p.id AND b.variant_id=v.id),0))::float8,v.id
 FROM product_variants v JOIN products p ON p.id=v.product_id
 WHERE lower(COALESCE(p.sold_by,''))='variants' AND v.stock>COALESCE((SELECT SUM(b.quantity) FROM variant_stock_batches b WHERE b.product_id=p.id AND b.variant_id=v.id),0)
) SELECT id,name,sku,category,location,SUM(quantity)::float8 AS quantity,variant_id FROM stock
 GROUP BY id,name,sku,category,location,variant_id
 HAVING lower(trim(location))<>'legacy / unassigned' OR SUM(quantity)<>0
 ORDER BY LOWER(name),location,id,variant_id";
