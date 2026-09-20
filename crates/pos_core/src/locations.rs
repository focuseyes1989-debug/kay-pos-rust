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
}
pub async fn list(pool: &PgPool) -> Result<(Vec<Location>, Vec<Stock>)> {
    let locations = list_names(pool).await?;
    let stock=sqlx::query_as("SELECT p.id,p.name,COALESCE(p.sku,'') AS sku,COALESCE(p.category,'') AS category,COALESCE(pl.location,'') AS location,COALESCE(SUM(pl.quantity),0)::float8 AS quantity FROM product_locations pl JOIN products p ON p.id=pl.product_id GROUP BY p.id,p.name,p.sku,p.category,pl.location ORDER BY LOWER(p.name),pl.location,p.id").fetch_all(pool).await?;
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
