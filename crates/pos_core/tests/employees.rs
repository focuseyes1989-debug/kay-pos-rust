use anyhow::Result;
use pos_core::{
    auth::{self, Session},
    employees::{self, Command, Section},
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
async fn fixture() -> Result<(PgPool, Session, Session, String)> {
    let url = std::env::var("EMPLOYEE_TEST_DATABASE_URL")?;
    ensure_local(&url)?;
    let bootstrap = PgPoolOptions::new().connect(&url).await?;
    let schema = format!("emp_{}", auth::new_request_id().replace("RUST-", ""));
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&bootstrap)
        .await?;
    bootstrap.close().await;
    let namespace = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .after_connect(move |conn, _| {
            let q = format!("SET search_path TO {namespace}");
            Box::pin(async move {
                sqlx::query(&q).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    sqlx::raw_sql(include_str!("fixtures/checkout.sql"))
        .execute(&pool)
        .await?;
    sqlx::raw_sql(include_str!("fixtures/employees.sql"))
        .execute(&pool)
        .await?;
    sqlx::raw_sql(include_str!(
        "../../../migrations/003_employee_requests.sql"
    ))
    .execute(&pool)
    .await?;
    let mut hash = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(b"test", b"salt", 100000, &mut hash);
    for (name, role) in [("admin", "admin"), ("manager", "manager")] {
        sqlx::query("INSERT INTO users(username,role,password_hash,salt) VALUES($1,$2,$3,$4)")
            .bind(name)
            .bind(role)
            .bind(hex::encode(hash))
            .bind(hex::encode(b"salt"))
            .execute(&pool)
            .await?;
    }
    let admin = auth::login(&pool, "admin", "test").await?;
    let manager = auth::login(&pool, "manager", "test").await?;
    Ok((pool, admin, manager, schema))
}
fn ensure_local(url: &str) -> Result<()> {
    anyhow::ensure!(
        url.contains("127.0.0.1:55487/"),
        "Tests require isolated loopback database on port 55487"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "Requires isolated EMPLOYEE_TEST_DATABASE_URL"]
async fn attendance_sync_is_atomic_idempotent_and_preserves_manual_corrections() -> Result<()> {
    use pos_core::{
        attendance_sync as sync,
        zkteco::{self, Device},
    };
    let (pool, admin, manager, schema) = fixture().await?;
    sqlx::raw_sql(include_str!("../../../migrations/004_zkteco_devices.sql"))
        .execute(&pool)
        .await?;
    sqlx::raw_sql(include_str!(
        "../../../migrations/005_zkteco_attendance.sql"
    ))
    .execute(&pool)
    .await?;
    let mut employee = employees::defaults(Section::Employees);
    employee["full_name"] = json!("Sync test");
    let eid = employees::execute(&pool, &admin, &command(Section::Employees, employee)).await?;
    let device = zkteco::save(
        &pool,
        &admin,
        &Device {
            ip_address: "127.0.0.1".into(),
            ..Device::default()
        },
        None,
    )
    .await?;
    assert!(sync::prepare(&pool, &admin, device.id).await.is_err());
    sqlx::query("INSERT INTO zkteco_employee_mappings(device_id,employee_id,device_user_id) VALUES($1,$2,'42')").bind(device.id).bind(eid).execute(&pool).await?;
    assert!(sync::prepare(&pool, &manager, device.id).await.is_err());
    let sid:i32=sqlx::query_scalar("INSERT INTO shifts(name,start_time,end_time) VALUES('Sync shift','08:00','17:00') RETURNING id").fetch_one(&pool).await?;
    sqlx::query("INSERT INTO employee_shifts(employee_id,shift_id,effective_from) VALUES($1,$2,'2026-01-01')").bind(eid).bind(sid).execute(&pool).await?;
    sqlx::query("INSERT INTO attendance(employee_id,attendance_date,check_in,check_out,status,correction_reason) VALUES($1,'2026-09-22','08:00','18:00','Present','Manager correction')").bind(eid).execute(&pool).await?;
    let time = |s: &str| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap();
    let snapshot = || sync::Snapshot {
        serial: "TEST-SYNC".into(),
        device_time: time("2026-09-22 18:00:00"),
        users: ["42".into(), "77".into()].into_iter().collect(),
        punches: vec![
            sync::Punch {
                user_id: "42".into(),
                time: time("2026-09-21 09:00:00"),
                status: 1,
                punch: 0,
            },
            sync::Punch {
                user_id: "42".into(),
                time: time("2026-09-21 17:00:00"),
                status: 1,
                punch: 1,
            },
            sync::Punch {
                user_id: "42".into(),
                time: time("2026-09-21 09:00:00"),
                status: 1,
                punch: 0,
            },
            sync::Punch {
                user_id: "42".into(),
                time: time("2026-09-22 09:00:00"),
                status: 1,
                punch: 0,
            },
            sync::Punch {
                user_id: "42".into(),
                time: time("2026-09-22 17:00:00"),
                status: 1,
                punch: 1,
            },
            sync::Punch {
                user_id: "42".into(),
                time: time("2026-10-01 09:00:00"),
                status: 1,
                punch: 0,
            },
            sync::Punch {
                user_id: "77".into(),
                time: time("2026-09-21 09:00:00"),
                status: 1,
                punch: 0,
            },
        ],
    };
    let plan = sync::prepare(&pool, &admin, device.id).await?;
    let first = sync::import_snapshot(&pool, &admin, &plan, snapshot()).await?;
    assert_eq!(
        (
            first.inserted,
            first.duplicates,
            first.invalid,
            first.unmapped,
            first.days,
            first.preserved
        ),
        (5, 1, 1, 1, 1, 1)
    );
    let (a, b) = tokio::join!(
        sync::import_snapshot(&pool, &admin, &plan, snapshot()),
        sync::import_snapshot(&pool, &admin, &plan, snapshot())
    );
    assert_eq!(a?.inserted, 0);
    assert_eq!(b?.inserted, 0);
    let row:(String,i32,String,String)=sqlx::query_as("SELECT status,late_minutes,check_in,check_out FROM attendance WHERE employee_id=$1 AND attendance_date='2026-09-21'").bind(eid).fetch_one(&pool).await?;
    assert_eq!(row, ("Late".into(), 60, "09:00".into(), "17:00".into()));
    let manual: String = sqlx::query_scalar(
        "SELECT check_in FROM attendance WHERE employee_id=$1 AND attendance_date='2026-09-22'",
    )
    .bind(eid)
    .fetch_one(&pool)
    .await?;
    assert_eq!(manual, "08:00");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attendance")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 2);
    sqlx::query("UPDATE zkteco_employee_mappings SET device_user_id='77' WHERE device_id=$1")
        .bind(device.id)
        .execute(&pool)
        .await?;
    assert!(sync::import_snapshot(&pool, &admin, &plan, snapshot())
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM zkteco_attendance_logs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 5);
    sqlx::query("UPDATE zkteco_employee_mappings SET device_user_id='42' WHERE device_id=$1")
        .bind(device.id)
        .execute(&pool)
        .await?;
    sqlx::query(
        "UPDATE zkteco_attendance_logs SET employee_id=NULL WHERE punch_time='2026-09-21 09:00:00'",
    )
    .execute(&pool)
    .await?;
    let mut conflicting = snapshot();
    conflicting.punches.insert(
        0,
        sync::Punch {
            user_id: "42".into(),
            time: time("2026-09-20 08:00:00"),
            status: 1,
            punch: 0,
        },
    );
    assert!(sync::import_snapshot(&pool, &admin, &plan, conflicting)
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM zkteco_attendance_logs")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        count, 5,
        "A failed import must roll back earlier inserted punches"
    );
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "Requires isolated EMPLOYEE_TEST_DATABASE_URL"]
async fn zkteco_configuration_permissions_retries_and_tcp_probe() -> Result<()> {
    use pos_core::zkteco::{self, Device};
    let (pool, admin, manager, schema) = fixture().await?;
    sqlx::raw_sql(include_str!("../../../migrations/004_zkteco_devices.sql"))
        .execute(&pool)
        .await?;
    let value = Device {
        ip_address: "127.0.0.1".into(),
        ..Device::default()
    };
    assert!(zkteco::list(&pool, &manager).await.is_err());
    assert!(zkteco::save(&pool, &manager, &value, None).await.is_err());
    let saved = zkteco::save(&pool, &admin, &value, None).await?;
    let retried = zkteco::save(&pool, &admin, &value, None).await?;
    assert_eq!(saved.id, retried.id);
    let mut duplicate = value.clone();
    duplicate.name = "Different".into();
    assert!(zkteco::save(&pool, &admin, &duplicate, None).await.is_err());
    let mut updated = saved.clone();
    updated.name = "K20".into();
    updated.is_active = 0;
    let updated = zkteco::save(&pool, &admin, &updated, Some(&saved)).await?;
    assert_eq!(updated.name, "K20");
    assert!(zkteco::save(&pool, &admin, &saved, Some(&saved))
        .await
        .is_err());
    assert_eq!(zkteco::list(&pool, &admin).await?.len(), 1);
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let probe = Device {
        port: i32::from(listener.local_addr()?.port()),
        ..value
    };
    assert!(zkteco::test_tcp(&pool, &manager, &probe).await.is_err());
    zkteco::test_tcp(&pool, &admin, &probe).await?;
    drop(listener);
    assert!(zkteco::test_tcp(&pool, &admin, &probe).await.is_err());
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
fn command(section: Section, values: Value) -> Command {
    Command {
        request_id: auth::new_request_id(),
        section,
        action: "save".into(),
        id: None,
        revision: String::new(),
        values,
    }
}
async fn record(pool: &PgPool, actor: &Session, section: Section, id: i32) -> Result<Value> {
    let page = employees::list(pool, actor, section, "2020-01-01", "2030-12-31").await?;
    Ok(page.rows.into_iter().find(|v| v["id"] == id).unwrap())
}
fn action(section: Section, row: &Value, action: &str, values: Value) -> Command {
    Command {
        request_id: auth::new_request_id(),
        section,
        action: action.into(),
        id: row["id"].as_i64().map(|n| n as i32),
        revision: employees::text(row, "revision"),
        values,
    }
}

#[tokio::test]
#[ignore = "requires isolated EMPLOYEE_TEST_DATABASE_URL"]
async fn complete_employee_workflows_and_atomic_financial_retries() -> Result<()> {
    let (pool, admin, manager, schema) = fixture().await?;
    let mut v = employees::defaults(Section::Employees);
    v["full_name"] = json!("Test Employee");
    v["user_id"] = json!(1);
    v["hire_date"] = json!("2026-01-01");
    let cmd = command(Section::Employees, v);
    let id = employees::execute(&pool, &admin, &cmd).await?;
    assert_eq!(employees::execute(&pool, &admin, &cmd).await?, id);
    let row = record(&pool, &admin, Section::Employees, id).await?;
    assert_eq!(row["employee_no"], "EMP-0001");
    let mut edited = row.clone();
    edited["phone"] = json!("123");
    let edit = action(Section::Employees, &row, "save", edited);
    employees::execute(&pool, &manager, &edit).await?;
    let stale = action(Section::Employees, &row, "save", row.clone());
    assert!(employees::execute(&pool, &admin, &stale).await.is_err());
    let mut shift = employees::defaults(Section::Shifts);
    shift["name"] = json!("Morning");
    let sid = employees::execute(&pool, &admin, &command(Section::Shifts, shift)).await?;
    let mut assign = employees::defaults(Section::Assignments);
    assign["employee_id"] = json!(id);
    assign["shift_id"] = json!(sid);
    assign["effective_from"] = json!("2026-01-01");
    let aid = employees::execute(&pool, &admin, &command(Section::Assignments, assign)).await?;
    let mut attendance = employees::defaults(Section::Attendance);
    attendance["employee_id"] = json!(id);
    attendance["attendance_date"] = json!("2026-09-22");
    attendance["check_in"] = json!("08:30");
    attendance["check_out"] = json!("17:00");
    attendance["correction_reason"] = json!("Manual correction");
    let atid = employees::execute(&pool, &admin, &command(Section::Attendance, attendance)).await?;
    assert_eq!(
        record(&pool, &admin, Section::Attendance, atid).await?["late_minutes"],
        30
    );
    let mut leave = employees::defaults(Section::Leave);
    leave["employee_id"] = json!(id);
    leave["start_date"] = json!("2026-09-22");
    leave["end_date"] = json!("2026-09-22");
    let lid = employees::execute(&pool, &admin, &command(Section::Leave, leave)).await?;
    let l = record(&pool, &admin, Section::Leave, lid).await?;
    employees::execute(
        &pool,
        &manager,
        &action(Section::Leave, &l, "review", json!({"status":"Approved"})),
    )
    .await?;
    assert_eq!(
        record(&pool, &admin, Section::Attendance, atid).await?["correction_reason"],
        "Manual correction"
    );
    let mut payroll = employees::defaults(Section::Payroll);
    payroll["employee_id"] = json!(id);
    payroll["period_month"] = json!("2026-09");
    payroll["basic_salary"] = json!(1000);
    let pay = command(Section::Payroll, payroll);
    assert!(employees::execute(&pool, &manager, &pay).await.is_err());
    let pid = employees::execute(&pool, &admin, &pay).await?;
    let p = record(&pool, &admin, Section::Payroll, pid).await?;
    let pay = action(
        Section::Payroll,
        &p,
        "pay",
        json!({"paid_date":"2026-09-22","payment_method":"Cash"}),
    );
    let (a, b) = tokio::join!(
        employees::execute(&pool, &admin, &pay),
        employees::execute(&pool, &admin, &pay)
    );
    assert_eq!(a?, b?);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM expenses")
            .fetch_one(&pool)
            .await?,
        1
    );
    let mut advance = employees::defaults(Section::Advances);
    advance["employee_id"] = json!(id);
    advance["advance_date"] = json!("2026-09-22");
    advance["amount"] = json!(100);
    let adid = employees::execute(&pool, &admin, &command(Section::Advances, advance)).await?;
    let a = record(&pool, &admin, Section::Advances, adid).await?;
    assert!(employees::execute(
        &pool,
        &admin,
        &action(Section::Advances, &a, "repay", json!({"amount":101}))
    )
    .await
    .is_err());
    let repay = action(Section::Advances, &a, "repay", json!({"amount":40}));
    employees::execute(&pool, &admin, &repay).await?;
    employees::execute(&pool, &admin, &repay).await?;
    assert_eq!(
        record(&pool, &admin, Section::Advances, adid).await?["balance"],
        60.0
    );
    let mut rule = employees::defaults(Section::Commission);
    rule["employee_id"] = json!(id);
    rule["rate_percent"] = json!(10);
    employees::execute(&pool, &admin, &command(Section::Commission, rule)).await?;
    let mut doc = employees::defaults(Section::Documents);
    doc["employee_id"] = json!(id);
    employees::execute(&pool, &manager, &command(Section::Documents, doc)).await?;
    let mut cash = employees::defaults(Section::Cash);
    cash["employee_id"] = json!(id);
    cash["opening_cash"] = json!(100);
    let cid = employees::execute(&pool, &admin, &command(Section::Cash, cash.clone())).await?;
    assert!(
        employees::execute(&pool, &admin, &command(Section::Cash, cash))
            .await
            .is_err()
    );
    let c = record(&pool, &admin, Section::Cash, cid).await?;
    employees::execute(
        &pool,
        &admin,
        &action(Section::Cash, &c, "close", json!({"actual_cash":95})),
    )
    .await?;
    assert_eq!(
        record(&pool, &admin, Section::Cash, cid).await?["difference"],
        -5.0
    );
    for s in employees::SECTIONS {
        employees::list(&pool, &admin, *s, "2020-01-01", "2030-12-31").await?;
    }
    let assign = record(&pool, &admin, Section::Assignments, aid).await?;
    employees::execute(
        &pool,
        &manager,
        &action(Section::Assignments, &assign, "delete", json!({})),
    )
    .await?;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&pool)
        .await?;
    pool.close().await;
    Ok(())
}
