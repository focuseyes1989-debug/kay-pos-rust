use anyhow::{ensure, Result};
use chrono::NaiveDate;
use sqlx::PgPool;

#[derive(Clone, Default, sqlx::FromRow)]
pub struct SummaryRow {
    pub label: String,
    pub receipts: i64,
    pub sales: f64,
    pub gross: f64,
    pub discount: f64,
    pub cost: f64,
    pub refunds: f64,
    pub estimated: i64,
}
#[derive(Clone, Default)]
pub struct Summary {
    pub daily: Vec<SummaryRow>,
    pub payments: Vec<SummaryRow>,
    pub groups: Vec<Vec<GroupRow>>,
    pub group_receipts: Vec<i64>,
    pub expenses: Vec<ExpenseRow>,
}
#[derive(Clone, sqlx::FromRow)]
pub struct GroupRow {
    pub label: String,
    pub receipts: i64,
    pub quantity: f64,
    pub gross: f64,
    pub discount: f64,
    pub net: f64,
    pub cost: f64,
    pub savings: f64,
}
#[derive(Clone, sqlx::FromRow)]
pub struct ExpenseRow {
    pub label: String,
    pub records: i64,
    pub amount: f64,
}

const GROUP_LINES: &str = r#"
WITH window_sales AS (
 SELECT * FROM sales WHERE status='completed' AND created_at >= $1 AND created_at < $2
), raw AS (
 SELECT s.id AS sale_id,si.id AS line_id,
 COALESCE(si.product_name,'[Receipt without items]') AS item,
 COALESCE(c.name,p.category,'Uncategorized') AS category,
 COALESCE(parent.name,c.name,p.category,'Uncategorized') AS parent,
 COALESCE(si.qty,0)::numeric AS quantity,
 (COALESCE(si.qty,0)*COALESCE(si.price,0))::numeric AS gross,
 (COALESCE(si.qty,0)*COALESCE(si.cost,v.cost,p.cost,0))::numeric AS cost,
 COALESCE(s.discount_amount,0)::numeric AS header_discount,
 COALESCE(si.wholesale_savings,0)::numeric AS savings,
 COALESCE(si.wholesale_tier_min_qty,0) AS tier
 FROM window_sales s LEFT JOIN sale_items si ON si.sale_id=s.id
 LEFT JOIN products p ON p.id=COALESCE(si.product_id,(SELECT p2.id FROM products p2 WHERE p2.name=si.product_name ORDER BY p2.id DESC LIMIT 1))
 LEFT JOIN product_variants v ON v.id=si.variant_id AND v.product_id=p.id
 LEFT JOIN categories c ON c.id=COALESCE(p.category_id,(SELECT c2.id FROM categories c2 WHERE c2.name=p.category ORDER BY c2.id DESC LIMIT 1))
 LEFT JOIN categories parent ON parent.id=c.parent_id
), portions AS (
 SELECT *,ROW_NUMBER() OVER(PARTITION BY sale_id ORDER BY line_id) AS pos,
 COUNT(*) OVER(PARTITION BY sale_id) AS line_count,
 ROUND(CASE WHEN SUM(gross) OVER(PARTITION BY sale_id)<>0
 THEN header_discount*gross/SUM(gross) OVER(PARTITION BY sale_id)
 ELSE header_discount/COUNT(*) OVER(PARTITION BY sale_id) END,2) AS portion
 FROM raw
), lines AS (
 SELECT *,CASE WHEN pos=line_count THEN header_discount-(SUM(portion) OVER(PARTITION BY sale_id)-portion) ELSE portion END AS discount
 FROM portions
) "#;
pub fn dates(from: &str, to: &str) -> Result<(NaiveDate, NaiveDate)> {
    let start = NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
    let end = NaiveDate::parse_from_str(to, "%Y-%m-%d")?;
    ensure!(start <= end, "From date must not be after To date");
    Ok((
        start,
        end.succ_opt()
            .ok_or_else(|| anyhow::anyhow!("Invalid end date"))?,
    ))
}
pub async fn load(pool: &PgPool, from: &str, to: &str) -> Result<Summary> {
    let (start, end) = dates(from, to)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let mut result = Summary::default();
    for (field, daily) in [
        ("to_char(s.created_at,'YYYY-MM-DD')", true),
        ("COALESCE(NULLIF(s.payment_type,''),'Other')", false),
    ] {
        let sql=format!("WITH item_totals AS (SELECT si.sale_id,SUM(COALESCE(si.qty,0)*COALESCE(si.price,0)) AS gross,SUM(COALESCE(si.qty,0)*COALESCE(si.cost,pv.cost,p.cost,0)) AS cost,COUNT(*) FILTER(WHERE si.cost IS NULL) AS estimated FROM sale_items si JOIN sales s ON s.id=si.sale_id LEFT JOIN products p ON p.id=COALESCE(si.product_id,(SELECT p2.id FROM products p2 WHERE p2.name=si.product_name ORDER BY p2.id DESC LIMIT 1)) LEFT JOIN product_variants pv ON pv.id=si.variant_id AND pv.product_id=p.id WHERE s.created_at >= $1 AND s.created_at < $2 GROUP BY si.sale_id) SELECT {field} AS label,COUNT(*) FILTER(WHERE s.status='completed') AS receipts,COALESCE(SUM(s.total) FILTER(WHERE s.status='completed'),0)::float8 AS sales,COALESCE(SUM(i.gross) FILTER(WHERE s.status='completed'),0)::float8 AS gross,COALESCE(SUM(s.discount_amount) FILTER(WHERE s.status='completed'),0)::float8 AS discount,COALESCE(SUM(i.cost) FILTER(WHERE s.status='completed'),0)::float8 AS cost,COALESCE(SUM(s.total) FILTER(WHERE s.status='refunded'),0)::float8 AS refunds,COALESCE(SUM(i.estimated) FILTER(WHERE s.status='completed'),0)::bigint AS estimated FROM sales s LEFT JOIN item_totals i ON i.sale_id=s.id WHERE s.created_at >= $1 AND s.created_at < $2 AND s.status IN ('completed','refunded') GROUP BY {field} ORDER BY {field}");
        let rows = sqlx::query_as::<_, SummaryRow>(&sql)
            .bind(start.and_hms_opt(0, 0, 0).unwrap())
            .bind(end.and_hms_opt(0, 0, 0).unwrap())
            .fetch_all(&mut *tx)
            .await?;
        if daily {
            result.daily = rows;
        } else {
            result.payments = rows;
        }
    }
    for (field, filter) in [
        ("category", "TRUE"),
        ("parent", "TRUE"),
        ("item", "TRUE"),
        ("item", "savings>0 OR tier>0"),
        ("item", "header_discount>0"),
    ] {
        let sql=format!("{GROUP_LINES} SELECT {field} AS label,COUNT(DISTINCT sale_id) AS receipts,SUM(quantity)::float8 AS quantity,SUM(gross)::float8 AS gross,SUM(discount)::float8 AS discount,SUM(gross-discount)::float8 AS net,SUM(cost)::float8 AS cost,SUM(savings)::float8 AS savings FROM lines WHERE {filter} GROUP BY {field} ORDER BY SUM(gross-discount) DESC,{field}");
        result.groups.push(
            sqlx::query_as(&sql)
                .bind(start.and_hms_opt(0, 0, 0).unwrap())
                .bind(end.and_hms_opt(0, 0, 0).unwrap())
                .fetch_all(&mut *tx)
                .await?,
        );
        result.group_receipts.push(
            sqlx::query_scalar(&format!(
                "{GROUP_LINES} SELECT COUNT(DISTINCT sale_id) FROM lines WHERE {filter}"
            ))
            .bind(start.and_hms_opt(0, 0, 0).unwrap())
            .bind(end.and_hms_opt(0, 0, 0).unwrap())
            .fetch_one(&mut *tx)
            .await?,
        );
    }
    result.expenses=sqlx::query_as("SELECT COALESCE(NULLIF(category,''),'Uncategorized') AS label,COUNT(*) AS records,COALESCE(SUM(amount),0)::float8 AS amount FROM expenses WHERE expense_date >= $1 AND expense_date < $2 GROUP BY COALESCE(NULLIF(category,''),'Uncategorized') ORDER BY SUM(amount) DESC,label").bind(start.to_string()).bind(end.to_string()).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inclusive_range() {
        let (a, b) = dates("2026-09-17", "2026-09-17").unwrap();
        assert_eq!((b - a).num_days(), 1);
        assert!(dates("2026-09-18", "2026-09-17").is_err());
        assert!(dates("bad", "2026-09-17").is_err());
    }
}
