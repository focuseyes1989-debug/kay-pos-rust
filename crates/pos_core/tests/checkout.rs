use anyhow::Result;
use pos_core::{
    auth::{self, Session},
    complete_sale, refund_sale, SaleDraft, SaleItemDraft,
};
use sqlx::{postgres::PgPoolOptions, PgPool};

mod phase5;
mod phase6;
mod phase7;

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
        promotion: None,
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

async fn phase4_fixture()->Result<(PgPool,Session,Session)>{
    anyhow::ensure!(std::env::var("P1_TEST_DATABASE_URL")?.contains("127.0.0.1:55487/"),"Only the isolated local test cluster is allowed");
    let data=fixture().await?;
    sqlx::raw_sql(include_str!("fixtures/phase4.sql")).execute(&data.0).await?;
    for _ in 0..2{sqlx::raw_sql(include_str!("../../../migrations/007_discounts_expenses.sql")).execute(&data.0).await?;}
    Ok(data)
}

#[tokio::test]
#[ignore="requires isolated P1_TEST_DATABASE_URL"]
async fn phase4_campaigns_checkout_retry_and_stale_prices()->Result<()>{
    use pos_core::{discounts::{self,Discount},expense_management::{self as m,Action,Command}};
    let(pool,cashier,manager)=phase4_fixture().await?;
    let today=chrono::Local::now().format("%Y-%m-%d").to_string();
    let d=Discount{id:0,product_id:1,discount_percent:20.into(),discount_type:"percentage".into(),manual_price:0.into(),start_date:today.clone(),end_date:today.clone(),active:1,note:"Sale".into()};
    let cmd=Command{request_id:auth::new_request_id(),action:Action::Discount{old:None,new:d.clone()}};
    assert!(m::execute(&pool,&cashier,&cmd).await.is_err());
    let(a,b)=tokio::join!(m::execute(&pool,&manager,&cmd),m::execute(&pool,&manager,&cmd));assert_eq!(a?,b?);
    let saved=discounts::list(&pool,&manager).await?.remove(0);
    assert!(m::execute(&pool,&manager,&Command{request_id:auth::new_request_id(),action:Action::Discount{old:None,new:d.clone()}}).await.is_err());
    let mut request=draft(vec![item(1,1.0)]);request.items[0].price=80.0;request.items[0].promotion=Some(saved.clone());request.expected_total=80.0;request.payment=80.0;
    let sale=complete_sale(&pool,&request,&cashier).await?;assert_eq!(sale.total,80.0);
    let mut changed=saved.clone();changed.active=0;
    m::execute(&pool,&manager,&Command{request_id:auth::new_request_id(),action:Action::Discount{old:Some(saved.clone()),new:changed}}).await?;
    assert_eq!(complete_sale(&pool,&request,&cashier).await?.id,sale.id);
    request.invoice_no=auth::new_request_id();assert!(complete_sale(&pool,&request,&cashier).await.unwrap_err().to_string().contains("Promotion changed"));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sales").fetch_one(&pool).await?,1);
    assert!(m::execute(&pool,&manager,&Command{request_id:auth::new_request_id(),action:Action::Discount{old:Some(saved.clone()),new:saved}}).await.is_err());
    pool.close().await;Ok(())
}

