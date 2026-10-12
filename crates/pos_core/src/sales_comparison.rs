use crate::storage as sqlx;
use anyhow::{ensure,Result};
use crate::{auth::{Session,Permission},models::Money};
use sqlx::PgPool;
use std::collections::{BTreeMap,BTreeSet};
#[derive(Clone,Default)]
pub struct Period {pub sales:Money,pub quantity:Money,pub receipts:usize,pub categories:BTreeMap<String,Money>,pub products:BTreeMap<String,Money>}
#[derive(sqlx::FromRow)]
struct Line {sale_id:i32,product_key:String,category:String,amount:Money,quantity:Money}
const SQL:&str=r"WITH selected AS (SELECT id,total::numeric AS total FROM sales WHERE status='completed' AND created_at >= $1 AND created_at < $2), lines AS (
 SELECT s.id AS sale_id,s.total,i.id AS line_id,COALESCE(i.product_name,p.name,'[Receipt without items]') AS name,
 COALESCE(i.product_id::text,'legacy:'||COALESCE(i.product_name,'')) AS product_id,
 COALESCE(c.name,p.category,'Uncategorized') AS category,
 COALESCE(i.qty,0)::numeric AS qty,COALESCE(i.refunded_qty,0)::numeric AS refunded,
 COALESCE(i.total,COALESCE(i.qty,0)*COALESCE(i.price,0))::numeric AS gross
 FROM selected s LEFT JOIN sale_items i ON i.sale_id=s.id LEFT JOIN products p ON p.id=i.product_id LEFT JOIN categories c ON c.id=p.category_id
), portions AS (SELECT *,SUM(gross) OVER(PARTITION BY sale_id) AS subtotal FROM lines)
 SELECT sale_id,name||' [#'||product_id||']' AS product_key,category,
 CASE WHEN line_id IS NULL THEN total WHEN subtotal>0 AND qty>0 THEN total*gross/subtotal*GREATEST(qty-refunded,0)/qty ELSE 0 END::numeric AS amount,
 GREATEST(qty-refunded,0)::numeric AS quantity FROM portions";
fn aggregate(rows:Vec<Line>)->Result<Period>{
 let mut out=Period::default();let mut receipts=BTreeSet::new();
 for row in rows{ensure!(row.amount>=Money::ZERO && row.quantity>=Money::ZERO,"Sales quantities or amounts require verification");
  out.sales+=row.amount;out.quantity+=row.quantity;
  if row.quantity>Money::ZERO||row.amount>Money::ZERO{receipts.insert(row.sale_id);}
  *out.categories.entry(row.category).or_default()+=row.amount;
  *out.products.entry(row.product_key).or_default()+=row.amount;
 }
 out.receipts=receipts.len();Ok(out)
}
pub async fn load(pool:&PgPool,actor:&Session,a:&str,b:&str,c:&str,d:&str)->Result<(Period,Period)>{
 let ranges=[crate::sale_summary::dates(a,b)?,crate::sale_summary::dates(c,d)?];
 let mut tx=pool.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *tx).await?;
 actor.authorize(&mut tx,Permission::Manage).await?;
 let mut result=Vec::new();
 for (from,to) in ranges{
  let rows=sqlx::query_as::<_,Line>(SQL).bind(from.and_hms_opt(0,0,0).unwrap()).bind(to.and_hms_opt(0,0,0).unwrap()).fetch_all(&mut *tx).await?;
  result.push(aggregate(rows)?);
 }
 tx.commit().await?;let previous=result.pop().unwrap();Ok((result.pop().unwrap(),previous))
}
#[cfg(test)]mod tests{use super::*;#[test]fn totals_and_receipts(){let row=|id,amount,quantity|Line{sale_id:id,product_key:"Product".into(),category:"Category".into(),amount:Money::from(amount),quantity:Money::from(quantity)};let p=aggregate(vec![row(1,50,2),row(1,25,1),row(2,0,0)]).unwrap();assert_eq!(p.receipts,1);assert_eq!(p.sales,75.into());assert_eq!(p.quantity,3.into());assert_eq!(p.categories["Category"],75.into());}}
