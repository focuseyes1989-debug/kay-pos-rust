use anyhow::Result;
use pos_core::{
    auth, complete_sale, db, refund_sale, sqlite, storage as s, SaleDraft, SaleItemDraft,
};
async fn fixture() -> Result<(tempfile::TempDir, s::Pool, auth::Session)> {
    let directory = tempfile::tempdir()?;
    let pool = sqlite::connect(&format!(
        "sqlite://{}",
        directory.path().join("pos.db").display()
    ))
    .await?;
    assert!(sqlite::needs_admin(&pool).await?);
    sqlite::create_initial_admin(&pool, "owner", "test-password").await?;
    let actor = auth::login(&pool, "owner", "test-password").await?;
    s::query("INSERT INTO products(id,name,sold_by,price,cost,stock) VALUES(1,'Test product','Each',20000,5000,10)").execute(&pool).await?;
    s::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES(1,'Shop','OPENING','',10)").execute(&pool).await?;
    s::query("INSERT INTO customers(id,name,credit_limit,current_balance) VALUES(1,'ဦးအောင်သန်း',100000,0)").execute(&pool).await?;
    Ok((directory, pool, actor))
}
fn draft(method: &str, paid: f64) -> SaleDraft {
    SaleDraft {
        location_id: None,
        held_token: None,
        invoice_no: auth::new_request_id(),
        customer_id: Some(1),
        payment_type: method.into(),
        payment: paid,
        discount_amount: 0.0,
        expected_total: 20000.0,
        created_by: "owner".into(),
        items: vec![SaleItemDraft {
        serials: Vec::new(),
            promotion: None,
            product_id: 1,
            variant_id: None,
            product_name: "Test product".into(),
            qty: 1.0,
            price: 20000.0,
            cost: 5000.0,
            is_service: false,
            wholesale_regular_price: 0.0,
            wholesale_savings: 0.0,
            wholesale_tier_min_qty: None,
            wholesale_unit_label: None,
        }],
    }
}
#[tokio::test]
async fn standalone_credit_collection_retry_refund_and_reopen() -> Result<()> {
    let (dir, pool, actor) = fixture().await?;
    let request = draft("Credit", 0.0);
    let receipt = complete_sale(&pool, &request, &actor).await?;
    assert_eq!(receipt.invoice_no, "INV000001");
    let detail=pos_core::get_receipt_detail(&pool,receipt.id).await?;
    let customer=pos_core::customers::list(&pool).await?.into_iter().find(|c|c.id==1).unwrap();
    assert_eq!(detail.customer.as_ref().unwrap().name,customer.name);
    assert_eq!(detail.customer.as_ref().unwrap().address,customer.address);

    assert_eq!(complete_sale(&pool, &request, &actor).await?.id, receipt.id);
    let balance: f64 = s::query_scalar("SELECT current_balance FROM customers WHERE id=1")
        .fetch_one(&pool)
        .await?;
    assert_eq!(balance, 20000.0);
    assert_eq!(pos_core::customers::list(&pool).await?.iter().find(|c|c.id==1).unwrap().current_balance,20000.0);
    let outstanding=pos_core::customer_credit::invoices(&pool,None).await?;
    assert!(outstanding.iter().any(|r|r.balance=="20000".parse::<rust_decimal::Decimal>().unwrap()));

    let credits = pos_core::customer_credit::invoices(&pool, Some(1)).await?;
    assert_eq!(credits.len(), 1);
    assert_eq!(
        credits[0].balance,
        "20000".parse::<rust_decimal::Decimal>()?
    );
    pos_core::customer_credit::collect(
        &pool,
        &actor,
        1,
        Some(credits[0].id),
        "5000",
        "2026-10-11",
        "Cash",
        "TEST-1",
        "",
    )
    .await?;
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        15000.0
    );
    assert!(pos_core::customer_credit::collect(
        &pool,
        &actor,
        1,
        None,
        "15001",
        "2026-10-11",
        "Cash",
        "TEST-2",
        ""
    )
    .await
    .is_err());
    let detail = pos_core::get_receipt_detail(&pool, receipt.id).await?;
    assert_eq!(detail.summary.id, receipt.id);
    refund_sale(&pool, receipt.id, &actor).await?;
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        0.0
    );
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        10.0
    );
    assert!(refund_sale(&pool, receipt.id, &actor).await.is_err());
    let backup = dir.path().join("backup.db");
    sqlite::backup(&pool, &backup).await?;
    pool.close().await;
    let restored = sqlite::connect(&format!("sqlite://{}", backup.display())).await?;
    assert!(!sqlite::needs_admin(&restored).await?);
    auth::login(&restored, "owner", "test-password").await?;
    assert_eq!(
        s::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&restored)
            .await?,
        1
    );
    restored.close().await;
    Ok(())
}
#[tokio::test]
async fn standalone_cash_stock_rollback_and_catalog() -> Result<()> {
    let (_dir, pool, actor) = fixture().await?;
    assert_eq!(db::search_products(&pool, "test", 100).await?.len(), 1);
    assert!(db::search_products(&pool, "%' OR 1=1 --", 100)
        .await?
        .is_empty());
    let sale = complete_sale(&pool, &draft("Cash", 25000.0), &actor).await?;
    assert_eq!(sale.change_amount, 5000.0);
    let mut insufficient = draft("Credit", 0.0);
    insufficient.items[0].qty = 100.0;
    insufficient.expected_total = 2000000.0;
    assert!(complete_sale(&pool, &insufficient, &actor).await.is_err());
    assert_eq!(
        s::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        9.0
    );
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        0.0
    );
    pool.close().await;
    Ok(())
}
#[tokio::test]
async fn standalone_setup_is_safe_and_file_isolation() -> Result<()> {
    let (dir, pool, _actor) = fixture().await?;
    assert!(
        sqlite::create_initial_admin(&pool, "attacker", "other-password")
            .await
            .is_err()
    );
    assert!(auth::login(&pool, "owner", "wrong-password").await.is_err());
    let separate = sqlite::connect(&format!(
        "sqlite://{}",
        dir.path().join("other.db").display()
    ))
    .await?;
    assert!(sqlite::needs_admin(&separate).await?);
    assert_eq!(
        s::query_scalar::<_, i64>("SELECT COUNT(*) FROM products")
            .fetch_one(&separate)
            .await?,
        0
    );
    separate.close().await;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_pages_load_and_service_orders_reserve() -> Result<()> {
    use pos_core::*;
    let (_dir, pool, actor) = fixture().await?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    complete_sale(&pool, &draft("Cash", 20000.0), &actor).await?;
    db::list_settings(&pool).await?;
    db::list_categories(&pool).await?;
    db::list_payment_types(&pool).await?;
    customers::list(&pool).await?;
    suppliers::list(&pool).await?;
    categories::list(&pool, &actor).await?;
    category_groups::list(&pool, &actor).await?;
    inventory::movements(&pool, 1, 100).await?;
    dashboard::load(&pool, &actor, &day, &day).await?;
    reports::load(&pool, &actor, &day, &day).await?;
    sale_summary::load(&pool, &day, &day).await?;
    activity::list(&pool, &actor, &day, &day, "", 0).await?;
    customer_credit::payments(&pool, 1).await?;
    let checks = compatibility::load(&pool, &actor).await?;
    assert!(checks.iter().all(|v| v.ok), "{:?}", checks);
    service_orders::list(&pool, "", "", 100).await?;
    let mut job = service_orders::reserve(&pool, &actor).await?;
    job.job_title = "Test repair".into();
    service_orders::save(&pool, &actor, &job, true).await?;
    let mut current = service_orders::list(&pool, "", "", 100).await?.remove(0);
    for n in 0..20 {
        let stale = current.clone();
        current.job_title = format!("Repair {n}");
        service_orders::save(&pool, &actor, &current, false).await?;
        assert!(service_orders::save(&pool, &actor, &stale, false)
            .await
            .is_err());
        current = service_orders::list(&pool, "", "", 100).await?.remove(0);
    }
    assert_eq!(service_orders::list(&pool, "", "", 100).await?.len(), 1);
    for section in [
        employees::Section::Employees,
        employees::Section::Attendance,
        employees::Section::Shifts,
        employees::Section::Assignments,
        employees::Section::Payroll,
        employees::Section::Leave,
        employees::Section::Documents,
        employees::Section::Advances,
        employees::Section::Commission,
        employees::Section::Performance,
        employees::Section::Cash,
    ] {
        employees::list(&pool, &actor, section, &day, &day).await?;
    }
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_purchase_receipt_payment_and_expense_attachments() -> Result<()> {
    use pos_core::{expense_management as m, expenses as e, purchases as p};
    use rust_decimal::Decimal;
    let (_dir, pool, actor) = fixture().await?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    s::query("INSERT INTO suppliers(id,name,status) VALUES(1,'Supplier','active')")
        .execute(&pool)
        .await?;
    let save = p::Command {
        request_id: auth::new_request_id(),
        action: p::Action::Save {
            id: None,
            revision: 0,
            draft: p::Draft {
                supplier_id: 1,
                order_date: day.clone(),
                discount: "0".into(),
                tax: "0".into(),
                notes: "Test purchase".into(),
                lines: vec![p::Line {
                    product_id: 1,
                    variant_id: None,
                    quantity: 3,
                    unit_price: "5000.25".into(),
                    location: "Shop".into(),
                    batch: "".into(),
                    expiry: "".into(),
                }],
            },
        },
    };
    let id = p::execute(&pool, &actor, &save).await?;
    assert_eq!(p::execute(&pool, &actor, &save).await?, id);
    let receive = p::Command {
        request_id: auth::new_request_id(),
        action: p::Action::Receive {
            id,
            revision: 1,
            date: day.clone(),
        },
    };
    let po = p::execute(&pool, &actor, &receive).await?;
    assert_eq!(p::execute(&pool, &actor, &receive).await?, po);
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        13.0
    );
    let pay = p::Command {
        request_id: auth::new_request_id(),
        action: p::Action::Pay(p::Payment {
            supplier_id: 1,
            po_id: Some(po),
            amount: "5000.25".into(),
            date: day.clone(),
            method: "Cash".into(),
            reference: "TEST".into(),
            notes: "".into(),
        }),
    };
    p::execute(&pool, &actor, &pay).await?;
    p::execute(&pool, &actor, &pay).await?;
    assert_eq!(s::query_scalar::<_,f64>("SELECT SUM(CASE WHEN payment_type='Purchase' THEN amount ELSE -amount END) FROM supplier_payments WHERE supplier_id=1").fetch_one(&pool).await?,10000.5);
    assert_eq!(p::order_items(&pool, &actor, po).await?.len(), 1);
    assert!(!p::load(&pool, &actor, &day, &day).await?.orders.is_empty());
    assert!(!p::ledger(&pool, &actor, 1, &day, &day).await?.is_empty());
    s::query("INSERT INTO expense_categories(name) VALUES('Rent')")
        .execute(&pool)
        .await?;
    e::save(
        &pool,
        &e::Expense {
            expense_no: "EXP-1".into(),
            category: "Rent".into(),
            amount: "1234.56".parse()?,
            expense_date: day.clone(),
            payment_method: "Cash".into(),
            ..Default::default()
        },
        &actor,
    )
    .await?;
    let expense = e::list(&pool, &actor).await?.remove(0);
    assert_eq!(expense.amount, "1234.56".parse::<Decimal>()?);
    let attachment = m::Command {
        request_id: auth::new_request_id(),
        action: m::Action::Attach {
            expense_id: expense.id,
            filename: "receipt.pdf".into(),
            content: b"%PDF-1.4 test receipt".to_vec(),
        },
    };
    let attachment_id = m::execute(&pool, &actor, &attachment).await?;
    assert_eq!(m::execute(&pool, &actor, &attachment).await?, attachment_id);
    assert_eq!(
        m::download(&pool, &actor, attachment_id).await?.1,
        b"%PDF-1.4 test receipt"
    );
    assert_eq!(m::attachments(&pool, &actor, expense.id).await?.len(), 1);
    let date = chrono::Local::now().date_naive();
    use chrono::Datelike;
    let budget = m::Command {
        request_id: auth::new_request_id(),
        action: m::Action::Budget {
            old: None,
            new: m::Budget {
                category: "Rent".into(),
                month: date.month() as i32,
                year: date.year(),
                budget_amount: Decimal::from(1000),
                notes: "".into(),
            },
        },
    };
    m::execute(&pool, &actor, &budget).await?;
    assert_eq!(m::check_alerts(&pool, &actor, true).await?, 1);
    m::check_alerts(&pool, &actor, false).await?;
    let (rows, _, alerts) = m::load(&pool, &actor, date.year(), date.month() as i32).await?;
    assert_eq!(rows[0].actual, expense.amount);
    assert!(!alerts.is_empty());
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_concurrent_checkout_rollback_readonly_and_backup() -> Result<()> {
    let (dir, pool, actor) = fixture().await?;
    let sale = draft("Credit", 0.0);
    let (a, b) = tokio::join!(
        complete_sale(&pool, &sale, &actor),
        complete_sale(&pool, &sale, &actor)
    );
    assert_eq!(a?.id, b?.id);
    assert_eq!(
        s::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&pool)
            .await?,
        1
    );
    let mut tx = pool.begin().await?;
    s::query("UPDATE products SET stock=123 WHERE id=1")
        .execute(&mut *tx)
        .await?;
    drop(tx);
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        9.0
    );
    let mut tx = pool.begin().await?;
    s::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    assert!(s::query("UPDATE customers SET current_balance=0")
        .execute(&mut *tx)
        .await
        .is_err());
    assert!(s::query("SELECT nextval('rust_invoice_number_seq')")
        .execute(&mut *tx)
        .await
        .is_err());
    assert!(s::raw_sql("DELETE FROM sales")
        .execute(&mut *tx)
        .await
        .is_err());
    tx.rollback().await?;
    assert!(
        s::query("INSERT INTO sale_items(sale_id,product_id,qty) VALUES(99999,1,1)")
            .execute(&pool)
            .await
            .is_err()
    );
    let backup = dir.path().join("online-backup.db");
    sqlite::backup(&pool, &backup).await?;
    assert!(sqlite::backup(&pool, &backup).await.is_err());
    let restored = sqlite::connect(&format!("sqlite://{}", backup.display())).await?;
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&restored)
            .await?,
        20000.0
    );
    restored.close().await;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_refuses_foreign_and_future_schema() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("foreign.db");
    {
        let foreign = rusqlite::Connection::open(&path)?;
        foreign.execute_batch(
            "CREATE TABLE unrelated(value TEXT);INSERT INTO unrelated VALUES('keep me');",
        )?;
    }
    assert!(sqlite::connect(&format!("sqlite://{}", path.display()))
        .await
        .is_err());
    {
        let foreign = rusqlite::Connection::open(&path)?;
        assert_eq!(
            foreign.query_row("SELECT value FROM unrelated", [], |r| r.get::<_, String>(0))?,
            "keep me"
        );
    }
    let url = format!("sqlite://{}", directory.path().join("future.db").display());
    let pool = sqlite::connect(&url).await?;
    s::query("UPDATE kay_schema_versions SET version=99")
        .execute(&pool)
        .await?;
    pool.close().await;
    assert!(sqlite::connect(&url).await.is_err());
    Ok(())
}

