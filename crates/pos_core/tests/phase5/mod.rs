use super::*;
use pos_core::{
    held_sales::{self as holds, Action, Command, Hold},
    loyalty,
};

async fn setup() -> Result<(PgPool, Session, Session)> {
    anyhow::ensure!(
        std::env::var("P1_TEST_DATABASE_URL")?.contains("127.0.0.1:55487/"),
        "Isolated test database required"
    );
    let data = fixture().await?;
    sqlx::raw_sql(include_str!("../fixtures/phase5.sql"))
        .execute(&data.0)
        .await?;
    for _ in 0..2 {
        sqlx::raw_sql(include_str!("../../../../migrations/008_held_loyalty.sql"))
            .execute(&data.0)
            .await?;
    }
    Ok(data)
}

fn command(action: Action) -> Command {
    Command {
        request_id: auth::new_request_id(),
        action,
    }
}

fn sample() -> Hold {
    Hold {
        id: 0,
        hold_no: String::new(),
        cart_json: r#"[{"id":1,"qty":1,"price":100}]"#.into(),
        customer_id: Some(1),
        customer_name: String::new(),
        payment_type: "Cash".into(),
        note: "Counter hold".into(),
        total_amount: 100.0,
        item_count: 1,
        created_at: String::new(),
    }
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn held_retry_claim_cancel_and_checkout_are_atomic() -> Result<()> {
    let (pool, cashier, manager) = setup().await?;
    let save = command(Action::Save {
        hold: sample(),
        return_token: None,
    });
    let (a, b) = tokio::join!(
        holds::execute(&pool, &cashier, &save),
        holds::execute(&pool, &cashier, &save)
    );
    let saved = a?;
    assert_eq!(saved, b?);
    assert_eq!(saved.hold.customer_name, "Test customer");
    assert_eq!(holds::list(&pool, &cashier).await?.len(), 1);
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
        0
    );
    assert_eq!(loyalty::history(&pool, &manager, 1).await?.0, 0);
    let first = command(Action::Resume { id: saved.hold.id });
    let second = command(Action::Resume { id: saved.hold.id });
    let (a, b) = tokio::join!(
        holds::execute(&pool, &cashier, &first),
        holds::execute(&pool, &cashier, &second)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let (winner, loser) = if a.is_ok() {
        (&first, &second)
    } else {
        (&second, &first)
    };
    let resumed = holds::execute(&pool, &cashier, winner).await?;
    let token = resumed.token.clone().unwrap();
    assert!(holds::session(&pool, &manager, &token).await.is_err());
    assert!(!holds::cancel_pending(&pool, &cashier, loser).await?);
    assert!(holds::execute(&pool, &cashier, loser).await.is_err());
    assert!(holds::cancel_pending(&pool, &cashier, winner).await?);
    let mut sale = draft(vec![item(1, 1.0)]);
    sale.held_token = Some(token.clone());
    sale.customer_id = Some(1);
    let (a, b) = tokio::join!(
        complete_sale(&pool, &sale, &cashier),
        complete_sale(&pool, &sale, &cashier)
    );
    assert_eq!(a?.id, b?.id);
    assert!(holds::session(&pool, &cashier, &token).await?.is_none());
    assert!(holds::list(&pool, &cashier).await?.is_empty());
    sale.invoice_no = auth::new_request_id();
    assert!(complete_sale(&pool, &sale, &cashier).await.is_err());
    let (points, history) = loyalty::history(&pool, &manager, 1).await?;
    assert_eq!(points, 2);
    assert_eq!(history.len(), 1);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn held_return_and_failed_checkout_preserve_recovery() -> Result<()> {
    let (pool, cashier, _) = setup().await?;
    let saved = holds::execute(
        &pool,
        &cashier,
        &command(Action::Save {
            hold: sample(),
            return_token: None,
        }),
    )
    .await?;
    let resumed = holds::execute(
        &pool,
        &cashier,
        &command(Action::Resume { id: saved.hold.id }),
    )
    .await?;
    let mut sale = draft(vec![item(1, 99.0)]);
    sale.held_token = resumed.token.clone();
    sale.customer_id = Some(1);
    assert!(complete_sale(&pool, &sale, &cashier).await.is_err());
    assert!(
        holds::session(&pool, &cashier, resumed.token.as_ref().unwrap())
            .await?
            .is_some()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM customer_points_log")
            .fetch_one(&pool)
            .await?,
        0
    );
    let returned = holds::execute(
        &pool,
        &cashier,
        &command(Action::Save {
            hold: resumed.hold,
            return_token: resumed.token.clone(),
        }),
    )
    .await?;
    assert_ne!(returned.hold.id, saved.hold.id);
    assert!(
        holds::session(&pool, &cashier, resumed.token.as_ref().unwrap())
            .await?
            .is_none()
    );
    let cancelled = command(Action::Save {
        hold: sample(),
        return_token: None,
    });
    assert!(!holds::cancel_pending(&pool, &cashier, &cancelled).await?);
    assert!(holds::execute(&pool, &cashier, &cancelled).await.is_err());
    sqlx::query("UPDATE users SET is_active=0 WHERE username='operator'")
        .execute(&pool)
        .await?;
    assert!(holds::list(&pool, &cashier).await.is_err());
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires isolated P1_TEST_DATABASE_URL"]
async fn points_credit_refund_and_permissions() -> Result<()> {
    let (pool, cashier, manager) = setup().await?;
    let mut cash = draft(vec![item(1, 1.0)]);
    cash.customer_id = Some(1);
    let saved = complete_sale(&pool, &cash, &cashier).await?;
    complete_sale(&pool, &cash, &cashier).await?;
    assert_eq!(loyalty::history(&pool, &manager, 1).await?.0, 2);
    assert!(loyalty::history(&pool, &cashier, 1).await.is_err());
    let mut credit = draft(vec![item(1, 1.0)]);
    credit.customer_id = Some(1);
    credit.payment_type = "Credit".into();
    credit.payment = 0.0;
    complete_sale(&pool, &credit, &cashier).await?;
    assert_eq!(loyalty::history(&pool, &manager, 1).await?.0, 2);
    sqlx::query("UPDATE settings SET value='10' WHERE key='loyalty_points_per_dollar'")
        .execute(&pool)
        .await?;
    refund_sale(&pool, saved.id, &manager).await?;
    assert!(refund_sale(&pool, saved.id, &manager).await.is_err());
    let (points, history) = loyalty::history(&pool, &manager, 1).await?;
    assert_eq!(points, 0);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].points, -2);
    assert_eq!(history[0].kind, "refund");
    pool.close().await;
    Ok(())
}