#[tokio::test]
#[ignore="requires isolated P1_TEST_DATABASE_URL"]
async fn phase4_budgets_alerts_files_and_permissions()->Result<()>{
    use chrono::Datelike;
    use pos_core::expense_management::{self as m,Action,Command,Budget};
    let(pool,cashier,manager)=phase4_fixture().await?;
    let now=chrono::Local::now();let year=now.year();let month=now.month() as i32;let date=now.format("%Y-%m-%d").to_string();
    let b=Budget{category:"Rent".into(),month,year,budget_amount:100.into(),notes:String::new()};
    let cmd=Command{request_id:auth::new_request_id(),action:Action::Budget{old:None,new:b.clone()}};
    assert!(m::execute(&pool,&cashier,&cmd).await.is_err());m::execute(&pool,&manager,&cmd).await?;m::execute(&pool,&manager,&cmd).await?;
    let expense:i32=sqlx::query_scalar("INSERT INTO expenses(category,amount,expense_date) VALUES('Rent',80,$1) RETURNING id").bind(&date).fetch_one(&pool).await?;
    let(a,b)=tokio::join!(m::check_alerts(&pool,&manager,true),m::check_alerts(&pool,&manager,true));assert_eq!(a?+b?,1);
    sqlx::query("UPDATE expenses SET amount=100 WHERE id=$1").bind(expense).execute(&pool).await?;
    assert_eq!(m::check_alerts(&pool,&manager,true).await?,1);assert_eq!(m::check_alerts(&pool,&manager,true).await?,0);
    let(rows,_,alerts)=m::load(&pool,&manager,year,month).await?;assert_eq!(rows.iter().find(|r|r.category=="Rent").unwrap().actual,100.into());assert_eq!(alerts.len(),2);
    let file=Command{request_id:auth::new_request_id(),action:Action::Attach{expense_id:expense,filename:"receipt.pdf".into(),content:b"%PDF-1.4 test".to_vec()}};
    let id=m::execute(&pool,&manager,&file).await?;assert_eq!(id,m::execute(&pool,&manager,&file).await?);
    assert!(m::download(&pool,&cashier,id).await.is_err());assert_eq!(m::download(&pool,&manager,id).await?.1,b"%PDF-1.4 test");
    assert_eq!(m::attachments(&pool,&manager,expense).await?.len(),1);
    let removal=Command{request_id:auth::new_request_id(),action:Action::RemoveAttachment{i:id}};m::execute(&pool,&manager,&removal).await?;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM rust_expense_files").fetch_one(&pool).await?,0);
    let cancelled=Command{request_id:auth::new_request_id(),action:Action::ReadAlert{i:alerts[0].id}};
    assert!(!m::resolve(&pool,&manager,&cancelled).await?);assert!(m::execute(&pool,&manager,&cancelled).await.is_err());
    assert!(m::resolve(&pool,&manager,&file).await?);
    sqlx::query("UPDATE users SET is_active=0 WHERE username='supervisor'").execute(&pool).await?;assert!(m::load(&pool,&manager,year,month).await.is_err());
    pool.close().await;Ok(())
}

