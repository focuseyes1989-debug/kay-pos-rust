use super::*;
use pos_core::{
    activity,
    categories::{self, CategoryRecord},
    category_groups as groups,
};

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn credit_collection_authorization_and_audit_rollback() -> Result<()> {
    use pos_core::customer_credit as credit;
    let (pool, cashier, manager) = setup().await?;
    sqlx::raw_sql("CREATE TABLE payment_types(name TEXT,active INTEGER); INSERT INTO payment_types VALUES('Cash',1); ALTER TABLE credit_payments ADD COLUMN reference_no TEXT;").execute(&pool).await?;
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert!(
        credit::create(&pool, &cashier, 1, "CR1", "100", "0", &date, &date, "")
            .await
            .is_err()
    );
    credit::create(&pool, &manager, 1, "CR1", "100", "0", &date, &date, "").await?;
    assert!(
        credit::collect(&pool, &cashier, 1, None, "20", &date, "Cash", "", "")
            .await
            .is_err()
    );
    credit::collect(&pool, &manager, 1, None, "20", &date, "Cash", "", "").await?;
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        80.0
    );
    sqlx::raw_sql("CREATE FUNCTION reject_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced audit failure'; END $$; CREATE TRIGGER reject_audit BEFORE INSERT ON user_activity_log FOR EACH ROW EXECUTE FUNCTION reject_audit();").execute(&pool).await?;
    assert!(
        credit::collect(&pool, &manager, 1, None, "20", &date, "Cash", "", "")
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        80.0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM credit_payments")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn stock_in_reauthenticates_and_records_real_operator() -> Result<()> {
    let (pool, cashier, manager) = purchase_fixture().await?;
    let receipt = pos_core::inventory::Receipt {
        supplier_id: None,
        product_id: 1,
        variant_id: None,
        quantity: 2,
        unit_cost: 50.0,
        location: "Shop".into(),
        batch: "PHASE6".into(),
        expiry: String::new(),
        reason: "Correction".into(),
        reference: "TEST".into(),
        received_by: "forged-name".into(),
        notes: String::new(),
        expected_stock: 10.0,
        expected_location_stock: None,
    };
    assert!(pos_core::inventory::receive(&pool, &receipt, &cashier)
        .await
        .is_err());
    pos_core::inventory::receive(&pool, &receipt, &manager).await?;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT created_by FROM stock_movements ORDER BY id DESC LIMIT 1"
        )
        .fetch_one(&pool)
        .await?,
        "supervisor"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT username FROM user_activity_log ORDER BY id DESC LIMIT 1"
        )
        .fetch_one(&pool)
        .await?,
        "supervisor"
    );
    sqlx::query("UPDATE users SET is_active=0 WHERE username='supervisor'")
        .execute(&pool)
        .await?;
    assert!(pos_core::inventory::receive(&pool, &receipt, &manager)
        .await
        .is_err());
    pool.close().await;
    Ok(())
}

async fn setup() -> Result<(PgPool, Session, Session)> {
    anyhow::ensure!(
        std::env::var("P1_TEST_DATABASE_URL")?.contains("127.0.0.1:55487/"),
        "Isolated test database required"
    );
    let data = fixture().await?;
    sqlx::raw_sql("CREATE TABLE categories(id SERIAL PRIMARY KEY,name TEXT UNIQUE,parent_id INTEGER,description TEXT,status TEXT DEFAULT 'active',is_system INTEGER DEFAULT 0,sort_order INTEGER DEFAULT 0,updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP); ALTER TABLE products ADD COLUMN category TEXT,ADD COLUMN category_id INTEGER;").execute(&data.0).await?;
    for _ in 0..2 {
        sqlx::raw_sql(include_str!("../../../../migrations/009_audit_groups.sql"))
            .execute(&data.0)
            .await?;
    }
    Ok(data)
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn groups_assignments_permissions_and_stale_edits() -> Result<()> {
    let (pool, cashier, manager) = setup().await?;
    assert!(groups::reserve(&pool, &cashier).await.is_err());
    let mut group = groups::reserve(&pool, &manager).await?;
    group.name = "Stationery".into();
    groups::save(&pool, &manager, None, &group).await?;
    groups::save(&pool, &manager, None, &group).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_activity_log")
            .fetch_one(&pool)
            .await?,
        1
    );
    let mut other = groups::reserve(&pool, &manager).await?;
    other.name = "  stationery ".into();
    assert!(groups::save(&pool, &manager, None, &other).await.is_err());
    let base = CategoryRecord {
        status: "active".into(),
        ..Default::default()
    };
    let category = CategoryRecord {
        name: "Paper".into(),
        group_id: Some(group.id),
        ..base.clone()
    };
    assert!(categories::save(&pool, &cashier, &category, &base)
        .await
        .is_err());
    categories::save(&pool, &manager, &category, &base).await?;
    let old = categories::list(&pool, &manager).await?.remove(0);
    assert_eq!(old.group_id, Some(group.id));
    let mut changed = old.clone();
    changed.description = "Fresh".into();
    categories::save(&pool, &manager, &changed, &old).await?;
    assert!(categories::save(&pool, &manager, &old, &old).await.is_err());
    assert!(categories::delete(&pool, &cashier, old.id).await.is_err());
    let mut inactive = group.clone();
    inactive.is_active = 0;
    groups::save(&pool, &manager, Some(&group), &inactive).await?;
    assert!(groups::save(&pool, &manager, Some(&group), &group)
        .await
        .is_err());
    let new_category = CategoryRecord {
        name: "Books".into(),
        group_id: Some(group.id),
        ..base.clone()
    };
    assert!(categories::save(&pool, &manager, &new_category, &base)
        .await
        .is_err());
    sqlx::query("UPDATE users SET is_active=0 WHERE username='supervisor'")
        .execute(&pool)
        .await?;
    assert!(groups::list(&pool, &manager).await.is_err());
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn audit_search_retry_and_transaction_rollback() -> Result<()> {
    let (pool, cashier, manager) = setup().await?;
    let request = draft(vec![item(1, 1.0)]);
    complete_sale(&pool, &request, &cashier).await?;
    complete_sale(&pool, &request, &cashier).await?;
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert!(activity::list(&pool, &cashier, &date, &date, "", 0)
        .await
        .is_err());
    let rows = activity::list(&pool, &manager, &date, &date, "rust.sale.complete", 0).await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].username, "operator");
    assert!(activity::list(&pool, &manager, &date, &date, "%", 0)
        .await?
        .is_empty());
    sqlx::raw_sql("CREATE FUNCTION reject_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'forced audit failure'; END $$; CREATE TRIGGER reject_audit BEFORE INSERT ON user_activity_log FOR EACH ROW EXECUTE FUNCTION reject_audit();").execute(&pool).await?;
    assert!(complete_sale(&pool, &draft(vec![item(1, 1.0)]), &cashier)
        .await
        .is_err());
    assert_eq!(
        sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        9.0
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
