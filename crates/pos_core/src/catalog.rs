use crate::models::Product;
use anyhow::{ensure, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use sqlx::PgPool;

#[derive(Clone)]
pub struct ProductImage {
    data: Vec<u8>,
    mime: &'static str,
    filename: String,
}

impl ProductImage {
    pub fn from_bytes(filename: String, data: Vec<u8>) -> Result<Self> {
        ensure!(
            data.len() <= 10 * 1024 * 1024,
            "Image must be 10 MB or smaller"
        );
        let mime = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if data.starts_with(b"\xff\xd8\xff") {
            "image/jpeg"
        } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            "image/gif"
        } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            anyhow::bail!("Choose a PNG, JPEG, GIF or WebP image");
        };
        Ok(Self {
            data,
            mime,
            filename,
        })
    }

    pub fn data_url(&self) -> String {
        format!("data:{};base64,{}", self.mime, STANDARD.encode(&self.data))
    }
}

pub async fn save(pool: &PgPool, p: &Product) -> Result<()> {
    save_with_image(pool, p, None).await
}

#[cfg(test)]
mod image_tests {
    use super::ProductImage;

    #[test]
    fn image_preview_uses_detected_mime() {
        let image = ProductImage::from_bytes("photo.png".into(), b"\xff\xd8\xff".to_vec()).unwrap();
        assert_eq!(image.mime, "image/jpeg");
        assert_eq!(image.data_url(), "data:image/jpeg;base64,/9j/");
        assert_eq!(image.filename, "photo.png");
    }

    #[test]
    fn rejects_unsupported_and_oversized_images() {
        assert!(ProductImage::from_bytes("bad.png".into(), b"not an image".to_vec()).is_err());
        assert!(ProductImage::from_bytes("empty.png".into(), vec![]).is_err());
        assert!(
            ProductImage::from_bytes("large.png".into(), vec![0; 10 * 1024 * 1024 + 1]).is_err()
        );
    }
}

