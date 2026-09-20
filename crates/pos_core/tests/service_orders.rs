use anyhow::Result;
use pos_core::{
    auth::{self, Session},
    service_orders::{self as jobs, Action, Job, Prompt},
};
use sqlx::{postgres::PgPoolOptions, PgPool};

async fn fixture() -> Result<(PgPool, Session, Session)> {
    let url = std::env::var("P1_TEST_DATABASE_URL")?;
    let bootstrap = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    let schema = format!("jobs_{}", auth::new_request_id().replace("RUST-", ""));
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&bootstrap)
        .await?;
    bootstrap.close().await;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |conn, _| {
            let command = format!("SET search_path TO {schema}");
            Box::pin(async move {
                sqlx::query(&command).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    sqlx::raw_sql(include_str!("fixtures/service_orders.sql"))
        .execute(&pool)
        .await?;
    let mut hash = [0u8; 32];
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
    let a = auth::login(&pool, "operator", "test-only").await?;
    let b = auth::login(&pool, "supervisor", "test-only").await?;
    Ok((pool, a, b))
}
async fn reload(pool: &PgPool, id: i32) -> Result<Job> {
    Ok(jobs::list(pool, "", "", 100)
        .await?
        .into_iter()
        .find(|j| j.id == id)
        .unwrap())
}
async fn create(pool: &PgPool, actor: &Session) -> Result<Job> {
    let mut j = jobs::reserve(pool, actor).await?;
    j.job_title = "Poster".into();
    j.complaint = "A3 print".into();
    jobs::save(pool, actor, &j, true).await?;
    jobs::save(pool, actor, &j, true).await?;
    assert_eq!(jobs::history_list(pool, j.id).await?.len(), 1);
    reload(pool, j.id).await
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn lifecycle_concurrency_and_audit() -> Result<()> {
    let (pool, a, b) = fixture().await?;
    let j = create(&pool, &a).await?;
    let (first, second) = tokio::join!(
        jobs::act(&pool, &a, &j, Action::Start, "start"),
        jobs::act(&pool, &b, &j, Action::Start, "start")
    );
    assert_ne!(first.is_ok(), second.is_ok());
    let started = reload(&pool, j.id).await?;
    assert_eq!(started.status, "in_progress");
    assert!(started.started_at.is_some());
    assert_eq!(
        started.started_by,
        if first.is_ok() {
            "operator"
        } else {
            "supervisor"
        }
    );
    assert!(jobs::act(&pool, &a, &j, Action::Cancel, "").await.is_err());
    jobs::act(&pool, &b, &started, Action::Complete, "").await?;
    let ready = reload(&pool, j.id).await?;
    assert_eq!(ready.completed_by, "supervisor");
    assert!(ready.completed_at.is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM service_order_notifications")
            .fetch_one(&pool)
            .await?,
        1
    );
    jobs::act(&pool, &a, &ready, Action::Collect, "collected").await?;
    let done = reload(&pool, j.id).await?;
    assert_eq!(done.delivered_by, "operator");
    assert_eq!(done.status, "delivered");
    assert!(jobs::save(&pool, &a, &done, false).await.is_err());
    assert_eq!(jobs::history_list(&pool, j.id).await?.len(), 4);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn edit_delete_permissions_and_financial_guard() -> Result<()> {
    let (pool, a, b) = fixture().await?;
    let mut j = create(&pool, &a).await?;
    j.internal_notes = "Paid at counter".into();
    jobs::save(&pool, &a, &j, false).await?;
    assert!(jobs::save(&pool, &b, &j, false).await.is_err());
    let j = reload(&pool, j.id).await?;
    jobs::act(&pool, &a, &j, Action::Cancel, "").await?;
    let j = reload(&pool, j.id).await?;
    assert!(jobs::act(&pool, &a, &j, Action::Delete, "").await.is_err());
    sqlx::query("UPDATE service_orders SET sale_id=5 WHERE id=$1")
        .bind(j.id)
        .execute(&pool)
        .await?;
    assert!(jobs::act(&pool, &b, &j, Action::Delete, "").await.is_err());
    sqlx::query("UPDATE service_orders SET sale_id=NULL WHERE id=$1")
        .bind(j.id)
        .execute(&pool)
        .await?;
    jobs::act(&pool, &b, &j, Action::Delete, "").await?;
    assert!(jobs::list(&pool, "", "", 10).await?.is_empty());
    sqlx::query("UPDATE users SET is_active=0 WHERE username='operator'")
        .execute(&pool)
        .await?;
    assert!(jobs::reserve(&pool, &a).await.is_err());
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn prompts_and_legacy_ready_states() -> Result<()> {
    let (pool, a, _) = fixture().await?;
    let p = Prompt {
        title: "Poster".into(),
        category: "Print".into(),
        prompt_text: "{job_title}: {details} {notes} {status}".into(),
        active: 1,
        ..Default::default()
    };
    jobs::save_prompt(&pool, &a, &p).await?;
    let mut saved = jobs::prompts(&pool).await?.remove(0);
    assert_eq!(jobs::prompt(&pool, saved.id).await?.category, "Print");
    sqlx::query("UPDATE service_order_design_prompts SET image_data='test-image-payload' WHERE id=$1")
        .bind(saved.id).execute(&pool).await?;
    assert!(jobs::prompts(&pool).await?[0].image_data.is_empty());
    assert_eq!(jobs::prompt_image(&pool,saved.id).await?,"test-image-payload");
    assert_eq!(jobs::prompt(&pool,saved.id).await?.image_data,"test-image-payload");
    sqlx::query("UPDATE service_order_design_prompts SET image_data=NULL WHERE id=$1")
        .bind(saved.id).execute(&pool).await?;
    saved.active = 0;
    jobs::save_prompt(&pool, &a, &saved).await?;
    assert!(jobs::save_prompt(&pool, &a, &saved).await.is_err());
    assert_eq!(jobs::prompts(&pool).await?[0].active, 0);
    for status in ["ready", "completed"] {
        let j = create(&pool, &a).await?;
        sqlx::query("UPDATE service_orders SET status=$1 WHERE id=$2")
            .bind(status)
            .bind(j.id)
            .execute(&pool)
            .await?;
        let mut j = reload(&pool, j.id).await?;
        j.job_title = "{notes}".into();
        assert!(jobs::render_prompt(&p.prompt_text, &j).starts_with("{notes}: A3 print"));
        jobs::act(&pool, &a, &j, Action::Collect, "").await?;
    }
    pool.close().await;
    Ok(())
}
