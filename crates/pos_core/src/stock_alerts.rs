use sqlx::PgPool;

pub fn is_low(stock: f64, threshold: f64) -> bool {
    stock > 0.0 && threshold > 0.0 && stock <= threshold
}

#[cfg(test)]
mod tests {
    #[test]
    fn low_stock_boundary() {
        assert!(!super::is_low(3.0,0.0));
        assert!(!super::is_low(0.0,5.0));
        assert!(!super::is_low(-1.0,5.0));
        assert!(super::is_low(3.0,3.0));
        assert!(!super::is_low(4.0,3.0));
    }
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct StockAlert {
    pub product_id: i32,
    pub variant_id: Option<i32>,
    pub name: String,
    pub sku: String,
    pub stock: f64,
    pub threshold: f64,
}

// Read quantities directly, without downloading catalogue images or changing stock.
pub async fn list(pool: &PgPool) -> anyhow::Result<Vec<StockAlert>> {
    Ok(sqlx::query_as(r#"
        WITH products_normalized AS (
            SELECT p.*, lower(replace(trim(COALESCE(sold_by,'Each')),'_',' ')) AS mode
            FROM products p
        ), tracked AS (
            SELECT p.id AS product_id, NULL::integer AS variant_id,
                p.name, COALESCE(p.sku,'') AS sku,
                COALESCE(p.stock,0)::float8 AS stock,
                GREATEST(COALESCE(p.low_stock,0),0)::float8 AS threshold
            FROM products_normalized p
            WHERE mode NOT IN ('service','services','variant','variants')
              AND mode NOT LIKE '% service' AND mode NOT LIKE '% variants'
            UNION ALL
            SELECT p.id, v.id,
                p.name || ' / ' || COALESCE(NULLIF(concat_ws(' / ',NULLIF(v.size,''),NULLIF(v.color,'')),''),v.sku,'Variant'),
                COALESCE(v.sku,p.sku,''), COALESCE(v.stock,0)::float8,
                GREATEST(COALESCE(v.low_stock,0),0)::float8
            FROM products_normalized p JOIN product_variants v ON v.product_id=p.id
            WHERE (mode IN ('variant','variants') OR mode LIKE '% variants')
              AND COALESCE(v.active,1)=1
        )
        SELECT * FROM tracked WHERE stock<=0 OR (threshold>0 AND stock<=threshold)
        ORDER BY (stock<=0) DESC, stock, name, product_id, variant_id
    "#).fetch_all(pool).await?)
}