pub async fn save_with_image(
    pool: &PgPool,
    p: &Product,
    image: Option<&ProductImage>,
) -> Result<()> {
    validate_modes(p)?;
    ensure!(!p.name.trim().is_empty(), "Product name is required");
    ensure!(
        [p.price, p.cost, p.low_stock]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0),
        "Prices and low stock must be non-negative numbers"
    );
    let mut tx = pool.begin().await?;
    if p.id != 0 {
        let old: (String,f64) = sqlx::query_as("SELECT COALESCE(sold_by,'Each'),COALESCE(stock,0)::float8 FROM products WHERE id=$1 FOR UPDATE").bind(p.id).fetch_one(&mut *tx).await?;
        ensure!(
            old.0
                .eq_ignore_ascii_case(p.sold_by.as_deref().unwrap_or("Each"))
                || old.1 == 0.0,
            "Clear stock through Inventory before changing product type"
        );
    }
    let category: Option<String> = if let Some(id) = p.category_id {
        Some(
            sqlx::query_scalar("SELECT name FROM categories WHERE id=$1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?,
        )
    } else {
        None
    };
    let sql = if p.id == 0 {
        "INSERT INTO products (name,category_id,category,sku,barcode,price,cost,low_stock,sold_by,description,stock) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,0) RETURNING id"
    } else {
        "UPDATE products SET name=$1,category_id=$2,category=$3,sku=$4,barcode=$5,price=$6,cost=$7,low_stock=$8,sold_by=$9,description=$10 WHERE id=$11 RETURNING id"
    };
    let q = sqlx::query_scalar::<_, i32>(sql)
        .bind(p.name.trim())
        .bind(p.category_id)
        .bind(category)
        .bind(&p.sku)
        .bind(&p.barcode)
        .bind(p.price)
        .bind(p.cost)
        .bind(p.low_stock)
        .bind(&p.sold_by)
        .bind(&p.description);
    let id = if p.id == 0 {
        q.fetch_one(&mut *tx).await?
    } else {
        q.bind(p.id).fetch_one(&mut *tx).await?
    };
    if p.id == 0 && p.sku.as_deref().unwrap_or("").trim().is_empty() {
        let sku = format!("P{id:05}");
        let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE sku=$1 AND id<>$2 UNION ALL SELECT 1 FROM product_variants WHERE sku=$1)")
            .bind(&sku).bind(id).fetch_one(&mut *tx).await?;
        ensure!(!used, "Generated SKU already exists; enter a unique SKU");
        sqlx::query("UPDATE products SET sku=$1 WHERE id=$2")
            .bind(sku).bind(id).execute(&mut *tx).await?;
    }
    if let Some(image) = image {
        sqlx::query(
            "UPDATE products SET image_data=$1,image_mime=$2,image_filename=$3 WHERE id=$4",
        )
        .bind(&image.data)
        .bind(image.mime)
        .bind(&image.filename)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    let variant_mode = p
        .sold_by
        .as_deref()
        .unwrap_or("Each")
        .eq_ignore_ascii_case("Variants");
    let service = p
        .sold_by
        .as_deref()
        .unwrap_or("Each")
        .eq_ignore_ascii_case("Service");
    let keep: Vec<i32> = if variant_mode {
        p.variants
            .iter()
            .map(|v| v.variant_id)
            .filter(|id| *id > 0)
            .collect()
    } else {
        vec![]
    };
    let removed_stock:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_variants WHERE product_id=$1 AND COALESCE(active,1)=1 AND NOT(id=ANY($2)) AND COALESCE(stock,0)<>0)").bind(id).bind(&keep).fetch_one(&mut *tx).await?;
    ensure!(
        !removed_stock,
        "A variant with stock cannot be removed or disabled"
    );
    sqlx::query("UPDATE product_variants SET active=0 WHERE product_id=$1 AND NOT(id=ANY($2))")
        .bind(id)
        .bind(&keep)
        .execute(&mut *tx)
        .await?;
    if variant_mode {
        for v in &p.variants {
            let sql = if v.variant_id == 0 {
                "INSERT INTO product_variants (product_id,size,color,sku,barcode,price,cost,low_stock,wholesale_min_qty,wholesale_price,stock,active) VALUES ($1,$2,$3,$4,$5,$6::float8,$7::float8,$8,$9,$10::float8,0,1)"
            } else {
                "UPDATE product_variants SET size=$2,color=$3,sku=$4,barcode=$5,price=$6::float8,cost=$7::float8,low_stock=$8,wholesale_min_qty=$9,wholesale_price=$10::float8,active=1 WHERE product_id=$1 AND id=$11"
            };
            let sku = if p.id == 0 && v.variant_id == 0 {
                v.sku
                    .as_ref()
                    .map(|sku| match sku.strip_prefix("VAR-NEW-") {
                        Some(suffix) if suffix.parse::<u32>().is_ok() => {
                            format!("P{id:05}-{suffix}")
                        }
                        _ => sku.clone(),
                    })
            } else {
                v.sku.clone()
            };
            let q = sqlx::query(sql)
                .bind(id)
                .bind(&v.size)
                .bind(&v.color)
                .bind(&sku)
                .bind(&v.barcode)
                .bind(v.price)
                .bind(v.cost)
                .bind(v.low_stock as i32)
                .bind(v.wholesale_min_qty)
                .bind(v.wholesale_price);
            let result = if v.variant_id == 0 {
                q.execute(&mut *tx).await?
            } else {
                q.bind(v.variant_id).execute(&mut *tx).await?
            };
            ensure!(result.rows_affected() == 1, "Variant no longer exists");
        }
        sqlx::query("UPDATE products SET stock=(SELECT COALESCE(SUM(stock),0) FROM product_variants WHERE product_id=$1 AND active=1) WHERE id=$1").bind(id).execute(&mut *tx).await?;
    }
    let tiers = if !variant_mode && !service {
        p.price_tiers.as_slice()
    } else {
        &[]
    };
    let keep: Vec<i32> = tiers.iter().map(|t| t.id).filter(|id| *id > 0).collect();
    sqlx::query("UPDATE product_price_tiers SET active=0 WHERE product_id=$1 AND NOT(id=ANY($2))")
        .bind(id)
        .bind(&keep)
        .execute(&mut *tx)
        .await?;
    for t in tiers {
        let sql = if t.id == 0 {
            "INSERT INTO product_price_tiers(product_id,min_qty,unit_label,unit_price,unit_multiplier,active) VALUES ($1,$2,$3,$4::float8,$5,1)"
        } else {
            "UPDATE product_price_tiers SET min_qty=$2,unit_label=$3,unit_price=$4::float8,unit_multiplier=$5,active=1 WHERE product_id=$1 AND id=$6"
        };
        let q = sqlx::query(sql)
            .bind(id)
            .bind(t.min_qty)
            .bind(&t.unit_label)
            .bind(t.unit_price)
            .bind(t.unit_multiplier);
        let result = if t.id == 0 {
            q.execute(&mut *tx).await?
        } else {
            q.bind(t.id).execute(&mut *tx).await?
        };
        ensure!(
            result.rows_affected() == 1,
            "Wholesale tier no longer exists"
        );
    }
    tx.commit().await?;
    Ok(())
}

