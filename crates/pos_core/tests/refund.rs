use pos_core::refund_sale;
use sqlx::{postgres::PgPoolOptions, Row};

// Requires an explicitly supplied database; all fixtures live in session-local temp tables.
#[tokio::test]
#[ignore = "requires REFUND_TEST_DATABASE_URL"]
async fn refund_stock_credit_and_rollback() -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&std::env::var("REFUND_TEST_DATABASE_URL")?)
        .await?;
    sqlx::raw_sql(r#"
        SET search_path TO pg_temp;
        CREATE TEMP TABLE users(id int,username text,role text,is_active int,password_hash text,salt text);
        CREATE TEMP TABLE user_activity_log(id SERIAL PRIMARY KEY,user_id INTEGER,username TEXT,action TEXT,details TEXT,ip_address TEXT,created_at TIMESTAMP);
        CREATE TEMP TABLE sales(id int PRIMARY KEY, status text, payment_type text, invoice_no text, customer_id int);
        CREATE TEMP TABLE sale_items(id int, sale_id int, product_id int, variant_id int, qty float8, refunded_qty real DEFAULT 0, refund_reason text, location_id int, location text, batch_no text, expire_date text);
        CREATE TEMP TABLE products(id int PRIMARY KEY, stock float8, sold_by text, last_updated timestamp);
        CREATE TEMP TABLE product_variants(id int PRIMARY KEY, product_id int, stock int, updated_at timestamp);
        CREATE TEMP TABLE customers(id int PRIMARY KEY, current_balance float8);
        CREATE TEMP TABLE credit_sales(id int PRIMARY KEY, sale_id int, invoice_no text, customer_id int, total_amount float8, paid_amount float8, balance_amount float8, status text, notes text);
        CREATE TEMP TABLE credit_payments(credit_sale_id int, customer_id int, amount float8, payment_date text, payment_method text, note text);
        CREATE TEMP TABLE credit_adjustments(customer_id int, credit_sale_id int, amount float8, adjustment_type text, reason text, created_by text);
        CREATE TEMP TABLE stock_movements(product_id int, variant_id int, type text, quantity float8, old_stock float8, new_stock float8, reason text, reference text, created_by text, location text, created_at timestamp);
        CREATE TEMP TABLE product_locations(id int, product_id int, location text, batch_no text, expire_date text, quantity float8, last_updated timestamp DEFAULT CURRENT_TIMESTAMP, UNIQUE(product_id,location,batch_no,expire_date));
        CREATE TEMP TABLE variant_stock_batches(product_id int, variant_id int, location text, batch_no text, expire_date text, quantity int, expiry_unknown int, UNIQUE(product_id,variant_id,location,batch_no,expire_date));
        INSERT INTO products VALUES(1,10,'Each',NULL),(2,0,'Service',NULL),(3,4,'Variants',NULL);
        INSERT INTO product_variants VALUES(30,3,4,NULL);
        INSERT INTO sales VALUES(1,'completed','Cash','TEST-1',NULL),(2,'completed','Credit','TEST-2',1),(3,'completed','Credit','TEST-3',1);
        INSERT INTO customers VALUES(1,1000);
        INSERT INTO credit_sales VALUES(2,NULL,'TEST-2',1,500,200,300,'partial',NULL),(3,3,'TEST-3',1,700,0,700,'pending',NULL);
        INSERT INTO sale_items(id,sale_id,product_id,variant_id,qty,refunded_qty) VALUES(1,1,1,NULL,5,2),(2,1,2,NULL,1,0),(3,1,3,30,2,0),(4,2,2,NULL,1,0),(5,3,999,NULL,1,0);
    "#).execute(&pool).await?;
    let mut hash = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(b"test-only", b"salt", 100000, &mut hash);
    sqlx::query("INSERT INTO users VALUES(1,'manager','manager',1,$1,$2)")
        .bind(hex::encode(hash))
        .bind(hex::encode(b"salt"))
        .execute(&pool)
        .await?;
    let actor = pos_core::auth::login(&pool, "manager", "test-only").await?;
    refund_sale(&pool, 1, &actor).await?;
    let stocks: Vec<f64> = sqlx::query_scalar("SELECT stock FROM products ORDER BY id")
        .fetch_all(&pool)
        .await?;
    assert_eq!(stocks, vec![13.0, 0.0, 6.0]);
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT stock FROM product_variants WHERE id=30")
            .fetch_one(&pool)
            .await?,
        6
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stock_movements")
            .fetch_one(&pool)
            .await?,
        2
    );
    assert!(refund_sale(&pool, 1, &actor).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        13.0
    );
    refund_sale(&pool, 2, &actor).await?;
    let row = sqlx::query("SELECT current_balance FROM customers WHERE id=1")
        .fetch_one(&pool)
        .await?;
    assert_eq!(row.get::<f64, _>(0), 700.0);
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT amount FROM credit_payments")
            .fetch_one(&pool)
            .await?,
        -200.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT balance_amount FROM credit_sales WHERE id=2")
            .fetch_one(&pool)
            .await?,
        0.0
    );
    assert!(refund_sale(&pool, 3, &actor).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        700.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM credit_sales WHERE id=3")
            .fetch_one(&pool)
            .await?,
        "pending"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM sales WHERE id=3")
            .fetch_one(&pool)
            .await?,
        "completed"
    );
    pool.close().await;
    Ok(())
}
