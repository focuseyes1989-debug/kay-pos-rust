use crate::storage as sqlx;
use anyhow::Result;
use sqlx::PgPool;
use crate::auth::{Permission, Session};

#[derive(Clone, Default, sqlx::FromRow)]
pub struct Row {
    pub label: String,
    pub amount: f64,
    pub count: i64,
}
#[derive(Clone, Default)]
pub struct Dashboard {
    pub period: Vec<Row>,
    pub daily: Vec<Row>,
    pub hourly: Vec<Row>,
    pub customers: Vec<Row>,
    pub suppliers: Vec<Row>,
    pub operations: Vec<Row>,
    pub loaded_at: String,
}

const HOURLY_SQL: &str = "SELECT to_char(created_at,'HH24') AS label,COALESCE(SUM(total),0)::float8 AS amount,COUNT(*) AS count FROM sales WHERE status='completed' AND created_at >= $1 AND created_at < $2 GROUP BY 1 ORDER BY 1";

pub async fn load(pool: &PgPool, actor: &Session, from: &str, to: &str) -> Result<Dashboard> {
    let (start,end) = crate::sale_summary::dates(from,to)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *tx).await?;
    sqlx::query("SET LOCAL statement_timeout='20s'").execute(&mut *tx).await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let period = sqlx::query_as(include_str!("dashboard_period.sql"))
        .bind(start.to_string()).bind(end.to_string()).fetch_all(&mut *tx).await?;
    let daily = sqlx::query_as("SELECT to_char(created_at,'YYYY-MM-DD') AS label,COALESCE(SUM(total),0)::float8 AS amount,COUNT(*) AS count FROM sales WHERE status='completed' AND created_at >= $1 AND created_at < $2 GROUP BY 1 ORDER BY 1")
        .bind(start.and_hms_opt(0,0,0).unwrap()).bind(end.and_hms_opt(0,0,0).unwrap()).fetch_all(&mut *tx).await?;
    let hourly_rows: Vec<Row> = sqlx::query_as(HOURLY_SQL)
        .bind(start.and_hms_opt(0,0,0).unwrap()).bind(end.and_hms_opt(0,0,0).unwrap()).fetch_all(&mut *tx).await?;
    let hourly = (0..24).map(|hour| {
        let label = format!("{hour:02}");
        hourly_rows.iter().find(|row|row.label==label).cloned().unwrap_or(Row {label, ..Row::default()})
    }).collect();
    let customers = sqlx::query_as("SELECT COALESCE(name,'Customer') || ' #' || id::text AS label,current_balance::float8 AS amount,1::bigint AS count FROM customers WHERE current_balance>0 ORDER BY current_balance DESC,id")
        .fetch_all(&mut *tx).await?;
    // Match Main POS supplier ledger: Purchase is a debit; other types are credits.
    // Keep negative supplier balances (advances) separate from amounts payable.
    let suppliers = sqlx::query_as("SELECT COALESCE(s.name,'Supplier') || ' #' || s.id::text AS label,COALESCE(SUM(CASE WHEN p.payment_type='Purchase' THEN p.amount WHEN p.payment_type<>'Purchase' THEN -p.amount ELSE 0 END),0)::float8 AS amount,COUNT(p.id) AS count FROM suppliers s JOIN supplier_payments p ON p.supplier_id=s.id GROUP BY s.id,s.name HAVING SUM(CASE WHEN p.payment_type='Purchase' THEN p.amount WHEN p.payment_type<>'Purchase' THEN -p.amount ELSE 0 END)<>0 ORDER BY amount DESC,s.id")
        .fetch_all(&mut *tx).await?;
    let mut operations: Vec<Row> = sqlx::query_as(r#"
        SELECT 'Open service orders'::text AS label,0::float8 AS amount,COUNT(*) AS count FROM service_orders WHERE status NOT IN ('delivered','cancelled')
        UNION ALL SELECT 'Outstanding invoices',COALESCE(SUM(balance_amount),0)::float8,COUNT(*) FROM credit_sales WHERE balance_amount>0 AND COALESCE(status,'pending')<>'refunded'
        UNION ALL SELECT 'Overdue invoices',COALESCE(SUM(balance_amount),0)::float8,COUNT(*) FROM credit_sales WHERE balance_amount>0 AND COALESCE(status,'pending')<>'refunded' AND NULLIF(due_date,'')<to_char(CURRENT_DATE,'YYYY-MM-DD')
        UNION ALL SELECT 'Credit marked fully paid (period)',COALESCE(SUM(total),0)::float8,COUNT(*) FROM sales WHERE status='completed' AND lower(trim(payment_type))='credit' AND payment>=total AND total>0 AND created_at >= $1 AND created_at < $2
    "#).bind(start.and_hms_opt(0,0,0).unwrap()).bind(end.and_hms_opt(0,0,0).unwrap()).fetch_all(&mut *tx).await?;
    for (table, query) in [
        ("purchase_orders", "SELECT 'Purchase orders without ledger entries'::text AS label,COALESCE(SUM(total_amount),0)::float8 AS amount,COUNT(*) AS count FROM purchase_orders po WHERE NOT EXISTS(SELECT 1 FROM supplier_payments sp WHERE sp.purchase_order_id=po.id)"),
        ("employees", "SELECT 'Active employees'::text AS label,0::float8 AS amount,COUNT(*) AS count FROM employees WHERE lower(employment_status)='active'"),
    ] {
        let exists: bool=sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL").bind(table).fetch_one(&mut *tx).await?;
        if exists { operations.push(sqlx::query_as(query).fetch_one(&mut *tx).await?); }
    }
    tx.commit().await?;
    Ok(Dashboard { period,daily,hourly,customers,suppliers,operations,loaded_at:chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string() })
}

#[cfg(test)]
mod hourly_tests {
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL on port 55487"]
    async fn postgres_hourly_sales_boundaries_and_completed_only() -> anyhow::Result<()> {
        use ::sqlx::Connection;
        let url=std::env::var("P1_TEST_DATABASE_URL")?;
        anyhow::ensure!(url.contains("127.0.0.1:55487/"),"Use isolated test PostgreSQL");
        let mut conn=::sqlx::PgConnection::connect(&url).await?;
        let mut tx=conn.begin().await?;
        ::sqlx::query("CREATE TEMP TABLE sales(created_at timestamp,total numeric,status text) ON COMMIT DROP").execute(&mut *tx).await?;
        ::sqlx::query("INSERT INTO sales VALUES ('2026-10-12 09:00',100,'completed'),('2026-10-12 09:59:59',200,'completed'),('2026-10-12 23:59:59',50,'completed'),('2026-10-13 00:00',900,'completed'),('2026-10-12 09:30',800,'refunded')").execute(&mut *tx).await?;
        let start=chrono::NaiveDate::from_ymd_opt(2026,10,12).unwrap().and_hms_opt(0,0,0).unwrap();
        let rows:Vec<(String,f64,i64)>=::sqlx::query_as(super::HOURLY_SQL).bind(start).bind(start+chrono::Duration::days(1)).fetch_all(&mut *tx).await?;
        assert_eq!(rows,vec![("09".into(),300.0,2),("23".into(),50.0,1)]);
        tx.rollback().await?;
        Ok(())
    }
}