fn validate_modes(p: &Product) -> Result<()> {
    let mode = p.sold_by.as_deref().unwrap_or("Each");
    ensure!(
        ["Each", "Service", "Variants", "Wholesale"]
            .iter()
            .any(|m| m.eq_ignore_ascii_case(mode)),
        "Unsupported product type"
    );
    let mut quantities = std::collections::HashSet::new();
    let mut codes = std::collections::HashSet::new();
    for code in std::iter::once(p.barcode.as_deref())
        .chain(p.variants.iter().map(|v| v.barcode.as_deref()))
        .flatten()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        ensure!(
            codes.insert(code),
            "Product and variant barcodes must be unique"
        );
    }
    for t in &p.price_tiers {
        ensure!(
            t.min_qty > 0
                && quantities.insert(t.min_qty)
                && t.unit_multiplier > 0
                && t.unit_price.is_finite()
                && t.unit_price > 0.0,
            "Wholesale quantities must be positive and unique; prices must be greater than zero"
        );
    }
    if mode.eq_ignore_ascii_case("Variants") {
        ensure!(!p.variants.is_empty(), "Add at least one variant");
    }
    for v in &p.variants {
        ensure!(
            [v.price, v.cost, v.low_stock, v.wholesale_price]
                .iter()
                .all(|x| x.is_finite() && *x >= 0.0),
            "Variant values must be non-negative numbers"
        );
        ensure!(
            v.low_stock.fract() == 0.0 && v.low_stock <= i32::MAX as f64,
            "Variant low stock must be a whole number"
        );
        ensure!(
            (v.wholesale_min_qty == 0 && v.wholesale_price == 0.0)
                || (v.wholesale_min_qty > 0 && v.wholesale_price > 0.0),
            "Variant wholesale quantity and price must both be set"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn product() -> Product {
        Product {
            id: 0,
            name: "Test".into(),
            description: None,
            category_id: None,
            category_name: None,
            sku: None,
            barcode: None,
            price: 0.0,
            cost: 0.0,
            stock: 0.0,
            low_stock: 0.0,
            sold_by: Some("Each".into()),
            image_filename: None,
            image_data_url: None,
            variants: vec![],
            price_tiers: vec![],
        }
    }
    #[test]
    fn requires_variants_and_valid_tiers() {
        let mut p = product();
        assert!(validate_modes(&p).is_ok());
        p.sold_by = Some("Service".into());
        assert!(validate_modes(&p).is_ok());
        p.sold_by = Some("Variants".into());
        assert!(validate_modes(&p).is_err());
        p.sold_by = Some("Each".into());
        let tier = crate::models::ProductPriceTier {
            id: 0,
            product_id: 0,
            min_qty: 3,
            unit_multiplier: 1,
            unit_label: None,
            unit_price: 100.0,
        };
        p.price_tiers.push(tier.clone());
        assert!(validate_modes(&p).is_ok());
        p.price_tiers.push(tier);
        assert!(validate_modes(&p).is_err());
        p.price_tiers.pop();
        p.price_tiers[0].unit_price = f64::NAN;
        assert!(validate_modes(&p).is_err());
    }
}

pub async fn delete(pool: &PgPool, id: i32) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM products WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let linked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sale_items WHERE product_id=$1 UNION ALL SELECT 1 FROM stock_movements WHERE product_id=$1 UNION ALL SELECT 1 FROM product_variants WHERE product_id=$1)").bind(id).fetch_one(&mut *tx).await?;
    ensure!(
        !linked,
        "Product has variants or transaction history and cannot be deleted here"
    );
    sqlx::query("DELETE FROM products WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
