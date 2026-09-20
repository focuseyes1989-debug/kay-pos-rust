use anyhow::Result;
use pos_core::{
    auth::{self, Session},
    complete_sale, refund_sale, SaleDraft, SaleItemDraft,
};
use sqlx::{postgres::PgPoolOptions, PgPool};

async fn fixture() -> Result<(PgPool, Session, Session)> {
    let url = std::env::var("P1_TEST_DATABASE_URL")?;
    let bootstrap = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let schema = format!("p1_{}", auth::new_request_id().replace("RUST-", ""));
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&bootstrap)
        .await?;
    bootstrap.close().await;
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(move |conn, _| {
            let command = format!("SET search_path TO {schema}");
            Box::pin(async move {
                sqlx::query(&command).execute(&mut *conn).await?;
                sqlx::query("SET statement_timeout='10s'")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    sqlx::raw_sql(include_str!("fixtures/checkout.sql"))
        .execute(&pool)
        .await?;
    sqlx::raw_sql(include_str!(
        "../../../migrations/001_checkout_requests.sql"
    ))
    .execute(&pool)
    .await?;
    let mut hash = [0u8; 32];
    sqlx::raw_sql(include_str!("../../../migrations/002_short_numbers.sql"))
        .execute(&pool).await?;
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(b"test-only", b"fixture-salt", 100000, &mut hash);
    for (name, role) in [("operator", "cashier"), ("supervisor", "manager")] {
        sqlx::query("INSERT INTO users(username,role,password_hash,salt) VALUES($1,$2,$3,$4)")
            .bind(name)
            .bind(role)
            .bind(hex::encode(hash))
            .bind(hex::encode(b"fixture-salt"))
            .execute(&pool)
            .await?;
    }
    let cashier = auth::login(&pool, "operator", "test-only").await?;
    let manager = auth::login(&pool, "supervisor", "test-only").await?;
    Ok((pool, cashier, manager))
}

fn item(id: i32, qty: f64) -> SaleItemDraft {
    SaleItemDraft {
        product_id: id,
        variant_id: (id == 3).then_some(30),
        product_name: format!("Product {id}"),
        qty,
        price: 100.0,
        cost: 50.0,
        is_service: id == 2,
        wholesale_regular_price: 100.0,
        wholesale_savings: 0.0,
        wholesale_tier_min_qty: None,
        wholesale_unit_label: None,
    }
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn short_numbers_are_reserved_once_and_skip_existing_batches() -> Result<()> {
    let (pool, _, _) = fixture().await?;
    sqlx::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES(1,'Shop','B000001','',0)")
        .execute(&pool).await?;
    let mut a = pool.acquire().await?;
    let mut b = pool.acquire().await?;
    let (first, second) = tokio::join!(
        pos_core::numbers::batch(&mut a),
        pos_core::numbers::batch(&mut b)
    );
    let first = first?;
    let second = second?;
    assert_ne!(first, second);
    assert_ne!(first, "B000001");
    assert_ne!(second, "B000001");
    assert_eq!(first.len(), 7);
    assert_eq!(second.len(), 7);
    Ok(())
}
fn draft(items: Vec<SaleItemDraft>) -> SaleDraft {
    let total = items.iter().map(|i| i.qty * i.price).sum();
    SaleDraft {
        invoice_no: auth::new_request_id(),
        customer_id: None,
        payment_type: "Cash".into(),
        payment: total,
        expected_total: total,
        discount_amount: 0.0,
        created_by: "operator".into(),
        items,
    }
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn batches_refund_and_credit_are_atomic() -> Result<()> {
    let (pool, cashier, manager) = fixture().await?;
    let mut request = draft(vec![item(1, 6.0), item(3, 5.0), item(2, 1.0)]);
    request.payment_type = "Credit".into();
    request.customer_id = Some(1);
    request.payment = 200.0;
    let sale = complete_sale(&pool, &request, &cashier).await?;
    assert_eq!(sale.invoice_no, "INV000001");
    assert_ne!(sale.invoice_no, request.invoice_no);
    let retry = complete_sale(&pool, &request, &cashier).await?;
    assert_eq!(retry.id, sale.id);
    assert_eq!(retry.invoice_no, sale.invoice_no);
    assert_eq!(sqlx::query_scalar::<_, String>("SELECT invoice_no FROM credit_sales WHERE sale_id=$1").bind(sale.id).fetch_one(&pool).await?, sale.invoice_no);
    let stocks: Vec<f64> = sqlx::query_scalar("SELECT stock FROM products ORDER BY id")
        .fetch_all(&pool)
        .await?;
    assert_eq!(stocks, vec![4.0, 0.0, 5.0, 10.0]);
    assert_eq!(
        sqlx::query_scalar::<_, f64>(
            "SELECT SUM(quantity) FROM product_locations WHERE product_id=1"
        )
        .fetch_one(&pool)
        .await?,
        4.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT SUM(quantity) FROM variant_stock_batches")
            .fetch_one(&pool)
            .await?,
        5
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sale_items WHERE location IS NOT NULL")
            .fetch_one(&pool)
            .await?,
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        1000.0
    );
    assert!(refund_sale(&pool, sale.id, &cashier).await.is_err());
    refund_sale(&pool, sale.id, &manager).await?;
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        0.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, f64>(
            "SELECT SUM(quantity) FROM product_locations WHERE product_id=1"
        )
        .fetch_one(&pool)
        .await?,
        10.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT SUM(quantity) FROM variant_stock_batches")
            .fetch_one(&pool)
            .await?,
        10
    );
    assert!(refund_sale(&pool, sale.id, &manager).await.is_err());
    let bad = draft(vec![item(1, 1.0), item(3, 99.0)]);
    assert!(complete_sale(&pool, &bad, &cashier).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        10.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn retries_cancellation_and_payload_mismatch() -> Result<()> {
    let (pool, cashier, _) = fixture().await?;
    let request = draft(vec![item(1, 2.0)]);
    let (a, b) = tokio::join!(
        complete_sale(&pool, &request, &cashier),
        complete_sale(&pool, &request, &cashier)
    );
    assert_eq!(a?.id, b?.id);
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        8.0
    );
    let mut changed = request.clone();
    changed.payment += 1.0;
    assert!(complete_sale(&pool, &changed, &cashier).await.is_err());
    assert!(!pos_core::sales::cancel_uncommitted(&pool, &request, &cashier).await?);
    let cancelled = draft(vec![item(4, 1.0)]);
    assert!(pos_core::sales::cancel_uncommitted(&pool, &cancelled, &cashier).await?);
    assert!(complete_sale(&pool, &cancelled, &cashier).await.is_err());
    // A new connection after a lost reply still retrieves the original sale.
    assert!(complete_sale(&pool, &request, &cashier).await.is_ok());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn reversed_cart_order_and_refund_do_not_deadlock() -> Result<()> {
    let (pool, cashier, manager) = fixture().await?;
    let first = draft(vec![item(3, 1.0), item(1, 1.0)]);
    let second = draft(vec![item(1, 1.0), item(3, 1.0)]);
    let (a, b) = tokio::join!(
        complete_sale(&pool, &first, &cashier),
        complete_sale(&pool, &second, &cashier)
    );
    let a = a?;
    let b = b?;
    let third = draft(vec![item(3, 1.0), item(1, 1.0)]);
    let (sale, refund) = tokio::join!(
        complete_sale(&pool, &third, &cashier),
        refund_sale(&pool, a.id, &manager)
    );
    sale?;
    refund?;
    refund_sale(&pool, b.id, &manager).await?;
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=3")
            .fetch_one(&pool)
            .await?,
        9.0
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn account_revocation_and_invalid_amounts_fail_closed() -> Result<()> {
    let (pool, cashier, _) = fixture().await?;
    assert!(auth::login(&pool, "operator", "wrong").await.is_err());
    let mut bad = draft(vec![item(1, -1.0)]);
    assert!(complete_sale(&pool, &bad, &cashier).await.is_err());
    bad = draft(vec![item(1, f64::NAN)]);
    assert!(complete_sale(&pool, &bad, &cashier).await.is_err());
    let good = draft(vec![item(1, 1.0)]);
    sqlx::query("UPDATE users SET is_active=0 WHERE username='operator'")
        .execute(&pool)
        .await?;
    assert!(complete_sale(&pool, &good, &cashier).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await?,
        0
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn inventory_sale_and_cancel_races_are_safe() -> Result<()> {
    let (pool, cashier, _) = fixture().await?;
    let request = draft(vec![item(3, 1.0)]);
    let adjustment = pos_core::inventory::Receipt {
        supplier_id: None,
        product_id: 3,
        variant_id: Some(30),
        quantity: 11,
        unit_cost: 50.0,
        location: "Shop".into(),
        batch: String::new(),
        expiry: String::new(),
        reason: "Test adjustment".into(),
        reference: "test".into(),
        received_by: "supervisor".into(),
        notes: String::new(),
        expected_stock: 10.0,
    };
    let (sale, stock) = tokio::join!(
        complete_sale(&pool, &request, &cashier),
        pos_core::inventory::change(&pool, &adjustment, true)
    );
    sale?;
    if let Err(e) = &stock {
        assert!(e.to_string().contains("Stock changed"), "{e:#}");
    }
    let master: f64 = sqlx::query_scalar("SELECT stock FROM products WHERE id=3")
        .fetch_one(&pool)
        .await?;
    let batches: i64 =
        sqlx::query_scalar("SELECT SUM(quantity) FROM variant_stock_batches WHERE product_id=3")
            .fetch_one(&pool)
            .await?;
    assert_eq!(master, if stock.is_ok() { 10.0 } else { 9.0 });
    assert_eq!(master, batches as f64);
    let request = draft(vec![item(4, 1.0)]);
    let (sale, cancel) = tokio::join!(
        complete_sale(&pool, &request, &cashier),
        pos_core::sales::cancel_uncommitted(&pool, &request, &cashier)
    );
    if cancel? {
        assert!(sale.is_err());
    } else {
        sale?;
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales WHERE invoice_no=$1")
        .bind(&request.invoice_no)
        .fetch_one(&pool)
        .await?;
    assert!(count <= 1);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn inconsistent_batches_and_changed_settings_fail_closed() -> Result<()> {
    let (pool, cashier, _) = fixture().await?;
    let saved = draft(vec![item(2, 1.0)]);
    let sale = complete_sale(&pool, &saved, &cashier).await?;
    sqlx::query("INSERT INTO settings VALUES('tax_enabled','1'),('tax_rate','10')")
        .execute(&pool)
        .await?;
    assert_eq!(complete_sale(&pool, &saved, &cashier).await?.id, sale.id);
    assert!(complete_sale(&pool, &draft(vec![item(1, 1.0)]), &cashier)
        .await
        .is_err());
    sqlx::query("DELETE FROM settings").execute(&pool).await?;
    sqlx::query("UPDATE product_locations SET quantity=0 WHERE product_id=1")
        .execute(&pool)
        .await?;
    assert!(complete_sale(&pool, &draft(vec![item(1, 1.0)]), &cashier)
        .await
        .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        10.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}
