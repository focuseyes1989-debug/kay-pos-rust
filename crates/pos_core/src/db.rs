use anyhow::{Context, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::models::{AppSetting, Category, PaymentType, Product, ProductPriceTier, ProductVariant};

#[derive(Clone, Debug)]
pub struct DatabaseConfig {
    pub database_url: String,
    pub max_connections: u32,
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self> {
        let database_url =
            std::env::var("ZAY_POS_DATABASE_URL").context("ZAY_POS_DATABASE_URL is not set")?;
        Ok(Self {
            database_url,
            max_connections: 5,
        })
    }
}

pub async fn connect(config: &DatabaseConfig) -> Result<PgPool> {
    use std::sync::{Mutex, OnceLock};
    static POOLS: OnceLock<Mutex<std::collections::HashMap<String, PgPool>>> = OnceLock::new();
    let pools = POOLS.get_or_init(Default::default);
    if let Some(pool) = pools
        .lock()
        .unwrap()
        .get(&config.database_url)
        .filter(|p| !p.is_closed())
        .cloned()
    {
        return Ok(pool);
    }
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&config.database_url)
        .await
        .context("failed to connect to PostgreSQL")?;
    let mut cache = pools.lock().unwrap();
    if cache.len() >= 4 {
        cache.retain(|_, p| !p.is_closed());
        if cache.len() >= 4 {
            cache.clear();
        }
    }
    Ok(cache
        .entry(config.database_url.clone())
        .or_insert(pool)
        .clone())
}

pub async fn ping(config: &DatabaseConfig) -> Result<()> {
    let pool = connect(config).await?;
    sqlx::query("SELECT 1").execute(&pool).await?;
    Ok(())
}

pub async fn list_categories(pool: &PgPool) -> Result<Vec<Category>> {
    sqlx::query_as::<_, Category>(
        r#"
        SELECT
            c.id,
            c.name,
            c.parent_id,
            c.color
        FROM categories c
        LEFT JOIN (
            SELECT
                p.category_id,
                SUM(si.qty) AS sold_qty,
                SUM(si.total) AS sold_amount
            FROM sale_items si
            JOIN products p ON p.id = si.product_id
            GROUP BY p.category_id
        ) sales ON sales.category_id = c.id
        WHERE COALESCE(c.status, 'active') = 'active'
        ORDER BY
            COALESCE(sales.sold_qty, 0) DESC,
            COALESCE(sales.sold_amount, 0) DESC,
            c.sort_order,
            c.name
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to load categories")
}

pub async fn save_settings(pool: &PgPool, values: &[(String, String)]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for (key, value) in values {
        sqlx::query("INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")
            .bind(key).bind(value).execute(&mut *tx).await
            .context("failed to save settings")?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list_settings(pool: &PgPool) -> Result<Vec<AppSetting>> {
    sqlx::query_as::<_, AppSetting>(
        r#"
        SELECT key, value
        FROM settings
        ORDER BY key
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to load app settings")
}

pub async fn list_payment_types(pool: &PgPool) -> Result<Vec<PaymentType>> {
    sqlx::query_as::<_, PaymentType>(
        r#"
        SELECT id, name
        FROM payment_types
        WHERE COALESCE(active, 1) = 1
        ORDER BY sort_order, name
        "#,
    )
    .fetch_all(pool)
    .await
    .context("failed to load payment types")
}

pub async fn save_payment_type(
    pool: &PgPool,
    payment_id: Option<i32>,
    name: &str,
) -> Result<PaymentType> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("payment type name is required");
    }

    if let Some(payment_id) = payment_id {
        sqlx::query_as::<_, PaymentType>(
            r#"
            UPDATE payment_types
            SET name = $1
            WHERE id = $2
            RETURNING id, name
            "#,
        )
        .bind(name)
        .bind(payment_id)
        .fetch_one(pool)
        .await
        .context("failed to update payment type")
    } else {
        sqlx::query_as::<_, PaymentType>(
            r#"
            INSERT INTO payment_types (name, active)
            VALUES ($1, 1)
            ON CONFLICT (name) DO UPDATE SET active = 1
            RETURNING id, name
            "#,
        )
        .bind(name)
        .fetch_one(pool)
        .await
        .context("failed to add payment type")
    }
}

pub async fn delete_payment_type(pool: &PgPool, payment_id: i32) -> Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE payment_types
        SET active = 0
        WHERE id = $1
        "#,
    )
    .bind(payment_id)
    .execute(pool)
    .await
    .context("failed to delete payment type")?;

    if result.rows_affected() == 0 {
        anyhow::bail!("payment type was not found");
    }

    Ok(())
}

pub async fn search_products(pool: &PgPool, term: &str, limit: i64) -> Result<Vec<Product>> {
    search_products_with_images(pool, term, limit, true).await
}

pub async fn search_product_metadata(
    pool: &PgPool,
    term: &str,
    limit: i64,
) -> Result<Vec<Product>> {
    search_products_with_images(pool, term, limit, false).await
}

