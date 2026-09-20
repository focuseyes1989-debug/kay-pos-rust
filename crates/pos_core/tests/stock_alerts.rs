use anyhow::Result;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn alerts_exclude_services_and_inactive_variants() -> Result<()> {
    let url=std::env::var("P1_TEST_DATABASE_URL")?;
    let schema=format!("alerts_{}",pos_core::auth::new_request_id().replace("RUST-",""));
    let setup=PgPoolOptions::new().max_connections(1).connect(&url).await?;
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&setup).await?;
    let schema_for_pool=schema.clone();
    let pool=PgPoolOptions::new().max_connections(1).after_connect(move |c,_| {
        let q=format!("SET search_path TO {schema_for_pool}");
        Box::pin(async move {sqlx::query(&q).execute(c).await?;Ok(())})
    }).connect(&url).await?;
    sqlx::raw_sql("CREATE TABLE products(id int,name text,sku text,sold_by text,stock float8,low_stock float8);
        CREATE TABLE product_variants(id int,product_id int,size text,color text,sku text,stock int,low_stock int,active int);
        INSERT INTO products VALUES(1,'Low','A','Each',3,3),(2,'Empty','B','Each',0,0),
        (3,'Service','','Service',0,5),(4,'Parent','','Variants',0,5),(5,'Healthy','','Each',8,3),
        (6,'Disabled threshold','','Each',2,0),(7,'Negative','','Each',-1,0);
        INSERT INTO product_variants VALUES(40,4,'Small',NULL,'VS',2,2,1),(41,4,'Large',NULL,'VL',0,5,0),(42,4,'XL',NULL,'VX',0,0,1);")
        .execute(&pool).await?;
    let rows=pos_core::stock_alerts::list(&pool).await?;
    assert_eq!(rows.len(),5);
    assert_eq!(rows.iter().filter(|r|r.stock<=0.0).count(),3);
    assert!(rows.iter().any(|r|r.variant_id==Some(40)&&r.name=="Parent / Small"));
    assert!(!rows.iter().any(|r|r.product_id==3||r.variant_id==Some(41)));
    sqlx::query("UPDATE products SET stock=20 WHERE id=1").execute(&pool).await?;
    assert_eq!(pos_core::stock_alerts::list(&pool).await?.len(),4);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&setup).await?;
    setup.close().await;
    Ok(())
}
