use anyhow::Result;
use pos_core::{catalog, db, Product};
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
#[ignore = "Requires isolated CATALOG_TEST_DATABASE_URL on port 55487"]
async fn description_round_trips_and_clears_without_changing_stock() -> Result<()> {
    let url = std::env::var("CATALOG_TEST_DATABASE_URL")?;
    anyhow::ensure!(url.contains("127.0.0.1:55487/"), "Use isolated local test server");
    // One connection keeps all test tables temporary and off the production schema.
    let pool = PgPoolOptions::new().max_connections(1).connect(&url).await?;
    sqlx::raw_sql(r#"
        CREATE TEMP TABLE categories(id int PRIMARY KEY, name text);
        CREATE TEMP TABLE products(id serial PRIMARY KEY, name text, description text,
            category_id int, category text, sku text, barcode text, price float8,
            cost float8, low_stock float8, sold_by text, stock float8,
            image_filename text, image_data bytea, image_mime text, is_favourite int DEFAULT 0);
        CREATE TEMP TABLE product_variants(id serial PRIMARY KEY, product_id int,
            size text, color text, sku text, barcode text, price float8, cost float8,
            stock int, low_stock float8, wholesale_min_qty int, wholesale_price float8, active int);
        CREATE TEMP TABLE product_price_tiers(id serial PRIMARY KEY, product_id int,
            min_qty int, unit_multiplier int, unit_label text, unit_price float8, active int);
    "#).execute(&pool).await?;
    let text = "\u{1019}\u{103c}\u{1014}\u{103a}\u{1019}\u{102c}\nProduct details 'quoted' <plain text>";
    let mut product = Product {
        promotion: None,
        id: 0, name: "Description test".into(), description: Some(text.into()),
        category_id: None, category_name: None, sku: Some("DESC-1".into()), barcode: None,
        price: 100.0, cost: 40.0, stock: 0.0, low_stock: 2.0, sold_by: Some("Each".into()),
        image_filename: None, image_data_url: None, variants: vec![], price_tiers: vec![],
    };
    catalog::save_with_image(&pool, &product, None).await?;
    product = db::search_product_metadata(&pool, "Description test", 1).await?.remove(0);
    assert_eq!(product.description.as_deref(), Some(text));
    sqlx::query("UPDATE products SET stock=17,description='Main POS description' WHERE id=$1")
        .bind(product.id).execute(&pool).await?;
    product = db::search_product_metadata(&pool, "Description test", 1).await?.remove(0);
    assert_eq!(product.description.as_deref(), Some("Main POS description"));
    product.description = Some(format!("{text}\nEdited"));
    catalog::save_with_image(&pool, &product, None).await?;
    let read = db::search_product_metadata(&pool, "Description test", 1).await?.remove(0);
    assert_eq!(read.description, product.description);
    assert_eq!(read.stock, 17.0);
    for description in [Some(String::new()), None] {
        product.description = description;
        catalog::save_with_image(&pool, &product, None).await?;
        let read = db::search_product_metadata(&pool, "Description test", 1).await?.remove(0);
        assert_eq!(read.description, product.description);
        assert_eq!(read.stock, 17.0);
    }
    // Older serialized cart/product records have no description field.
    let mut legacy = serde_json::to_value(&product)?;
    legacy.as_object_mut().unwrap().remove("description");
    assert_eq!(serde_json::from_value::<Product>(legacy)?.description, None);
    pool.close().await;
    Ok(())
}
