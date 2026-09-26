use super::*;
use pos_core::ai;

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn ai_permissions_readonly_and_audit() -> Result<()> {
    let (pool, cashier, manager) = fixture().await?;
    sqlx::raw_sql("ALTER TABLE users ADD COLUMN permissions TEXT; CREATE TABLE user_roles(name TEXT PRIMARY KEY,permissions TEXT); INSERT INTO user_roles VALUES('manager','ai_pages,products,sales_summary,dashboard,expense,credit'); ALTER TABLE products ADD COLUMN name TEXT DEFAULT 'Test product', ADD COLUMN sku TEXT DEFAULT 'SKU', ADD COLUMN barcode TEXT, ADD COLUMN price NUMERIC DEFAULT 100, ADD COLUMN low_stock INTEGER DEFAULT 2; CREATE TABLE expenses(amount NUMERIC,expense_date TEXT); INSERT INTO sales(invoice_no,total,status,payment_type,created_at) VALUES('AI-CREDIT',45000,'completed','Credit','2026-09-25'),('AI-CASH',20000,'completed','Cash','2026-09-25');").execute(&pool).await?;
    let r = ai::chat("dashboard", "2026-09-25", "2026-09-25")?;
    assert!(ai::ask(&pool, &cashier, &r).await.is_err());
    let answer = ai::ask(&pool, &manager, &r).await?;
    assert!(answer
        .rows
        .iter()
        .any(|r| r[0] == "ပြီးစီးသည့် အရောင်းစုစုပေါင်း" && r[1] == "65000"));
    assert!(answer
        .rows
        .iter()
        .any(|r| r[0] == "အကြွေးရောင်းစုစုပေါင်း (အရောင်းစုစုပေါင်းတွင် ပါဝင်သည်)" && r[1] == "45000"));
    let product = ai::chat("product Test", "2026-09-25", "2026-09-25")?;
    assert_eq!(ai::ask(&pool, &manager, &product).await?.rows.len(), 4);
    let literal = ai::chat(
        "product '; DROP TABLE sales; --",
        "2026-09-25",
        "2026-09-25",
    )?;
    assert!(ai::ask(&pool, &manager, &literal).await?.rows.is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await?,
        2
    );
    let details: Vec<String> =
        sqlx::query_scalar("SELECT details FROM user_activity_log WHERE action='rust.ai.query'")
            .fetch_all(&pool)
            .await?;
    assert_eq!(details.len(), 3);
    assert!(details
        .iter()
        .all(|d| d.contains("request_hash=") && !d.contains("DROP TABLE") && !d.contains("Test")));
    sqlx::query("UPDATE user_roles SET permissions='ai_pages,products'")
        .execute(&pool)
        .await?;
    assert!(ai::ask(&pool, &manager, &r).await.is_err());
    assert!(ai::ask(&pool, &manager, &product).await.is_ok());
    sqlx::query("UPDATE user_roles SET permissions='products'")
        .execute(&pool)
        .await?;
    assert!(ai::ask(&pool, &manager, &product).await.is_err());
    sqlx::query("UPDATE user_roles SET permissions='ai_pages,products'")
        .execute(&pool)
        .await?;
    sqlx::raw_sql("CREATE FUNCTION reject_ai_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test audit unavailable'; END $$; CREATE TRIGGER reject_ai_audit BEFORE INSERT ON user_activity_log FOR EACH ROW EXECUTE FUNCTION reject_ai_audit();").execute(&pool).await?;
    assert!(ai::ask(&pool, &manager, &product).await.is_err());
    pool.close().await;
    Ok(())
}