async fn report_fixture() -> Result<(PgPool,Session,Session)> {
    anyhow::ensure!(std::env::var("P1_TEST_DATABASE_URL")?.contains("127.0.0.1:55487/"),"Reports tests require isolated loopback database");
    let (pool,cashier,manager)=fixture().await?;
    sqlx::raw_sql("ALTER TABLE customers ADD COLUMN name text DEFAULT 'Test customer';
        CREATE TABLE expenses(id serial PRIMARY KEY,expense_no text,category text,description text,amount float8,expense_date text,payment_method text);
        CREATE TABLE suppliers(id int PRIMARY KEY,name text);
        CREATE TABLE supplier_payments(id serial PRIMARY KEY,supplier_id int,amount float8,payment_type text,payment_date text);
        INSERT INTO expenses(expense_no,category,description,amount,expense_date,payment_method) VALUES('E1','Rent','=unsafe',25,'2026-09-25','Cash'),('E2','Other','',999,'2026-09-26','Cash');
        INSERT INTO suppliers VALUES(1,'Supplier'),(2,'Advance supplier');
        INSERT INTO supplier_payments(supplier_id,amount,payment_type,payment_date) VALUES(1,500,'Purchase','2026-09-25'),(1,100,'Cash','2026-09-25'),(2,50,'Cash','2026-09-25');
        INSERT INTO sales(id,invoice_no,total,payment,change_amount,customer_id,status,payment_type,discount_amount,created_at) VALUES
          (101,'CASH',90,100,10,NULL,'completed','Cash',10,'2026-09-25 00:00:00'),
          (102,'CREDIT',200,0,0,1,'completed','Credit',0,'2026-09-25 23:59:59'),
          (103,'REFUND',50,50,0,NULL,'refunded','Cash',0,'2026-09-25 12:00:00'),
          (104,'OUTSIDE',999,999,0,NULL,'completed','Cash',0,'2026-09-26 00:00:00');
        INSERT INTO sale_items(sale_id,qty,cost,refunded_qty) VALUES(101,1,40,0),(102,2,50,0),(103,1,20,1),(104,1,10,0);
        INSERT INTO credit_sales(sale_id,customer_id,total_amount,paid_amount,balance_amount,sale_date,due_date,status) VALUES(102,1,200,20,180,'2026-09-25','2000-01-01','partial');
        INSERT INTO credit_payments(credit_sale_id,customer_id,amount,payment_date) SELECT id,1,20,'2026-09-25 12:00:00' FROM credit_sales;
        UPDATE customers SET current_balance=185 WHERE id=1;") .execute(&pool).await?;
    Ok((pool,cashier,manager))
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn phase2_reports_financial_semantics_and_permissions() -> Result<()> {
    use pos_core::reports::{self,Cell};
    let (pool,cashier,manager)=report_fixture().await?;
    assert!(reports::load(&pool,&cashier,"2026-09-25","2026-09-25").await.is_err());
    let report=reports::load(&pool,&manager,"2026-09-25","2026-09-25").await?;
    let money=|n:i64|Cell::Money(n.into());
    assert_eq!(report.tables[0].rows.len(),3);
    assert_eq!(report.tables[1].rows.len(),1);
    assert_eq!(&report.tables[2].rows.last().unwrap()[1..6],&[money(290),money(140),money(150),money(25),money(125)]);
    let metric=|name:&str|report.tables[3].rows.iter().find(|r|r[0].plain()==name).unwrap()[1].clone();
    assert_eq!(metric("Received at checkout (completed sales)"),money(90));
    assert_eq!(metric("Credit collections"),money(20));
    assert_eq!(metric("Discounts (already included in sale total)"),money(10));
    assert_eq!(metric("Refunded sales (original sale date)"),money(50));
    assert_eq!(&report.tables[4].rows[0][1..],&[money(185),money(180),money(180),money(5)]);
    assert_eq!(&report.tables[5].rows[0][1..],&[money(400),money(0)]);
    assert_eq!(&report.tables[5].rows[1][1..],&[money(0),money(50)]);
    assert!(reports::load(&pool,&manager,"2026-09-26","2026-09-25").await.is_err());
    let empty=reports::load(&pool,&manager,"1990-01-01","1990-01-01").await?;
    assert!(empty.tables[0].rows.is_empty());
    assert_eq!(empty.tables[4].rows,report.tables[4].rows);
    sqlx::query("UPDATE users SET is_active=0 WHERE username='supervisor'").execute(&pool).await?;
    assert!(reports::load(&pool,&manager,"2026-09-25","2026-09-25").await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn phase2_reports_unknown_cost_and_partial_refund_fail_closed() -> Result<()> {
    use pos_core::reports::{self,Cell};
    let (pool,_,manager)=report_fixture().await?;
    sqlx::query("UPDATE sale_items SET cost=NULL WHERE sale_id=101").execute(&pool).await?;
    let report=reports::load(&pool,&manager,"2026-09-25","2026-09-25").await?;
    assert_eq!(report.tables[2].rows.last().unwrap()[5],Cell::Unknown);
    assert!(report.warnings.iter().any(|w|w.contains("1 completed sale")));
    sqlx::query("UPDATE sale_items SET refunded_qty=1 WHERE sale_id=102").execute(&pool).await?;
    let error=reports::load(&pool,&manager,"2026-09-25","2026-09-25").await.err().unwrap();
    assert!(error.to_string().contains("partial refunds"));
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn phase1_compatibility_and_idempotent_employee_migration() -> Result<()> {
    let (pool,_,manager)=report_fixture().await?;
    assert!(pos_core::compatibility::load(&pool,&manager).await.is_err());
    let mut connection=pool.acquire().await?;
    let before=pos_core::compatibility::inspect(&mut connection).await?;
    assert!(!before.iter().find(|r|r.object=="rust_employee_requests").unwrap().ok);
    for _ in 0..2 {sqlx::raw_sql(include_str!("../../../migrations/003_employee_requests.sql")).execute(&mut *connection).await?;}
    let after=pos_core::compatibility::inspect(&mut connection).await?;
    assert!(after.iter().find(|r|r.object=="rust_employee_requests").unwrap().ok);
    assert!(after.iter().find(|r|r.object=="Request ledger safety").unwrap().ok);
    assert!(after.iter().filter(|r|r.area=="Reports").all(|r|r.ok));
    sqlx::query("ALTER TABLE sale_items DROP COLUMN cost").execute(&mut *connection).await?;
    let broken=pos_core::compatibility::inspect(&mut connection).await?;
    assert!(!broken.iter().find(|r|r.object=="sale_items").unwrap().ok);
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn dashboard_separates_credit_collections_and_current_balances() -> Result<()> {
    let (pool,cashier,manager)=fixture().await?;
    sqlx::raw_sql("ALTER TABLE customers ADD COLUMN name text DEFAULT 'Test customer';
        CREATE TABLE suppliers(id int PRIMARY KEY,name text);
        CREATE TABLE supplier_payments(id serial PRIMARY KEY,supplier_id int,amount float8,payment_type text,payment_date text,purchase_order_id int);
        CREATE TABLE expenses(amount float8,expense_date text);
        CREATE TABLE service_orders(status text);
        INSERT INTO suppliers VALUES(1,'Test supplier');
        INSERT INTO supplier_payments(supplier_id,amount,payment_type,payment_date) VALUES(1,500,'Purchase','2026-09-25'),(1,100,'Cash','2026-09-25');
        INSERT INTO expenses VALUES(50,'2026-09-25'),(999,'2026-09-26');
        INSERT INTO service_orders VALUES('received'),('delivered');")
        .execute(&pool).await?;
    let mut request=draft(vec![item(1,1.0)]);
    request.payment_type="Credit".into();request.payment=0.0;request.customer_id=Some(1);
    let sale=complete_sale(&pool,&request,&cashier).await?;
    sqlx::query("UPDATE sales SET created_at='2026-09-25 23:59:59' WHERE id=$1").bind(sale.id).execute(&pool).await?;
    sqlx::query("INSERT INTO credit_payments(credit_sale_id,customer_id,amount,payment_date) SELECT id,customer_id,20,'2026-09-25 14:00:00' FROM credit_sales WHERE sale_id=$1").bind(sale.id).execute(&pool).await?;
    sqlx::query("UPDATE credit_sales SET paid_amount=20,balance_amount=80,status='partial' WHERE sale_id=$1").bind(sale.id).execute(&pool).await?;
    sqlx::query("UPDATE customers SET current_balance=80 WHERE id=1").execute(&pool).await?;
    assert!(pos_core::dashboard::load(&pool,&cashier,"2026-09-25","2026-09-25").await.is_err());
    let d=pos_core::dashboard::load(&pool,&manager,"2026-09-25","2026-09-25").await?;
    let metric=|label:&str|d.period.iter().find(|r|r.label==label).unwrap().amount;
    assert_eq!(metric("Completed sales"),100.0);
    assert_eq!(metric("Received at checkout"),0.0);
    assert_eq!(metric("Credit sales"),100.0);
    assert_eq!(metric("Credit collections"),20.0);
    assert_eq!(metric("Recorded expenses"),50.0);
    assert_eq!(metric("Supplier payments"),100.0);
    assert_eq!(d.customers[0].amount,80.0);
    assert_eq!(d.suppliers[0].amount,400.0);
    assert_eq!(d.daily.len(),1);
    let empty=pos_core::dashboard::load(&pool,&manager,"2026-09-24","2026-09-24").await?;
    assert!(empty.daily.is_empty());
    assert_eq!(empty.customers[0].amount,80.0);
    assert!(pos_core::dashboard::load(&pool,&manager,"2026-09-26","2026-09-25").await.is_err());
    pool.close().await;
    Ok(())
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
        held_token:None,
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

async fn purchase_fixture() -> Result<(PgPool,Session,Session)> {
    anyhow::ensure!(std::env::var("P1_TEST_DATABASE_URL")?.contains("127.0.0.1:55487/"),"Purchase tests require isolated loopback database");
    let (pool,cashier,manager)=fixture().await?;
    sqlx::raw_sql(include_str!("fixtures/purchases.sql")).execute(&pool).await?;
    for _ in 0..2 {sqlx::raw_sql(include_str!("../../../migrations/006_purchases.sql")).execute(&pool).await?;}
    Ok((pool,cashier,manager))
}
fn purchase_draft()->pos_core::purchases::Draft {
    use pos_core::purchases::{Draft,Line};
    Draft{supplier_id:1,order_date:"2026-09-25".into(),discount:"10".into(),tax:"10".into(),notes:"Test only".into(),lines:vec![
        Line{product_id:1,variant_id:None,quantity:2,unit_price:"50".into(),location:"Shop".into(),batch:"PO-EACH".into(),expiry:String::new()},
        Line{product_id:3,variant_id:Some(30),quantity:3,unit_price:"20".into(),location:"Shop".into(),batch:"PO-VAR".into(),expiry:"2027-10-01".into()}
    ]}
}
fn purchase_command(action:pos_core::purchases::Action)->pos_core::purchases::Command {
    pos_core::purchases::Command{request_id:auth::new_request_id(),action}
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn phase3_purchase_receipt_and_supplier_payment_are_atomic_and_idempotent()->Result<()> {
    use pos_core::purchases::{self,Action,Payment};
    let (pool,cashier,manager)=purchase_fixture().await?;
    let save=purchase_command(Action::Save{id:None,revision:0,draft:purchase_draft()});
    assert!(purchases::execute(&pool,&cashier,&save).await.is_err());
    let id=purchases::execute(&pool,&manager,&save).await?;
    assert_eq!(id,purchases::execute(&pool,&manager,&save).await?);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM purchase_orders").fetch_one(&pool).await?,0);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM supplier_payments").fetch_one(&pool).await?,0);
    let receive=purchase_command(Action::Receive{id,revision:1,date:"2026-09-25".into()});
    let (a,b)=tokio::join!(purchases::execute(&pool,&manager,&receive),purchases::execute(&pool,&manager,&receive));
    assert_eq!(a?,id);assert_eq!(b?,id);
    assert_eq!(sqlx::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=1").fetch_one(&pool).await?,12.0);
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?,13);
    assert_eq!(sqlx::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=3").fetch_one(&pool).await?,13.0);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM purchase_orders").fetch_one(&pool).await?,1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM stock_movements").fetch_one(&pool).await?,2);
    let debit:f64=sqlx::query_scalar("SELECT SUM(amount) FROM supplier_payments").fetch_one(&pool).await?;assert_eq!(debit,165.0);
    let data=purchases::load(&pool,&manager,"2026-09-25","2026-09-25").await?;
    assert_eq!(data.drafts[0].status,"received");assert_eq!(data.orders.len(),1);
    let po=data.orders[0].id;
    assert_eq!(purchases::order_items(&pool,&manager,po).await?.len(),2);
    assert_eq!(purchases::ledger(&pool,&manager,1,"2026-09-25","2026-09-25").await?.len(),1);
    let fresh=purchase_command(Action::Receive{id,revision:1,date:"2026-09-25".into()});
    assert!(purchases::execute(&pool,&manager,&fresh).await.is_err());
    let movement:i32=sqlx::query_scalar("SELECT id FROM stock_movements WHERE product_id=1").fetch_one(&pool).await?;
    assert!(pos_core::inventory_reversal::reverse_stock_in(&pool,1,movement,"wrong receipt",&manager).await.is_err());
    let payment=purchase_command(Action::Pay(Payment{supplier_id:1,po_id:Some(po),amount:"50".into(),date:"2026-09-25".into(),method:"Cash".into(),reference:"test".into(),notes:String::new()}));
    let (a,b)=tokio::join!(purchases::execute(&pool,&manager,&payment),purchases::execute(&pool,&manager,&payment));assert_eq!(a?,b?);
    let payment_status:String=sqlx::query_scalar("SELECT payment_status FROM purchase_orders WHERE id=$1").bind(po).fetch_one(&pool).await?;assert_eq!(payment_status,"Partial");
    let mut p=if let Action::Pay(p)=payment.action.clone(){p}else{unreachable!()};p.amount="80".into();
    let a=purchase_command(Action::Pay(p.clone()));let b=purchase_command(Action::Pay(p.clone()));
    let (a,b)=tokio::join!(purchases::execute(&pool,&manager,&a),purchases::execute(&pool,&manager,&b));assert_eq!(usize::from(a.is_ok())+usize::from(b.is_ok()),1);
    p.amount="35".into();purchases::execute(&pool,&manager,&purchase_command(Action::Pay(p))).await?;
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT payment_status FROM purchase_orders WHERE id=$1").bind(po).fetch_one(&pool).await?,"Paid");
    assert_eq!(sqlx::query_scalar::<_,f64>("SELECT SUM(CASE WHEN payment_type='Purchase' THEN amount ELSE -amount END) FROM supplier_payments").fetch_one(&pool).await?,0.0);
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn phase3_purchase_rollback_conflicts_and_cancelled_request_fencing()->Result<()> {
    use pos_core::purchases::{self,Action};
    let (pool,_,manager)=purchase_fixture().await?;
    let save=purchase_command(Action::Save{id:None,revision:0,draft:purchase_draft()});
    assert!(purchases::cancel_unsaved(&pool,&manager,&save).await?.is_none());
    assert!(purchases::execute(&pool,&manager,&save).await.is_err());
    let fresh=purchase_command(save.action.clone());let id=purchases::execute(&pool,&manager,&fresh).await?;
    assert_eq!(purchases::cancel_unsaved(&pool,&manager,&fresh).await?,Some(id));
    let mut changed=fresh.clone();if let Action::Save{draft,..}=&mut changed.action{draft.notes="changed".into();}
    assert!(purchases::execute(&pool,&manager,&changed).await.is_err());
    let edit=purchase_command(Action::Save{id:Some(id),revision:1,draft:purchase_draft()});purchases::execute(&pool,&manager,&edit).await?;
    assert!(purchases::execute(&pool,&manager,&purchase_command(edit.action.clone())).await.is_err());
    sqlx::raw_sql("CREATE FUNCTION fail_purchase_ledger() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected ledger failure'; END $$; CREATE TRIGGER fail_purchase BEFORE INSERT ON supplier_payments FOR EACH ROW EXECUTE FUNCTION fail_purchase_ledger();").execute(&pool).await?;
    let receive=purchase_command(Action::Receive{id,revision:2,date:"2026-09-25".into()});
    assert!(purchases::execute(&pool,&manager,&receive).await.is_err());
    assert_eq!(sqlx::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=1").fetch_one(&pool).await?,10.0);
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?,10);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM purchase_orders").fetch_one(&pool).await?,0);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM stock_movements").fetch_one(&pool).await?,0);
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM rust_purchase_drafts WHERE id=$1").bind(id).fetch_one(&pool).await?,"pending");
    sqlx::query("DROP TRIGGER fail_purchase ON supplier_payments").execute(&pool).await?;
    purchases::execute(&pool,&manager,&receive).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn phase3_validation_cancellation_legacy_ledger_and_advances()->Result<()> {
    use pos_core::purchases::{self,Action,Payment};
    let (pool,cashier,manager)=purchase_fixture().await?;
    let mut invalid=purchase_draft();invalid.lines[0].variant_id=Some(30);
    assert!(purchases::execute(&pool,&manager,&purchase_command(Action::Save{id:None,revision:0,draft:invalid})).await.is_err());
    let id=purchases::execute(&pool,&manager,&purchase_command(Action::Save{id:None,revision:0,draft:purchase_draft()})).await?;
    purchases::execute(&pool,&manager,&purchase_command(Action::Cancel{id,revision:1,reason:"Test cancellation".into()})).await?;
    assert!(purchases::execute(&pool,&manager,&purchase_command(Action::Receive{id,revision:2,date:"2026-09-25".into()})).await.is_err());
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM purchase_orders").fetch_one(&pool).await?,0);
    let advance=Payment{supplier_id:1,po_id:None,amount:"25.25".into(),date:"2026-09-25".into(),method:"Bank Transfer".into(),reference:"Advance".into(),notes:String::new()};
    assert!(purchases::execute(&pool,&cashier,&purchase_command(Action::Pay(advance.clone()))).await.is_err());
    purchases::execute(&pool,&manager,&purchase_command(Action::Pay(advance))).await?;
    let data=purchases::load(&pool,&manager,"2026-09-25","2026-09-25").await?;
    assert_eq!(data.suppliers.iter().find(|s|s.id==1).unwrap().balance,"-25.25");
    let po:i32=sqlx::query_scalar("INSERT INTO purchase_orders(po_no,supplier_id,order_date,total_amount,status) VALUES('LEGACY',1,'2026-09-25',100,'completed') RETURNING id").fetch_one(&pool).await?;
    let p=Payment{supplier_id:1,po_id:Some(po),amount:"20".into(),date:"2026-09-25".into(),method:"Cash".into(),reference:String::new(),notes:String::new()};
    assert!(purchases::execute(&pool,&manager,&purchase_command(Action::Pay(p.clone()))).await.is_err());
    sqlx::query("INSERT INTO supplier_payments(supplier_id,amount,payment_date,payment_type,purchase_order_id) VALUES(1,100,'2026-09-25','Purchase',$1)").bind(po).execute(&pool).await?;
    let mut wrong=p.clone();wrong.supplier_id=2;
    assert!(purchases::execute(&pool,&manager,&purchase_command(Action::Pay(wrong))).await.is_err());
    sqlx::query("UPDATE purchase_orders SET payment_status='Paid' WHERE id=$1").bind(po).execute(&pool).await?;
    assert!(purchases::execute(&pool,&manager,&purchase_command(Action::Pay(p.clone()))).await.unwrap_err().to_string().contains("Payment status"));
    sqlx::query("UPDATE purchase_orders SET payment_status='Unpaid' WHERE id=$1").bind(po).execute(&pool).await?;
    purchases::execute(&pool,&manager,&purchase_command(Action::Pay(p.clone()))).await?;
    sqlx::query("UPDATE users SET is_active=0 WHERE username='supervisor'").execute(&pool).await?;
    assert!(purchases::execute(&pool,&manager,&purchase_command(Action::Pay(p))).await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn stock_in_reversal_is_atomic_authorized_and_only_applied_once() -> Result<()> {
    let (pool,cashier,manager)=fixture().await?;
    let id:i32=sqlx::query_scalar("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,location,notes) VALUES(3,30,'stock_in',3,7,10,'Shop','Counter error') RETURNING id").fetch_one(&pool).await?;
    sqlx::query("INSERT INTO variant_batch_changes VALUES($1,3,30,'Shop','V1','2027-01-01',3,0)").bind(id).execute(&pool).await?;
    assert!(pos_core::inventory_reversal::reverse_stock_in(&pool,3,id,"Wrong count",&cashier).await.is_err());
    let (a,b)=tokio::join!(pos_core::inventory_reversal::reverse_stock_in(&pool,3,id,"Wrong count",&manager),pos_core::inventory_reversal::reverse_stock_in(&pool,3,id,"Wrong count",&manager));
    assert_ne!(a.is_ok(),b.is_ok());
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?,7);
    assert_eq!(sqlx::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=3").fetch_one(&pool).await?,7.0);
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT quantity FROM variant_stock_batches WHERE batch_no='V2'").fetch_one(&pool).await?,7);
    let failed:i32=sqlx::query_scalar("INSERT INTO stock_movements(product_id,variant_id,type,quantity,old_stock,new_stock,location,notes) VALUES(3,30,'stock_in',8,0,8,'Store','') RETURNING id").fetch_one(&pool).await?;
    sqlx::query("INSERT INTO variant_batch_changes VALUES($1,3,30,'Store','V2','2028-01-01',8,0)").bind(failed).execute(&pool).await?;
    assert!(pos_core::inventory_reversal::reverse_stock_in(&pool,3,failed,"Wrong count",&manager).await.is_err());
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT quantity FROM variant_stock_batches WHERE batch_no='V2'").fetch_one(&pool).await?,7);
    assert!(!sqlx::query_scalar::<_,String>("SELECT notes FROM stock_movements WHERE id=$1").bind(failed).fetch_one(&pool).await?.contains("[REVERSED]"));
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn unassigned_variant_can_be_adjusted_to_zero_without_touching_other_locations() -> Result<()> {
    let (pool, _, manager) = fixture().await?;
    sqlx::query("DELETE FROM variant_stock_batches WHERE variant_id=30").execute(&pool).await?;
    assert_eq!(pos_core::inventory::location_stock(&pool,3,Some(30),"Legacy / unassigned").await?,10.0);
    let mut request = pos_core::inventory::Receipt {
        supplier_id:None,product_id:3,variant_id:Some(30),quantity:0,unit_cost:0.0,
        location:"Default".into(),batch:String::new(),expiry:String::new(),reason:"Entry correction".into(),
        reference:String::new(),received_by:String::new(),notes:String::new(),expected_stock:10.0,expected_location_stock:Some(0.0),
    };
    let error=pos_core::inventory::change(&pool,&request,true,&manager).await.unwrap_err();
    assert!(error.to_string().contains("No change"));
    request.location="Legacy / unassigned".into();
    request.expected_location_stock=Some(10.0);
    pos_core::inventory::change(&pool,&request,true,&manager).await?;
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?,0);
    assert_eq!(sqlx::query_scalar::<_,f64>("SELECT stock FROM products WHERE id=3").fetch_one(&pool).await?,0.0);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT SUM(quantity) FROM variant_stock_batches WHERE variant_id=30").fetch_one(&pool).await?,0);
    assert_eq!(sqlx::query_scalar::<_,i32>("SELECT delta FROM variant_batch_changes WHERE variant_id=30").fetch_one(&pool).await?,-10);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn adjustment_is_location_scoped_and_uses_authenticated_actor() -> Result<()> {
    let (pool, cashier, manager) = fixture().await?;
    let mut request = pos_core::inventory::Receipt {
        supplier_id: None, product_id: 1, variant_id: None, quantity: 2,
        unit_cost: 0.0, location: "Shop".into(), batch: String::new(), expiry: String::new(),
        reason: "Physical count".into(), reference: "COUNT".into(), received_by: "forged".into(),
        notes: "Two on shelf".into(), expected_stock: 10.0, expected_location_stock: Some(4.0),
    };
    assert!(pos_core::inventory::change(&pool, &request, true, &cashier).await.is_err());
    pos_core::inventory::change(&pool, &request, true, &manager).await?;
    assert_eq!(pos_core::inventory::location_stock(&pool, 1, None, "Shop").await?, 2.0);
    assert_eq!(pos_core::inventory::location_stock(&pool, 1, None, "Store").await?, 6.0);
    assert_eq!(sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=1").fetch_one(&pool).await?, 8.0);
    let audit: (String, String) = sqlx::query_as("SELECT created_by,notes FROM stock_movements WHERE product_id=1").fetch_one(&pool).await?;
    assert_eq!(audit, ("supervisor".into(), "Two on shelf".into()));
    assert!(pos_core::inventory::change(&pool, &request, true, &manager).await.is_err());
    request.expected_stock = 8.0;
    request.expected_location_stock = Some(2.0);
    request.quantity = 0;
    pos_core::inventory::change(&pool, &request, true, &manager).await?;
    assert_eq!(pos_core::inventory::location_stock(&pool, 1, None, "Shop").await?, 0.0);
    assert_eq!(pos_core::inventory::location_stock(&pool, 1, None, "Store").await?, 6.0);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn stale_variant_parent_is_repaired_atomically_and_retry_is_idempotent() -> Result<()> {
    let (pool, cashier, _) = fixture().await?;
    for stale in [0.0, 99.0] {
        sqlx::query("UPDATE products SET stock=$1 WHERE id=3").bind(stale).execute(&pool).await?;
        let request = draft(vec![item(3, 1.0)]);
        let sale = complete_sale(&pool, &request, &cashier).await?;
        assert_eq!(complete_sale(&pool, &request, &cashier).await?.id, sale.id);
        let master: f64 = sqlx::query_scalar("SELECT stock FROM products WHERE id=3").fetch_one(&pool).await?;
        let variant: i32 = sqlx::query_scalar("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?;
        let batches: i64 = sqlx::query_scalar("SELECT SUM(quantity) FROM variant_stock_batches WHERE variant_id=30").fetch_one(&pool).await?;
        assert_eq!(master, variant as f64);
        assert_eq!(master, batches as f64);
    }
    assert_eq!(sqlx::query_scalar::<_, i32>("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?, 8);
    sqlx::query("UPDATE products SET stock=99 WHERE id=3").execute(&pool).await?;
    // Parent reconciliation must not bypass insufficient stock or invent batches.
    assert!(complete_sale(&pool, &draft(vec![item(3, 9.0)]), &cashier).await.is_err());
    sqlx::query("UPDATE variant_stock_batches SET quantity=99 WHERE variant_id=30").execute(&pool).await?;
    assert!(complete_sale(&pool, &draft(vec![item(3, 1.0)]), &cashier).await.is_err());
    assert_eq!(sqlx::query_scalar::<_, f64>("SELECT stock FROM products WHERE id=3").fetch_one(&pool).await?, 99.0);
    assert_eq!(sqlx::query_scalar::<_, i32>("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?, 8);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales").fetch_one(&pool).await?, 2);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn legacy_variant_opening_preserves_batches_and_rolls_back_on_failure() -> Result<()> {
    let (pool, cashier, _) = fixture().await?;
    sqlx::query("DELETE FROM variant_stock_batches WHERE batch_no='V2'").execute(&pool).await?;
    // A later invalid cart line must roll back both the opening and the sale.
    assert!(complete_sale(&pool, &draft(vec![item(3, 5.0), item(1, 99.0)]), &cashier).await.is_err());
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM variant_stock_batches WHERE batch_no='LEGACY-OPENING'").fetch_one(&pool).await?, 0);
    let request = draft(vec![item(3, 5.0)]);
    let sale = complete_sale(&pool, &request, &cashier).await?;
    assert_eq!(complete_sale(&pool, &request, &cashier).await?.id, sale.id);
    let opening: (i32, i32, String) = sqlx::query_as("SELECT quantity,expiry_unknown,expire_date FROM variant_stock_batches WHERE variant_id=30 AND batch_no='LEGACY-OPENING'").fetch_one(&pool).await?;
    assert_eq!(opening, (5, 1, String::new()));
    let dated: (i32, String) = sqlx::query_as("SELECT quantity,expire_date FROM variant_stock_batches WHERE batch_no='V1'").fetch_one(&pool).await?;
    assert_eq!(dated, (0, "2027-01-01".into()));
    assert_eq!(sqlx::query_scalar::<_, i32>("SELECT stock FROM product_variants WHERE id=30").fetch_one(&pool).await?, 5);
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sales").fetch_one(&pool).await?, 1);
    pool.close().await;
    Ok(())
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
    let (pool, cashier, manager) = fixture().await?;
    let request = draft(vec![item(3, 1.0)]);
    let adjustment = pos_core::inventory::Receipt {
        supplier_id: None,
        product_id: 3,
        variant_id: Some(30),
        quantity: 4,
        unit_cost: 50.0,
        location: "Shop".into(),
        batch: String::new(),
        expiry: String::new(),
        reason: "Test adjustment".into(),
        reference: "test".into(),
        received_by: "supervisor".into(),
        notes: String::new(),
        expected_stock: 10.0,
        expected_location_stock: Some(3.0),
    };
    let (sale, stock) = tokio::join!(
        complete_sale(&pool, &request, &cashier),
        pos_core::inventory::change(&pool, &adjustment, true, &manager)
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