async fn search_products_with_images(
    pool: &PgPool,
    term: &str,
    limit: i64,
    images: bool,
) -> Result<Vec<Product>> {
    let like = format!("%{}%", term.trim());
    let limit = if limit <= 0 { i64::MAX } else { limit };
    let mut products = sqlx::query_as::<_, Product>(
        r#"
        SELECT
            p.id,
            p.name,
            p.category_id,
            c.name AS category_name,
            p.sku,
            p.barcode,
            p.price,
            p.cost,
            p.stock,
            p.low_stock,
            p.sold_by,
            p.image_filename,
            CASE
                WHEN $3 AND p.image_data IS NOT NULL THEN
                    CONCAT('data:', COALESCE(NULLIF(p.image_mime, ''), 'image/jpeg'), ';base64,', encode(p.image_data, 'base64'))
                ELSE NULL
            END AS image_data_url
        FROM products p
        LEFT JOIN categories c ON c.id = p.category_id
        WHERE
            $1 = '%%'
            OR p.name ILIKE $1
            OR COALESCE(p.sku, '') ILIKE $1
            OR COALESCE(p.barcode, '') ILIKE $1
        ORDER BY
            CASE WHEN COALESCE(p.stock, 0) <= 0 THEN 1 ELSE 0 END,
            p.is_favourite DESC,
            p.name
        LIMIT $2
        "#,
    )
    .bind(like)
    .bind(limit)
    .bind(images)
    .fetch_all(pool)
    .await
    .context("failed to load products")?;

    attach_variants_and_tiers(pool, &mut products).await?;
    Ok(products)
}

pub async fn product_image(pool: &PgPool, id: i32) -> Result<Option<String>> {
    Ok(sqlx::query_scalar::<_, Option<String>>("SELECT CASE WHEN image_data IS NOT NULL THEN CONCAT('data:',COALESCE(NULLIF(image_mime,''),'image/jpeg'),';base64,',encode(image_data,'base64')) END FROM products WHERE id=$1")
        .bind(id).fetch_optional(pool).await?.flatten())
}

async fn attach_variants_and_tiers(pool: &PgPool, products: &mut [Product]) -> Result<()> {
    if products.is_empty() {
        return Ok(());
    }

    let product_ids = products
        .iter()
        .map(|product| product.id)
        .collect::<Vec<_>>();

    let variants = sqlx::query_as::<_, ProductVariant>(
        r#"
        SELECT
            id AS variant_id,
            product_id,
            size,
            color,
            sku,
            barcode,
            price::DOUBLE PRECISION AS price,
            cost::DOUBLE PRECISION AS cost,
            stock::DOUBLE PRECISION AS stock,
            low_stock::DOUBLE PRECISION AS low_stock,
            COALESCE(wholesale_min_qty, 0) AS wholesale_min_qty,
            COALESCE(wholesale_price, 0)::DOUBLE PRECISION AS wholesale_price
        FROM product_variants
        WHERE product_id = ANY($1) AND COALESCE(active, 1) = 1
        ORDER BY product_id, size, color, id
        "#,
    )
    .bind(&product_ids)
    .fetch_all(pool)
    .await
    .context("failed to load product variants")?;

    let tiers = sqlx::query_as::<_, ProductPriceTier>(
        r#"
        SELECT
            id,
            product_id,
            min_qty,
            COALESCE(unit_multiplier,1) AS unit_multiplier,
            unit_label,
            unit_price::DOUBLE PRECISION AS unit_price
        FROM product_price_tiers
        WHERE product_id = ANY($1) AND COALESCE(active, 1) = 1 AND unit_price > 0
        ORDER BY product_id, min_qty ASC, unit_price ASC
        "#,
    )
    .bind(&product_ids)
    .fetch_all(pool)
    .await
    .context("failed to load wholesale price tiers")?;

    for product in products.iter_mut() {
        product.variants = variants
            .iter()
            .filter(|variant| variant.product_id == product.id)
            .cloned()
            .collect();
        product.price_tiers = tiers
            .iter()
            .filter(|tier| tier.product_id == product.id)
            .cloned()
            .collect();
        if is_variants_mode(product.sold_by.as_deref()) && !product.variants.is_empty() {
            product.stock = product.variants.iter().map(|variant| variant.stock).sum();
            product.low_stock = product
                .variants
                .iter()
                .map(|variant| variant.low_stock)
                .sum::<f64>();
        }
    }

    products.sort_by(|left, right| {
        effective_out_of_stock(left)
            .cmp(&effective_out_of_stock(right))
            .then_with(|| right.price_tiers.len().cmp(&left.price_tiers.len()))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    Ok(())
}

fn is_variants_mode(sold_by: Option<&str>) -> bool {
    let value = sold_by
        .unwrap_or("Each")
        .trim()
        .replace('_', " ")
        .to_lowercase();
    value == "variant" || value == "variants" || value.ends_with(" variants")
}

fn is_service_mode(sold_by: Option<&str>) -> bool {
    let value = sold_by
        .unwrap_or("Each")
        .trim()
        .replace('_', " ")
        .to_lowercase();
    value == "service" || value == "services" || value.ends_with(" service")
}

fn effective_out_of_stock(product: &Product) -> bool {
    !is_service_mode(product.sold_by.as_deref()) && product.stock <= 0.0
}