#[tokio::test]
async fn standalone_employee_json_writes_revisions_and_payroll() -> Result<()> {
    use pos_core::employees::{self as e, Command, Section};
    use serde_json::json;
    let (_dir, pool, actor) = fixture().await?;
    let mut values = e::defaults(Section::Employees);
    values["full_name"] = json!("Local Employee");
    values["user_id"] = json!(1);
    values["hire_date"] = json!("2026-01-01");
    let create = Command {
        request_id: auth::new_request_id(),
        section: Section::Employees,
        action: "save".into(),
        id: None,
        revision: String::new(),
        values,
    };
    let id = e::execute(&pool, &actor, &create).await?;
    assert_eq!(e::execute(&pool, &actor, &create).await?, id);
    let row = e::list(
        &pool,
        &actor,
        Section::Employees,
        "2020-01-01",
        "2030-12-31",
    )
    .await?
    .rows
    .remove(0);
    assert_eq!(row["employee_no"], "EMP-0001");
    let mut values = row.clone();
    values["phone"] = json!("091234567");
    let edit = Command {
        request_id: auth::new_request_id(),
        section: Section::Employees,
        action: "save".into(),
        id: Some(id),
        revision: e::text(&row, "revision"),
        values,
    };
    e::execute(&pool, &actor, &edit).await?;
    let stale = Command {
        request_id: auth::new_request_id(),
        ..edit.clone()
    };
    assert!(e::execute(&pool, &actor, &stale).await.is_err());
    let mut payroll = e::defaults(Section::Payroll);
    payroll["employee_id"] = json!(id);
    payroll["period_month"] = json!("2026-10");
    payroll["basic_salary"] = json!(1000);
    let create = Command {
        request_id: auth::new_request_id(),
        section: Section::Payroll,
        action: "save".into(),
        id: None,
        revision: String::new(),
        values: payroll,
    };
    let pid = e::execute(&pool, &actor, &create).await?;
    let row = e::list(&pool, &actor, Section::Payroll, "2020-01-01", "2030-12-31")
        .await?
        .rows
        .remove(0);
    let pay = Command {
        request_id: auth::new_request_id(),
        section: Section::Payroll,
        action: "pay".into(),
        id: Some(pid),
        revision: e::text(&row, "revision"),
        values: json!({"paid_date":"2026-10-11","payment_method":"Cash"}),
    };
    e::execute(&pool, &actor, &pay).await?;
    e::execute(&pool, &actor, &pay).await?;
    assert_eq!(
        s::query_scalar::<_, i64>("SELECT COUNT(*) FROM expenses")
            .fetch_one(&pool)
            .await?,
        1
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_held_cart_loyalty_and_location_transfer() -> Result<()> {
    use pos_core::{held_sales as h, loyalty, stock_transfer};
    let (_dir, pool, actor) = fixture().await?;
    db::save_settings(
        &pool,
        &[("loyalty_points_per_dollar".into(), "0.01".into())],
    )
    .await?;
    let save = h::Command {
        request_id: auth::new_request_id(),
        action: h::Action::Save {
            hold: h::Hold {
                id: 0,
                hold_no: String::new(),
                cart_json: r#"[{"id":1,"qty":1,"price":20000}]"#.into(),
                customer_id: Some(1),
                customer_name: String::new(),
                payment_type: "Cash".into(),
                note: "Test".into(),
                total_amount: 20000.0,
                item_count: 1,
                created_at: String::new(),
            },
            return_token: None,
        },
    };
    let held = h::execute(&pool, &actor, &save).await?;
    assert_eq!(h::execute(&pool, &actor, &save).await?, held);
    let resume = h::Command {
        request_id: auth::new_request_id(),
        action: h::Action::Resume { id: held.hold.id },
    };
    let claim = h::execute(&pool, &actor, &resume).await?;
    let mut request = draft("Cash", 20000.0);
    request.held_token = claim.token;
    let sale = complete_sale(&pool, &request, &actor).await?;
    assert!(h::list(&pool, &actor).await?.is_empty());
    assert_eq!(loyalty::history(&pool, &actor, 1).await?.0, 200);
    refund_sale(&pool, sale.id, &actor).await?;
    assert_eq!(loyalty::history(&pool, &actor, 1).await?.0, 0);
    s::query("INSERT INTO locations(name) VALUES('Warehouse')")
        .execute(&pool)
        .await?;
    stock_transfer::save(
        &pool,
        &actor,
        &stock_transfer::Transfer {
            product_id: 1,
            from: "Shop".into(),
            to: "Warehouse".into(),
            quantity: 2,
            reason: "Test".into(),
            reference: auth::new_request_id(),
            expected_available: 10.0,
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(
        pos_core::inventory::location_stock(&pool, 1, None, "Warehouse").await?,
        2.0
    );
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&pool)
            .await?,
        10.0
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_variants_expiry_and_fractional_report_allocation() -> Result<()> {
    use pos_core::{inventory, sales_comparison, stock_transfer};
    let (_dir, pool, actor) = fixture().await?;
    s::query("INSERT INTO products(id,name,sold_by,price,cost,stock) VALUES(2,'Variant product','Variants',100,50,4)").execute(&pool).await?;
    s::query("INSERT INTO product_variants(id,product_id,size,price,cost,stock,active) VALUES(1,2,'Small',100,50,4,1)").execute(&pool).await?;
    s::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES(2,1,'Shop','V1','2099-01-01',4,0)").execute(&pool).await?;
    let mut request = draft("Cash", 100.0);
    request.expected_total = 100.0;
    request.items[0].product_id = 2;
    request.items[0].variant_id = Some(1);
    request.items[0].product_name = "Variant product".into();
    request.items[0].price = 100.0;
    request.items[0].cost = 50.0;
    let sale = complete_sale(&pool, &request, &actor).await?;
    assert_eq!(
        s::query_scalar::<_, i32>("SELECT stock FROM product_variants WHERE id=1")
            .fetch_one(&pool)
            .await?,
        3
    );
    refund_sale(&pool, sale.id, &actor).await?;
    s::query("INSERT INTO locations(name) VALUES('Warehouse')")
        .execute(&pool)
        .await?;
    stock_transfer::save(
        &pool,
        &actor,
        &stock_transfer::Transfer {
            product_id: 2,
            variant_id: Some(1),
            from: "Shop".into(),
            to: "Warehouse".into(),
            quantity: 2,
            reason: "Transfer".into(),
            reference: auth::new_request_id(),
            expected_available: 4.0,
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(
        inventory::location_stock(&pool, 2, Some(1), "Warehouse").await?,
        2.0
    );
    let r = inventory::Receipt {
        supplier_id: None,
        product_id: 2,
        variant_id: Some(1),
        quantity: 1,
        unit_cost: 50.0,
        location: "Warehouse".into(),
        batch: String::new(),
        expiry: String::new(),
        reason: "Correction".into(),
        reference: auth::new_request_id(),
        received_by: "owner".into(),
        notes: String::new(),
        expected_stock: 4.0,
        expected_location_stock: Some(2.0),
    };
    inventory::change(&pool, &r, false, &actor).await?;
    assert_eq!(
        s::query_scalar::<_, i32>("SELECT stock FROM product_variants WHERE id=1")
            .fetch_one(&pool)
            .await?,
        3
    );
    // Whole-valued NUMERIC columns must not cause SQLite's integer division.
    s::query("INSERT INTO sales(id,invoice_no,total,payment,discount_amount,status,payment_type) VALUES(100,'FRACTION',29,29,1,'completed','Cash')").execute(&pool).await?;
    s::query("INSERT INTO sale_items(sale_id,product_id,product_name,qty,price,total,cost) VALUES(100,1,'Test product',1,10,10,5),(100,2,'Variant product',2,10,20,5)").execute(&pool).await?;
    s::query("UPDATE sale_items SET location=CASE WHEN product_id=1 THEN 'Shop' ELSE 'Warehouse' END WHERE sale_id=100").execute(&pool).await?;
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    let (period, _) = sales_comparison::load(&pool, &actor, &day, &day, &day, &day).await?;
    assert!((period.sales.to_string().parse::<f64>()? - 29.0).abs() < 0.0000001);
    let summary = pos_core::sale_summary::load(&pool, &day, &day).await?;
    assert!((summary.groups[0].iter().map(|v| v.discount).sum::<f64>() - 1.0).abs() < 0.0000001);
    let locations=&summary.groups[5];
    assert_eq!(locations.len(),2);
    assert_eq!(summary.group_receipts[5],1);
    let shop=locations.iter().find(|r|r.label=="Shop").unwrap();
    let warehouse=locations.iter().find(|r|r.label=="Warehouse").unwrap();
    assert_eq!((shop.receipts,warehouse.receipts),(1,1));
    assert_eq!((shop.quantity,warehouse.quantity),(1.0,2.0));
    assert!((shop.net+warehouse.net-29.0).abs()<0.0000001);
    assert!((shop.discount+warehouse.discount-1.0).abs()<0.0000001);
    assert_eq!(shop.cost+warehouse.cost,15.0);
    let shop_id:i32=s::query_scalar("SELECT id FROM locations WHERE name='Shop'").fetch_one(&pool).await?;
    let warehouse_id:i32=s::query_scalar("SELECT id FROM locations WHERE name='Warehouse'").fetch_one(&pool).await?;
    s::query("INSERT INTO rust_location_hierarchy(location_id,parent_id) VALUES($1,$2)").bind(warehouse_id).bind(shop_id).execute(&pool).await?;
    let grouped=pos_core::sale_summary::load(&pool,&day,&day).await?;
    assert_eq!(grouped.location_groups.len(),1);
    let parent=&grouped.location_groups[0];
    assert_eq!(parent.total.label,"Shop");
    assert_eq!(parent.total.receipts,1);
    assert_eq!(parent.total.quantity,3.0);
    assert!((parent.total.net-29.0).abs()<0.0000001);
    assert_eq!(parent.children.len(),2);
    assert_eq!(parent.children[1].0.label,"Shop / Warehouse");
    assert_eq!(parent.children[1].1,1);


    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_export_import_accounts_credit_stock_and_refusal() -> Result<()> {
    use pos_core::data_transfer;
    let (dir, pool, actor) = fixture().await?;
    let receipt = complete_sale(&pool, &draft("Credit", 0.0), &actor).await?;
    let exported = dir.path().join("export.db");
    let summary = data_transfer::export(&pool, &actor, &exported).await?;
    assert!(summary.tables > 100);
    assert!(summary.rows > 0);
    assert!(data_transfer::export(&pool, &actor, &exported)
        .await
        .is_err());
    let imported = dir.path().join("imported.db");
    data_transfer::import(&exported, &imported).await?;
    let destination = sqlite::connect(&format!("sqlite://{}", imported.display())).await?;
    let imported_actor = auth::login(&destination, "owner", "test-password").await?;
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&destination)
            .await?,
        20000.0
    );
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1")
            .fetch_one(&destination)
            .await?,
        9.0
    );
    assert_eq!(
        pos_core::get_receipt_detail(&destination, receipt.id)
            .await?
            .summary
            .id,
        receipt.id
    );
    pos_core::customer_credit::collect(
        &destination,
        &imported_actor,
        1,
        None,
        "5000",
        "2026-10-11",
        "Cash",
        "IMPORT",
        "",
    )
    .await?;
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&pool)
            .await?,
        20000.0
    );
    assert!(data_transfer::import(&exported, &imported).await.is_err());
    let missing = dir.path().join("missing.db");
    let failed = dir.path().join("failed.db");
    assert!(data_transfer::import(&missing, &failed).await.is_err());
    assert!(!missing.exists());
    assert!(!failed.exists());
    destination.close().await;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_transfer_rejects_corrupt_schema_foreign_keys_and_active_claims() -> Result<()> {
    use pos_core::data_transfer;
    let (dir, pool, actor) = fixture().await?;
    let export = dir.path().join("valid.db");
    data_transfer::export(&pool, &actor, &export).await?;
    {
        let c = rusqlite::Connection::open(&export)?;
        c.execute_batch("PRAGMA foreign_keys=OFF")?;
        c.execute(
            "INSERT INTO sale_items(sale_id,product_id) VALUES(9999,1)",
            [],
        )?;
    }
    let result = dir.path().join("bad-import.db");
    assert!(data_transfer::import(&export, &result).await.is_err());
    assert!(!result.exists());
    let future = dir.path().join("future-export.db");
    data_transfer::export(&pool, &actor, &future).await?;
    {
        let c = rusqlite::Connection::open(&future)?;
        c.execute("UPDATE kay_schema_versions SET version=99", [])?;
    }
    assert!(data_transfer::import(&future, &result).await.is_err());
    assert!(!result.exists());
    let corrupt = dir.path().join("corrupt.db");
    std::fs::write(&corrupt, b"not a database")?;
    assert!(data_transfer::import(&corrupt, &result).await.is_err());
    assert!(!result.exists());
    s::query("INSERT INTO rust_held_sessions(token,username,held_id,body,state) VALUES('ACTIVE','owner',1,'{}','active')").execute(&pool).await?;
    let blocked = dir.path().join("blocked.db");
    assert!(data_transfer::export(&pool, &actor, &blocked)
        .await
        .is_err());
    assert!(!blocked.exists());
    s::query("DELETE FROM rust_held_sessions")
        .execute(&pool)
        .await?;
    s::query("UPDATE users SET is_active=0 WHERE username='owner'")
        .execute(&pool)
        .await?;
    assert!(data_transfer::export(&pool, &actor, &blocked)
        .await
        .is_err());
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL on port 55487"]
async fn postgres_export_import_preserves_all_tables_blobs_credit_sequences_and_source(
) -> Result<()> {
    use ::sqlx::Row;
    use pos_core::data_transfer;
    let url = std::env::var("P1_TEST_DATABASE_URL")?;
    anyhow::ensure!(
        url.contains("127.0.0.1:55487/"),
        "Use isolated PostgreSQL only"
    );
    let (dir, local, actor) = fixture().await?;
    complete_sale(&local, &draft("Credit", 0.0), &actor).await?;
    s::query("UPDATE products SET image_data=$1,image_filename='photo.png',image_mime='image/png' WHERE id=1").bind(vec![0u8,255,17,100]).execute(&local).await?;
    let bootstrap = ::sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let schema = format!("transfer_{}", auth::new_request_id().replace("RUST-", ""));
    ::sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&bootstrap)
        .await?;
    let search_path = schema.clone();
    let pg = ::sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |conn, _| {
            let sql = format!("SET search_path TO {search_path}");
            Box::pin(async move {
                ::sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    let s::Pool::Sqlite(native) = &local else {
        unreachable!()
    };
    let metadata: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(include_str!("../../../migrations/sqlite/columns.json"))?;
    for (table, names) in metadata {
        if pos_core::warranty::is_addon_table(&table) { continue; }
        let fields = ::sqlx::query("SELECT name,type FROM pragma_table_info(?)")
            .bind(&table)
            .fetch_all(native)
            .await?;
        let mut types = vec![];
        let mut definitions = vec![];
        let mut selects = vec![];
        for name in &names {
            let row = fields
                .iter()
                .find(|row| row.get::<String, _>("name") == *name)
                .unwrap();
            let ty = row.get::<String, _>("type");
            let pg_ty = match ty.as_str() {
                "INTEGER" => "BIGINT",
                "REAL" => "DOUBLE PRECISION",
                "NUMERIC" => "NUMERIC",
                "BLOB" => "BYTEA",
                _ => {
                    if matches!(name.as_str(), "created_at" | "updated_at") {
                        "TIMESTAMP"
                    } else {
                        "TEXT"
                    }
                }
            };
            // IDs are int4 in the production schema and existing login decoder.
            let pg_ty = if ty == "INTEGER" { "INTEGER" } else { pg_ty };
            definitions.push(format!("\"{name}\" {pg_ty}"));
            types.push(pg_ty);
            selects.push(if ty == "BLOB" {
                format!("hex(\"{name}\")")
            } else {
                format!("CAST(\"{name}\" AS TEXT)")
            });
        }
        ::sqlx::query(&format!(
            "CREATE TABLE \"{table}\" ({})",
            definitions.join(",")
        ))
        .execute(&pg)
        .await?;
        let rows: Vec<String> = ::sqlx::query_scalar(&format!(
            "SELECT json_array({}) FROM \"{table}\"",
            selects.join(",")
        ))
        .fetch_all(native)
        .await?;
        let parameters = types
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                if *ty == "BYTEA" {
                    format!("decode(${}::text,'hex')", i + 1)
                } else {
                    format!("${}::text::{ty}", i + 1)
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        let insert = format!("INSERT INTO \"{table}\" VALUES({parameters})");
        for row in rows {
            let values: Vec<Option<String>> = serde_json::from_str(&row)?;
            let mut query = ::sqlx::query(&insert);
            for value in values {
                query = query.bind(value);
            }
            query.execute(&pg).await?;
        }
    }
    ::sqlx::raw_sql("CREATE SEQUENCE rust_invoice_number_seq START 57; SELECT nextval('rust_invoice_number_seq');CREATE SEQUENCE rust_batch_number_seq START 73;SELECT nextval('rust_batch_number_seq');").execute(&pg).await?;
    ::sqlx::raw_sql("ALTER TABLE sale_items ADD PRIMARY KEY(id);ALTER TABLE sales ADD PRIMARY KEY(id);ALTER TABLE customers ADD PRIMARY KEY(id);").execute(&pg).await?;
    let source = s::Pool::Postgres(pg.clone());
    let admin = auth::login(&source, "owner", "test-password").await?;
    s::query("INSERT INTO products(id,name,sold_by,price,cost,stock) VALUES(2,'ဖိတ်စာ','Service',0,0,0)").execute(&source).await?;
    s::query("INSERT INTO sale_items(id,sale_id,product_id,product_name,qty,price,total,cost) VALUES(2,1,2,'ဖိတ်စာ',1,0,0,0)").execute(&source).await?;
    let ai_request=pos_core::ai::chat("ဒီနေ့ ဖိတ်စာ (product) receipts ပြပါ","","")?;
    assert!(pos_core::assistant::candidates(&source,&admin,&ai_request).await?.is_empty());
    assert_eq!(pos_core::ai::ask(&source,&admin,&ai_request).await?.receipt_ids,vec![1]);
    let item:i32=s::query_scalar("SELECT id FROM sale_items ORDER BY id LIMIT 1").fetch_one(&source).await?;
    let warranty_unit=pos_core::warranty::register(&source,&admin,item,"PG-SOLAR-001",24).await?;
    assert_eq!(pos_core::warranty::certificate(&source,&admin,&warranty_unit).await?.serial,"PG-SOLAR-001");
    assert_eq!(pos_core::warranty::invoice_certificate(&source,&admin,&warranty_unit).await?.units.len(),2);
    let warranty_claim=pos_core::warranty::open_claim(&source,&admin,&warranty_unit,"Battery fault").await?;
    pos_core::warranty::update_claim(&source,&admin,&warranty_claim,"in_progress","Supplier inspection").await?;
    assert_eq!(pos_core::warranty::events(&source,&admin,&warranty_claim).await?.len(),2);
    s::query("INSERT INTO locations(id,name) VALUES(1000,'PG Child Location')").execute(&source).await?;
    s::query("INSERT INTO rust_location_hierarchy(location_id,parent_id) VALUES(1000,1)").execute(&source).await?;
    let pg_value=pos_core::stock_valuation::calculate(&db::search_product_metadata(&source,"",0).await?);
    assert_eq!(pg_value.total,rust_decimal::Decimal::from(45000));
    assert_eq!(pg_value.excluded_services,1);
    let exported = dir.path().join("server-export.db");
    let report = data_transfer::export(&source, &admin, &exported).await?;
    assert!(report.tables > 100);
    let imported = dir.path().join("server-imported.db");
    data_transfer::import(&exported, &imported).await?;
    let result = sqlite::connect(&format!("sqlite://{}", imported.display())).await?;
    let imported_admin = auth::login(&result, "owner", "test-password").await?;
    let copied_value=pos_core::stock_valuation::calculate(&db::search_product_metadata(&result,"",0).await?);
    assert_eq!(pg_value,copied_value);
    assert_eq!(pos_core::locations::list_names(&source).await?,pos_core::locations::list_names(&result).await?);

    assert_eq!(
        s::query_scalar::<_, Vec<u8>>("SELECT image_data FROM products WHERE id=1")
            .fetch_one(&result)
            .await?,
        [0, 255, 17, 100]
    );
    assert_eq!(
        s::query_scalar::<_, f64>("SELECT current_balance FROM customers WHERE id=1")
            .fetch_one(&result)
            .await?,
        20000.0
    );
    assert_eq!(pos_core::warranty::search(&result,&imported_admin,"PG-SOLAR-001").await?.len(),1);
    assert_eq!(pos_core::warranty::events(&result,&imported_admin,&warranty_claim).await?.len(),2);
    let new = complete_sale(&result, &draft("Cash", 20000.0), &imported_admin).await?;
    assert_eq!(new.invoice_no, "INV000058");
    assert_eq!(
        s::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales")
            .fetch_one(&source)
            .await?,
        1
    );
    ::sqlx::query("CREATE TABLE unsupported_shop_records(id int)")
        .execute(&pg)
        .await?;
    let rejected = dir.path().join("unsupported.db");
    assert!(data_transfer::export(&source, &admin, &rejected)
        .await
        .is_err());
    assert!(!rejected.exists());
    ::sqlx::query("DROP TABLE unsupported_shop_records")
        .execute(&pg)
        .await?;
    ::sqlx::query("INSERT INTO sale_items(id,sale_id) VALUES(9999,9999)")
        .execute(&pg)
        .await?;
    assert!(data_transfer::export(&source, &admin, &rejected)
        .await
        .is_err());
    assert!(!rejected.exists());
    assert_eq!(
        ::sqlx::query_scalar::<_, i64>("SELECT last_value FROM rust_invoice_number_seq")
            .fetch_one(&pg)
            .await?,
        57
    );
    assert!(!std::fs::read_dir(dir.path())?
        .filter_map(|e| e.ok())
        .any(|e| e
            .file_name()
            .to_string_lossy()
            .starts_with(".kay-pos-transfer-")));
    result.close().await;
    local.close().await;
    pg.close().await;
    ::sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&bootstrap)
        .await?;
    bootstrap.close().await;
    Ok(())
}

#[tokio::test]
async fn standalone_dashboard_hourly_sales_filter_dates_status_and_fill_quiet_hours() -> Result<()> {
    let (_dir,pool,actor)=fixture().await?;
    let empty=pos_core::dashboard::load(&pool,&actor,"2026-10-12","2026-10-12").await?;
    assert_eq!(empty.hourly.len(),24);
    assert!(empty.hourly.iter().all(|r|r.count==0 && r.amount==0.0));
    for (stamp,status) in [
        ("2026-10-12 09:00:00","completed"),
        ("2026-10-12 09:59:59","completed"),
        ("2026-10-12 18:30:00","completed"),
        ("2026-10-12 23:59:59","completed"),
        ("2026-10-13 00:00:00","completed"),
        ("2026-10-12 09:30:00","refunded"),
    ] {
        let receipt=complete_sale(&pool,&draft("Cash",20000.0),&actor).await?;
        s::query("UPDATE sales SET created_at=$1,status=$2 WHERE id=$3").bind(stamp).bind(status).bind(receipt.id).execute(&pool).await?;
    }
    let day=pos_core::dashboard::load(&pool,&actor,"2026-10-12","2026-10-12").await?;
    assert_eq!(day.hourly[9].label,"09");
    assert_eq!(day.hourly[9].count,2);
    assert_eq!(day.hourly[9].amount,40000.0);
    assert_eq!(day.hourly[18].count,1);
    assert_eq!(day.hourly[23].count,1);
    assert_eq!(day.hourly[0].count,0);
    assert_eq!(day.hourly.iter().map(|r|r.amount).sum::<f64>(),day.daily.iter().map(|r|r.amount).sum::<f64>());
    let range=pos_core::dashboard::load(&pool,&actor,"2026-10-12","2026-10-13").await?;
    assert_eq!(range.hourly[0].count,1);
    assert_eq!(range.hourly.iter().map(|r|r.count).sum::<i64>(),5);
    Ok(())
}

#[tokio::test]
async fn standalone_warranty_serial_claim_history_refund_permissions_and_export() -> Result<()> {
    use pos_core::warranty as w;
    let (dir,pool,actor)=fixture().await?;
    let receipt=complete_sale(&pool,&draft("Cash",20000.0),&actor).await?;
    let lines=w::receipt_lines(&pool,&actor,&receipt.invoice_no).await?;
    assert_eq!(lines.len(),1);
    let id=w::register(&pool,&actor,lines[0].id," inv-abc-123 ",12).await?;
    let rows=w::search(&pool,&actor,"inv-abc").await?;
    assert_eq!(rows.len(),1);assert_eq!(rows[0].id,id);assert_eq!(rows[0].serial,"INV-ABC-123");
    assert_eq!(rows[0].invoice,receipt.invoice_no);assert_eq!(rows[0].customer,"ဦးအောင်သန်း");
    let form=w::certificate(&pool,&actor,&id).await?;assert_eq!(form.serial,"INV-ABC-123");assert_eq!(form.customer,"ဦးအောင်သန်း");assert_eq!(form.invoice,receipt.invoice_no);
    assert!(w::coverage(&rows[0],chrono::Local::now().date_naive()));
    assert!(w::register(&pool,&actor,lines[0].id,"SECOND",12).await.is_err());
    let other=complete_sale(&pool,&draft("Cash",20000.0),&actor).await?;
    let other_line=w::receipt_lines(&pool,&actor,&other.invoice_no).await?[0].id;
    assert!(w::register(&pool,&actor,other_line,"inv-abc-123",12).await.is_err());
    assert!(w::register(&pool,&actor,other_line,"NEW",0).await.is_err());
    let (one,two)=tokio::join!(w::register(&pool,&actor,other_line,"CONCURRENT-A",12),w::register(&pool,&actor,other_line,"CONCURRENT-B",12));
    assert_ne!(one.is_ok(),two.is_ok());

    let claim=w::open_claim(&pool,&actor,&id,"Inverter will not start").await?;
    assert!(w::open_claim(&pool,&actor,&id,"Duplicate claim").await.is_err());
    w::update_claim(&pool,&actor,&claim,"in_progress","Sent to supplier").await?;
    w::update_claim(&pool,&actor,&claim,"resolved","Repaired and returned to customer").await?;
    assert_eq!(w::events(&pool,&actor,&claim).await?.len(),3);
    assert!(w::update_claim(&pool,&actor,&claim,"in_progress","Reopen").await.is_err());
    assert_eq!(w::claims(&pool,&actor,&id).await?[0].status,"resolved");
    assert_eq!(s::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=1").fetch_one(&pool).await?,8.0);
    let file=dir.path().join("warranty-export.db");pos_core::data_transfer::export(&pool,&actor,&file).await?;
    let dest=dir.path().join("warranty-import.db");pos_core::data_transfer::import(&file,&dest).await?;
    let imported=sqlite::connect(&format!("sqlite://{}",dest.display())).await?;
    let imported_actor=auth::login(&imported,"owner","test-password").await?;
    assert_eq!(w::search(&imported,&imported_actor,"INV-ABC-123").await?.len(),1);
    assert_eq!(w::events(&imported,&imported_actor,&claim).await?.len(),3);
    let pending_claim=w::open_claim(&pool,&actor,&id,"Second repair").await?;
    refund_sale(&pool,receipt.id,&actor).await?;
    assert!(w::update_claim(&pool,&actor,&pending_claim,"resolved","After refund").await.is_err());
    w::update_claim(&pool,&actor,&pending_claim,"rejected","Receipt refunded").await?;

    assert!(!w::coverage(&w::search(&pool,&actor,"INV-ABC").await?[0],chrono::Local::now().date_naive()));
    let rejected=w::open_claim(&pool,&actor,&id,"After refund").await?;
    assert_eq!(w::claims(&pool,&actor,&id).await?.iter().find(|c|c.id==rejected).unwrap().status,"rejected");
    assert!(w::register(&pool,&actor,lines[0].id,"AFTER-REFUND",12).await.is_err());
    s::query("UPDATE users SET role='Cashier' WHERE username='owner'").execute(&pool).await?;
    assert!(w::search(&pool,&actor,"").await.is_err());
    let cashier=auth::login(&pool,"owner","test-password").await?;
    assert!(w::search(&pool,&cashier,"INV-ABC").await?.len()==1);
    assert!(w::update_claim(&pool,&cashier,&claim,"resolved","Unauthorized").await.is_err());
    s::query("UPDATE users SET is_active=0 WHERE username='owner'").execute(&pool).await?;
    assert!(w::search(&pool,&actor,"").await.is_err());
    imported.close().await;pool.close().await;Ok(())
}

#[tokio::test]
async fn standalone_warranty_expiry_anonymous_fractional_and_legacy_upgrade() -> Result<()> {
    use pos_core::warranty as w;
    let (dir,pool,actor)=fixture().await?;
    let sale=complete_sale(&pool,&draft("Cash",20000.0),&actor).await?;
    s::query("UPDATE sales SET created_at='2020-01-31 12:00:00' WHERE id=$1").bind(sale.id).execute(&pool).await?;
    let line=w::receipt_lines(&pool,&actor,&sale.invoice_no).await?[0].id;
    let unit=w::register(&pool,&actor,line,"OLD",1).await?;
    let row=w::search(&pool,&actor,"OLD").await?.remove(0);
    assert_eq!(row.ends,"2020-02-29");
    assert!(w::coverage(&row,chrono::NaiveDate::from_ymd_opt(2020,2,29).unwrap()));
    assert!(!w::coverage(&row,chrono::NaiveDate::from_ymd_opt(2020,3,1).unwrap()));
    let claim=w::open_claim(&pool,&actor,&unit,"Expired battery").await?;
    assert_eq!(w::claims(&pool,&actor,&unit).await?[0].covered,0);
    assert!(w::update_claim(&pool,&actor,&claim,"resolved","Not valid").await.is_err());
    let mut anonymous=draft("Cash",20000.0);anonymous.customer_id=None;
    let anon=complete_sale(&pool,&anonymous,&actor).await?;
    let anonitem:i32=s::query_scalar("SELECT id FROM sale_items WHERE sale_id=$1").bind(anon.id).fetch_one(&pool).await?;
    assert!(w::register(&pool,&actor,anonitem,"ANON",12).await.is_err());
    let fraction=complete_sale(&pool,&draft("Cash",20000.0),&actor).await?;
    let fid=w::receipt_lines(&pool,&actor,&fraction.invoice_no).await?[0].id;
    s::query("UPDATE sale_items SET qty=0.5 WHERE id=$1").bind(fid).execute(&pool).await?;
    assert!(w::register(&pool,&actor,fid,"FRACTION",12).await.is_err());
    // Old standalone backups without the optional addon remain importable.
    let old=dir.path().join("old.db");pos_core::data_transfer::export(&pool,&actor,&old).await?;
    {let c=rusqlite::Connection::open(&old)?;c.execute_batch("PRAGMA foreign_keys=OFF;DROP TABLE rust_warranty_events;DROP TABLE rust_warranty_claims;DROP TABLE rust_serial_units;")?;}
    let new=dir.path().join("upgraded.db");pos_core::data_transfer::import(&old,&new).await?;
    let upgraded=sqlite::connect(&format!("sqlite://{}",new.display())).await?;
    let a=auth::login(&upgraded,"owner","test-password").await?;
    assert!(w::search(&upgraded,&a,"").await?.is_empty());
    upgraded.close().await;pool.close().await;Ok(())
}

#[tokio::test]
async fn standalone_checkout_serials_are_atomic_idempotent_and_split_across_batches() -> Result<()> {
    use pos_core::models::SerialWarranty;
    let (dir,pool,actor)=fixture().await?;
    s::query("UPDATE product_locations SET quantity=1 WHERE product_id=1").execute(&pool).await?;
    s::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES(1,'Shop','SECOND','',9)").execute(&pool).await?;
    let mut sale=draft("Credit",0.0);sale.expected_total=40000.0;sale.items[0].qty=2.0;
    sale.items[0].serials=vec![SerialWarranty {serial:" solar-001 ".into(),months:12},SerialWarranty {serial:"solar-002".into(),months:24}];
    let saved=complete_sale(&pool,&sale,&actor).await?;
    assert_eq!(complete_sale(&pool,&sale,&actor).await?.id,saved.id);
    let receipt=pos_core::get_receipt_detail(&pool,saved.id).await?;
    assert_eq!(receipt.serials.len(),2);assert_eq!(receipt.serials[0].serial,"SOLAR-001");
    assert!(receipt.serials.iter().all(|u|u.starts==saved.created_at.unwrap().date().to_string()));
    assert_eq!(s::query_scalar::<_,i64>("SELECT COUNT(DISTINCT sale_item_id) FROM rust_serial_units WHERE sale_id=$1").bind(saved.id).fetch_one(&pool).await?,2);
    assert_eq!(s::query_scalar::<_,f64>("SELECT current_balance FROM customers WHERE id=1").fetch_one(&pool).await?,40000.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=1").fetch_one(&pool).await?,8.0);
    let mut conflict=draft("Credit",0.0);conflict.items[0].serials=vec![SerialWarranty {serial:"solar-002".into(),months:12}];
    assert!(complete_sale(&pool,&conflict,&actor).await.is_err());
    assert_eq!(s::query_scalar::<_,i64>("SELECT COUNT(*) FROM sales").fetch_one(&pool).await?,1);
    assert_eq!(s::query_scalar::<_,i64>("SELECT COUNT(*) FROM rust_serial_units").fetch_one(&pool).await?,2);
    assert_eq!(s::query_scalar::<_,f64>("SELECT current_balance FROM customers WHERE id=1").fetch_one(&pool).await?,40000.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=1").fetch_one(&pool).await?,8.0);
    let mut invalid=conflict.clone();invalid.invoice_no=auth::new_request_id();invalid.customer_id=None;
    assert!(complete_sale(&pool,&invalid,&actor).await.is_err());
    invalid.customer_id=Some(1);invalid.items[0].qty=2.0;
    assert!(complete_sale(&pool,&invalid,&actor).await.is_err());
    invalid.items[0].qty=1.0;invalid.items[0].serials[0].months=0;
    assert!(complete_sale(&pool,&invalid,&actor).await.is_err());
    let mut changed=sale.clone();changed.items[0].serials[0].months=36;
    assert!(complete_sale(&pool,&changed,&actor).await.is_err());
    let exported=dir.path().join("checkout-serials.db");pos_core::data_transfer::export(&pool,&actor,&exported).await?;
    let copied=dir.path().join("copy.db");pos_core::data_transfer::import(&exported,&copied).await?;
    let reopened=sqlite::connect(&format!("sqlite://{}",copied.display())).await?;
    assert_eq!(pos_core::get_receipt_detail(&reopened,saved.id).await?.serials,receipt.serials);
    let claim=pos_core::warranty::open_claim(&pool,&actor,&pos_core::warranty::search(&pool,&actor,"SOLAR-001").await?[0].id,"Failure").await?;
    assert_eq!(pos_core::warranty::events(&pool,&actor,&claim).await?.len(),1);
    reopened.close().await;pool.close().await;Ok(())
}

#[tokio::test]
async fn standalone_invoice_warranty_groups_all_products_and_keeps_receipts_separate()->Result<()> {
    use pos_core::{warranty as w,models::SerialWarranty};
    let(_dir,pool,actor)=fixture().await?;
    let mut request=draft("Cash",40000.0);request.expected_total=40000.0;
    request.items[0].serials=vec![SerialWarranty {serial:"GROUP-A".into(),months:12}];
    let plain=draft("Cash",20000.0).items.remove(0);request.items.push(plain);
    let sale=complete_sale(&pool,&request,&actor).await?;
    let unit=w::search(&pool,&actor,"GROUP-A").await?.remove(0);
    let form=w::invoice_certificate(&pool,&actor,&unit.id).await?;
    assert_eq!(form.invoice,sale.invoice_no);assert_eq!(form.customer,"ဦးအောင်သန်း");
    assert_eq!(form.units.len(),2);assert_eq!(form.units.iter().filter(|u|u.serial.is_empty()).count(),1);
    let mut other=draft("Cash",20000.0);other.items[0].serials=vec![SerialWarranty {serial:"GROUP-B".into(),months:24}];
    complete_sale(&pool,&other,&actor).await?;
    assert!(w::invoice_certificate(&pool,&actor,&unit.id).await?.units.iter().all(|u|u.serial!="GROUP-B"));
    s::query("UPDATE users SET is_active=0 WHERE username='owner'").execute(&pool).await?;
    assert!(w::invoice_certificate(&pool,&actor,&unit.id).await.is_err());
    pool.close().await;Ok(())
}

#[tokio::test]
async fn standalone_assistant_service_product_receipts_dates_payment_amount_and_history()->Result<()> {
    use pos_core::{ai,assistant};
    let(_dir,pool,actor)=fixture().await?;
    s::query("INSERT INTO products(id,name,sold_by,price,cost,stock) VALUES(2,'ဖိတ်စာ','Service',5000,0,0)").execute(&pool).await?;
    let mut service=draft("Cash",5000.0);service.expected_total=5000.0;
    service.items[0].product_id=2;service.items[0].product_name="ဖိတ်စာ".into();service.items[0].price=5000.0;service.items[0].cost=0.0;service.items[0].is_service=true;
    let mut mixed=service.clone();mixed.invoice_no=auth::new_request_id();mixed.items.push(service.items[0].clone());mixed.items.push(draft("Cash",20000.0).items.remove(0));mixed.expected_total=30000.0;mixed.payment=30000.0;
    let first=complete_sale(&pool,&mixed,&actor).await?;
    let plain=complete_sale(&pool,&draft("Cash",20000.0),&actor).await?;
    let mut credit=service.clone();credit.invoice_no=auth::new_request_id();credit.payment_type="Credit".into();credit.payment=0.0;
    let second=complete_sale(&pool,&credit,&actor).await?;
    let mut refunded=service.clone();refunded.invoice_no=auth::new_request_id();let refunded=complete_sale(&pool,&refunded,&actor).await?;refund_sale(&pool,refunded.id,&actor).await?;
    let mut old=service.clone();old.invoice_no=auth::new_request_id();let old=complete_sale(&pool,&old,&actor).await?;
    let yesterday=(chrono::Local::now().date_naive()-chrono::Duration::days(1)).and_hms_opt(12,0,0).unwrap();
    s::query("UPDATE sales SET created_at=$1 WHERE id=$2").bind(yesterday).bind(old.id).execute(&pool).await?;
    let day=chrono::Local::now().date_naive();
    let request=ai::chat("ဒီနေ့ ဖိတ်စာ receipts ပြပါ","","")?;
    assert!(assistant::candidates(&pool,&actor,&request).await?.is_empty());
    let answer=ai::ask(&pool,&actor,&request).await?;assert_eq!(answer.receipt_ids.len(),2);assert!(answer.receipt_ids.contains(&first.id));assert!(answer.receipt_ids.contains(&second.id));assert!(!answer.receipt_ids.contains(&plain.id));
    let cash=ai::chat("ဒီနေ့ ဖိတ်စာ (product) cash receipts ပြပါ","","")?;
    assert_eq!(ai::ask(&pool,&actor,&cash).await?.receipt_ids,vec![first.id]);
    let amount=ai::chat("ဒီနေ့ ဖိတ်စာ receipts 30000 ကျပ် ပြပါ","","")?;
    assert_eq!(ai::ask(&pool,&actor,&amount).await?.receipt_ids,vec![first.id]);
    let follow=assistant::plan("မနေ့ကရော?",Some(&cash),day)?.request;
    assert_eq!(ai::ask(&pool,&actor,&follow).await?.receipt_ids,vec![old.id]);
    s::query("UPDATE products SET name='Renamed service' WHERE id=2").execute(&pool).await?;
    assert_eq!(ai::ask(&pool,&actor,&cash).await?.receipt_ids,vec![first.id]);
    pool.close().await;Ok(())
}


#[tokio::test]
async fn standalone_stock_value_variants_services_missing_cost_and_dashboard()->Result<()> {
    let (_dir,pool,actor)=fixture().await?;
    s::query("INSERT INTO products(id,name,sold_by,price,cost,stock) VALUES(2,'Missing cost','Each',100,NULL,5),(3,'Service fee','Service',100,999,100),(4,'Variant parent','Variants',100,999,999)").execute(&pool).await?;
    s::query("INSERT INTO product_variants(id,product_id,size,price,cost,stock,active) VALUES(1,4,'A',100,100,2,1),(2,4,'B',100,200,3,1),(3,4,'Inactive',100,999,5,0),(4,4,'No cost',100,0,7,1)").execute(&pool).await?;
    let products=db::search_product_metadata(&pool,"",0).await?;
    let value=pos_core::stock_valuation::calculate(&products);
    assert_eq!(value.total,rust_decimal::Decimal::from(50800));
    assert_eq!(value.missing_cost_products,2);
    assert_eq!(value.missing_cost_in_stock,2);
    assert_eq!(value.excluded_services,1);
    assert_eq!(value.rows.len(),5);
    let day=chrono::Local::now().date_naive().to_string();
    assert_eq!(pos_core::dashboard::load(&pool,&actor,&day,&day).await?.stock_value,value);
    complete_sale(&pool,&draft("Cash",20000.0),&actor).await?;
    assert_eq!(pos_core::dashboard::load(&pool,&actor,&day,&day).await?.stock_value.total,rust_decimal::Decimal::from(45800));
    Ok(())
}
#[tokio::test]
async fn standalone_quick_categories_rank_top_seven_exclude_returns_and_old_sales()->Result<()> {
    let (_dir,pool,_actor)=fixture().await?;
    for id in 10..20 {
        s::query("INSERT INTO categories(id,name) VALUES($1,$2)").bind(id).bind(format!("Category {id}")).execute(&pool).await?;
        s::query("INSERT INTO products(id,name,category_id,price,cost,stock) VALUES($1,$2,$1,100,0,0)").bind(id).bind(format!("Product {id}")).execute(&pool).await?;
        s::query("INSERT INTO sales(id,invoice_no,created_at,status,total,payment,payment_type) VALUES($1,$2,'2026-10-15 12:00:00','completed',100,100,'Cash')").bind(id).bind(format!("TEST-{id}")).execute(&pool).await?;
        s::query("INSERT INTO sale_items(sale_id,product_id,product_name,qty,refunded_qty,price,total) VALUES($1,$1,'Product',2,1,$2,$2*2)").bind(id).bind(id as f64*100.0).execute(&pool).await?;
    }
    s::query("INSERT INTO sales(id,invoice_no,created_at,status,total,payment,payment_type) VALUES(100,'OLD','2026-09-01 12:00:00','completed',999999,999999,'Cash'),(101,'REFUND','2026-10-15 12:00:00','refunded',999999,999999,'Cash'),(102,'RETURN','2026-10-15 12:00:00','completed',999999,999999,'Cash')").execute(&pool).await?;
    s::query("INSERT INTO sale_items(sale_id,product_id,product_name,qty,refunded_qty,price,total) VALUES(100,10,'Old',1,0,999999,999999),(101,10,'Refund',1,0,999999,999999),(102,10,'Returned',1,1,999999,999999)").execute(&pool).await?;
    let start=chrono::NaiveDate::from_ymd_opt(2026,9,16).unwrap();
    let end=chrono::NaiveDate::from_ymd_opt(2026,10,16).unwrap();
    let rows=db::top_sales_categories_between(&pool,start,end).await?;
    assert_eq!(rows.iter().map(|r|r.id).collect::<Vec<_>>(),vec![19,18,17,16,15,14,13]);
    assert_eq!(rows[0].amount,1900.0);
    assert!(db::top_sales_categories_between(&pool,end,end+chrono::Duration::days(1)).await?.is_empty());
    Ok(())
}
#[tokio::test]
async fn cashier_can_add_and_view_expenses_without_manager_access()->Result<()> {
    let (_dir,pool,_admin)=fixture().await?;
    s::query("INSERT INTO expense_categories(name) VALUES('Cashier expense')").execute(&pool).await?;
    s::query("UPDATE users SET role='Cashier' WHERE username='owner'").execute(&pool).await?;
    let cashier=auth::login(&pool,"owner","test-password").await?;
    assert!(cashier.allows(auth::Permission::Expenses));
    assert!(!cashier.allows(auth::Permission::Manage));
    let mut expense=pos_core::expenses::Expense {expense_no:"CASHIER-EXP-1".into(),category:"Cashier expense".into(),description:"Shop supplies".into(),amount:1500.into(),expense_date:"2026-10-18".into(),payment_method:"Cash".into(),..Default::default()};
    pos_core::expenses::save(&pool,&expense,&cashier).await?;
    let rows=pos_core::expenses::list(&pool,&cashier).await?;
    assert_eq!(rows.len(),1);assert_eq!(rows[0].amount,rust_decimal::Decimal::from(1500));
    expense=rows[0].clone();expense.amount=9000.into();
    assert!(pos_core::expenses::save(&pool,&expense,&cashier).await.is_err());
    assert!(pos_core::expenses::delete(&pool,expense.id,&cashier).await.is_err());
    assert!(pos_core::dashboard::load(&pool,&cashier,"2026-10-18","2026-10-18").await.is_err());
    assert_eq!(pos_core::expenses::list(&pool,&cashier).await?[0].amount,rust_decimal::Decimal::from(1500));
    s::query("UPDATE users SET role='Admin' WHERE username='owner'").execute(&pool).await?;
    let admin=auth::login(&pool,"owner","test-password").await?;
    pos_core::expenses::save(&pool,&expense,&admin).await?;
    pos_core::expenses::delete(&pool,expense.id,&admin).await?;
    assert!(pos_core::expenses::list(&pool,&admin).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn standalone_location_tree_checkout_refund_retry_and_isolation()->Result<()> {
    use pos_core::locations::{self,Location};
    let (dir,pool,actor)=fixture().await?;
    let shop=locations::list_names(&pool).await?.into_iter().find(|l|l.name=="Shop").unwrap();
    locations::save(&pool,&Location{id:0,name:"Shop backroom".into(),parent_id:Some(shop.id)},false).await?;
    locations::save(&pool,&Location{id:0,name:"Warehouse".into(),parent_id:None},false).await?;
    let all=locations::list_names(&pool).await?;let child=all.iter().find(|l|l.name=="Shop backroom").unwrap().clone();
    assert!(locations::save(&pool,&Location{parent_id:Some(child.id),..shop.clone()},false).await.is_err());
    assert!(locations::save(&pool,&shop,true).await.is_err());
    s::query("UPDATE products SET stock=15 WHERE id=1").execute(&pool).await?;
    s::query("UPDATE product_locations SET quantity=4 WHERE product_id=1").execute(&pool).await?;
    s::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES(1,'Shop backroom','CHILD','',6),(1,'Warehouse','WAREHOUSE','2020-01-01',5)").execute(&pool).await?;
    let mut request=draft("Cash",120000.0);request.location_id=Some(shop.id);request.items[0].qty=6.0;request.expected_total=120000.0;
    let sale=complete_sale(&pool,&request,&actor).await?;
    assert_eq!(complete_sale(&pool,&request,&actor).await?.id,sale.id);
    let wh:f64=s::query_scalar("SELECT quantity::float8 FROM product_locations WHERE location='Warehouse' AND product_id=1").fetch_one(&pool).await?;assert_eq!(wh,5.0);
    let sources:Vec<String>=s::query_scalar("SELECT DISTINCT location FROM sale_items WHERE sale_id=$1").bind(sale.id).fetch_all(&pool).await?;
    assert!(sources.iter().all(|v|v=="Shop"||v=="Shop backroom"));
    let mut too_much=request.clone();too_much.invoice_no=auth::new_request_id();too_much.items[0].qty=5.0;too_much.expected_total=100000.0;too_much.payment=100000.0;
    assert!(complete_sale(&pool,&too_much,&actor).await.is_err());
    assert_eq!(s::query_scalar::<_,f64>("SELECT stock::float8 FROM products WHERE id=1").fetch_one(&pool).await?,9.0);
    let mut changed=request.clone();changed.location_id=Some(child.id);assert!(complete_sale(&pool,&changed,&actor).await.is_err());
    refund_sale(&pool,sale.id,&actor).await?;
    assert_eq!(s::query_scalar::<_,f64>("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location IN ('Shop','Shop backroom')").fetch_one(&pool).await?,10.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT quantity::float8 FROM product_locations WHERE location='Warehouse' AND product_id=1").fetch_one(&pool).await?,5.0);
    let mut child_only=draft("Cash",120000.0);child_only.location_id=Some(child.id);child_only.items[0].qty=6.0;child_only.expected_total=120000.0;
    assert!(complete_sale(&pool,&child_only,&actor).await.is_ok());
    let mut bad=draft("Cash",20000.0);bad.location_id=Some(99999);assert!(complete_sale(&pool,&bad,&actor).await.is_err());
    let export=dir.path().join("location-tree.db");pos_core::data_transfer::export(&pool,&actor,&export).await?;
    let imported=dir.path().join("location-tree-copy.db");pos_core::data_transfer::import(&export,&imported).await?;
    let copy=sqlite::connect(&format!("sqlite://{}",imported.display())).await?;
    assert_eq!(locations::list_names(&copy).await?,locations::list_names(&pool).await?);
    copy.close().await;

    Ok(())
}

#[tokio::test]
async fn standalone_location_tree_variant_stock_and_refund()->Result<()> {
    use pos_core::locations::{self,Location};
    let (_dir,pool,actor)=fixture().await?;
    let shop=locations::list_names(&pool).await?.into_iter().find(|l|l.name=="Shop").unwrap();
    locations::save(&pool,&Location{id:0,name:"Variant room".into(),parent_id:Some(shop.id)},false).await?;
    locations::save(&pool,&Location{id:0,name:"Variant Warehouse".into(),parent_id:None},false).await?;
    s::query("INSERT INTO products(id,name,sold_by,stock,price,cost) VALUES(2,'Variant product','Variants',12,1000,500)").execute(&pool).await?;
    s::query("INSERT INTO product_variants(id,product_id,size,stock,price,cost,active) VALUES(100,2,'Large',12,1000,500,1)").execute(&pool).await?;
    s::query("INSERT INTO variant_stock_batches(product_id,variant_id,location,batch_no,expire_date,quantity,expiry_unknown) VALUES(2,100,'Shop','S','',3,1),(2,100,'Variant room','C','',4,1),(2,100,'Variant Warehouse','W','2020-01-01',5,0)").execute(&pool).await?;
    let mut request=draft("Cash",5000.0);request.location_id=Some(shop.id);request.expected_total=5000.0;
    let item=&mut request.items[0];item.product_id=2;item.variant_id=Some(100);item.product_name="Variant product".into();item.qty=5.0;item.price=1000.0;item.cost=500.0;
    let sale=complete_sale(&pool,&request,&actor).await?;
    assert_eq!(s::query_scalar::<_,i64>("SELECT quantity FROM variant_stock_batches WHERE variant_id=100 AND location='Variant Warehouse'").fetch_one(&pool).await?,5);
    let mut fail=request.clone();fail.invoice_no=auth::new_request_id();fail.items[0].qty=3.0;fail.expected_total=3000.0;fail.payment=3000.0;assert!(complete_sale(&pool,&fail,&actor).await.is_err());
    let child=locations::list_names(&pool).await?.into_iter().find(|l|l.name=="Variant room").unwrap();
    locations::save(&pool,&Location{name:"Variant room renamed".into(),..child},false).await?;
    refund_sale(&pool,sale.id,&actor).await?;
    assert_eq!(s::query_scalar::<_,i64>("SELECT SUM(quantity) FROM variant_stock_batches WHERE variant_id=100 AND location IN ('Shop','Variant room renamed')").fetch_one(&pool).await?,7);
    s::query("UPDATE product_variants SET stock=15 WHERE id=100").execute(&pool).await?;
    s::query("UPDATE products SET stock=15 WHERE id=2").execute(&pool).await?;
    let mut legacy=request.clone();legacy.invoice_no=auth::new_request_id();legacy.items[0].qty=8.0;legacy.expected_total=8000.0;legacy.payment=8000.0;assert!(complete_sale(&pool,&legacy,&actor).await.is_err());
    assert_eq!(s::query_scalar::<_,i64>("SELECT COUNT(*) FROM variant_stock_batches WHERE variant_id=100 AND location='Legacy / unassigned'").fetch_one(&pool).await?,0);
    let (all,stock)=locations::list(&pool).await?;let mut products=db::search_product_metadata(&pool,"",0).await?;
    locations::apply_sales_scope(&mut products,&stock,&locations::scope_names(&all,shop.id)?);
    assert_eq!(products.iter().find(|p|p.id==2).unwrap().variants[0].stock,7.0);
    Ok(())
}

#[tokio::test]
async fn standalone_shop2_sales_only_use_its_parent_and_descendants() -> Result<()> {
    use pos_core::locations::{self,Location};
    let (_dir,pool,actor)=fixture().await?;
    let shop=locations::list_names(&pool).await?.into_iter().find(|l|l.name=="Shop").unwrap();
    locations::save(&pool,&Location{id:0,name:"Shop child".into(),parent_id:Some(shop.id)},false).await?;
    locations::save(&pool,&Location{id:0,name:"Shop2 (သားကြီး)".into(),parent_id:None},false).await?;
    let shop2=locations::list_names(&pool).await?.into_iter().find(|l|l.name=="Shop2 (သားကြီး)").unwrap();
    locations::save(&pool,&Location{id:0,name:"Shop2 room".into(),parent_id:Some(shop2.id)},false).await?;
    let room=locations::list_names(&pool).await?.into_iter().find(|l|l.name=="Shop2 room").unwrap();
    locations::save(&pool,&Location{id:0,name:"Shop2 shelf".into(),parent_id:Some(room.id)},false).await?;
    s::query("UPDATE products SET stock=26 WHERE id=1").execute(&pool).await?;
    s::query("UPDATE product_locations SET quantity=10 WHERE product_id=1").execute(&pool).await?;
    s::query("INSERT INTO product_locations(product_id,location,batch_no,expire_date,quantity) VALUES(1,'Shop child','OTHER','2020-01-01',7),(1,'Shop2 (သားကြီး)','PARENT','',3),(1,'Shop2 room','CHILD','',4),(1,'Shop2 shelf','NESTED','',2)").execute(&pool).await?;
    let mut request=draft("Cash",100000.0);request.location_id=Some(shop2.id);request.items[0].qty=5.0;request.expected_total=100000.0;
    let first=complete_sale(&pool,&request,&actor).await?;
    assert_eq!(complete_sale(&pool,&request,&actor).await?.id,first.id);
    assert_eq!(s::query_scalar::<_,f64>("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location IN ('Shop2 (သားကြီး)','Shop2 room','Shop2 shelf')").fetch_one(&pool).await?,4.0);
    let sources:Vec<String>=s::query_scalar("SELECT DISTINCT location FROM sale_items WHERE sale_id=$1").bind(first.id).fetch_all(&pool).await?;
    assert!(sources.iter().all(|name|["Shop2 (သားကြီး)","Shop2 room","Shop2 shelf"].contains(&name.as_str())));
    assert!(sources.contains(&"Shop2 (သားကြီး)".to_string()));
    assert!(sources.iter().any(|name|name=="Shop2 room"||name=="Shop2 shelf"));
    let mut insufficient=request.clone();insufficient.invoice_no=auth::new_request_id();
    assert!(complete_sale(&pool,&insufficient,&actor).await.is_err(),"Shop2 cannot borrow the 17 units in another parent tree");
    assert_eq!(s::query_scalar::<_,f64>("SELECT stock::float8 FROM products WHERE id=1").fetch_one(&pool).await?,21.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location IN ('Shop2 (သားကြီး)','Shop2 room','Shop2 shelf')").fetch_one(&pool).await?,4.0);
    let mut remaining=draft("Cash",80000.0);remaining.location_id=Some(shop2.id);remaining.items[0].qty=4.0;remaining.expected_total=80000.0;
    let second=complete_sale(&pool,&remaining,&actor).await?;
    assert_eq!(s::query_scalar::<_,f64>("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location IN ('Shop2 (သားကြီး)','Shop2 room','Shop2 shelf')").fetch_one(&pool).await?,0.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT quantity::float8 FROM product_locations WHERE product_id=1 AND location='Shop'").fetch_one(&pool).await?,10.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT quantity::float8 FROM product_locations WHERE product_id=1 AND location='Shop child'").fetch_one(&pool).await?,7.0);
    refund_sale(&pool,first.id,&actor).await?;refund_sale(&pool,second.id,&actor).await?;
    assert_eq!(s::query_scalar::<_,f64>("SELECT stock::float8 FROM products WHERE id=1").fetch_one(&pool).await?,26.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT SUM(quantity)::float8 FROM product_locations WHERE product_id=1 AND location IN ('Shop2 (သားကြီး)','Shop2 room','Shop2 shelf')").fetch_one(&pool).await?,9.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT quantity::float8 FROM product_locations WHERE product_id=1 AND location='Shop'").fetch_one(&pool).await?,10.0);
    assert_eq!(s::query_scalar::<_,f64>("SELECT quantity::float8 FROM product_locations WHERE product_id=1 AND location='Shop child'").fetch_one(&pool).await?,7.0);
    Ok(())
}
