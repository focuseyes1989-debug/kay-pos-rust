use anyhow::{ensure, Context, Result};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use crate::{auth::{Permission,Session}, discounts::Discount};

#[derive(Clone,Debug,PartialEq,Serialize,Deserialize,sqlx::FromRow)]
pub struct Budget {pub category:String,pub month:i32,pub year:i32,pub budget_amount:Decimal,pub notes:String}
#[derive(Clone,Debug,PartialEq,sqlx::FromRow)]
pub struct BudgetRow {pub category:String,pub budget_amount:Decimal,pub actual:Decimal,pub notes:String,pub exists:bool}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize,sqlx::FromRow)]
pub struct Settings {pub enable_notifications:i32,pub warning_threshold:i32,pub check_frequency:String}
#[derive(Clone,Debug,PartialEq,sqlx::FromRow)]
pub struct Alert {pub id:i32,pub category:String,pub message:String,pub is_read:i32,pub created_at:String}
#[derive(Clone,Debug,PartialEq,sqlx::FromRow)]
pub struct Attachment {pub id:i32,pub filename:String,pub file_path:String,pub file_size:i32,pub mime_type:String,pub uploaded_by:String,pub stored:bool}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
pub enum Action {
    Discount{old:Option<Discount>,new:Discount},
    Budget{old:Option<Budget>,new:Budget},
    Settings{old:Settings,new:Settings},
    ReadAlert{i:i32},
    Attach{expense_id:i32,filename:String,content:Vec<u8>},
    RemoveAttachment{i:i32},
}
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
pub struct Command {pub request_id:String,pub action:Action}
const SETTINGS:&str="SELECT COALESCE(enable_notifications,1) AS enable_notifications,COALESCE(warning_threshold,80) AS warning_threshold,COALESCE(check_frequency,'daily') AS check_frequency FROM expense_notification_settings ORDER BY id LIMIT 1";

pub fn period(year:i32,month:i32)->Result<String> {
    ensure!((2000..=2100).contains(&year),"Year must be between 2000 and 2100");
    let d=NaiveDate::from_ymd_opt(year,month as u32,1).context("Invalid month")?;
    Ok(d.format("%Y-%m").to_string())
}
pub fn alert_level(budget:Decimal,actual:Decimal,threshold:i32)->Option<&'static str> {
    if budget<=Decimal::ZERO {None} else if actual>=budget {Some("exceeded")} else if actual*Decimal::from(100)>=budget*Decimal::from(threshold) {Some("warning")} else {None}
}
async fn budget_rows(conn:&mut PgConnection,year:i32,month:i32)->Result<Vec<BudgetRow>> {
    let key=period(year,month)?;
    Ok(sqlx::query_as("WITH categories AS (SELECT name AS category FROM expense_categories UNION SELECT category FROM expense_budgets WHERE year=$1 AND month=$2 UNION SELECT category FROM expenses WHERE LEFT(expense_date,7)=$3), actual AS (SELECT category,SUM(amount::numeric) AS amount FROM expenses WHERE LEFT(expense_date,7)=$3 GROUP BY category) SELECT c.category,COALESCE(b.budget_amount,0)::numeric AS budget_amount,COALESCE(a.amount,0) AS actual,COALESCE(b.notes,'') AS notes,b.id IS NOT NULL AS exists FROM categories c LEFT JOIN expense_budgets b ON b.category=c.category AND b.year=$1 AND b.month=$2 LEFT JOIN actual a ON a.category=c.category ORDER BY c.category")
        .bind(year).bind(month).bind(key).fetch_all(conn).await?)
}
pub async fn load(pool:&PgPool,actor:&Session,year:i32,month:i32)->Result<(Vec<BudgetRow>,Settings,Vec<Alert>)> {
    let mut tx=pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY").execute(&mut *tx).await?;
    actor.authorize(&mut tx,Permission::Manage).await?;
    let rows=budget_rows(&mut tx,year,month).await?;
    let settings=sqlx::query_as(SETTINGS).fetch_one(&mut *tx).await?;
    let alerts=sqlx::query_as("SELECT id,COALESCE(category,'') AS category,COALESCE(message,'') AS message,COALESCE(is_read,0) AS is_read,created_at::text AS created_at FROM expense_alerts_log WHERE year=$1 AND month=$2 ORDER BY created_at DESC,id DESC").bind(year).bind(month).fetch_all(&mut *tx).await?;
    tx.commit().await?;Ok((rows,settings,alerts))
}
pub async fn attachments(pool:&PgPool,actor:&Session,expense:i32)->Result<Vec<Attachment>> {
    let mut tx=pool.begin().await?;actor.authorize(&mut tx,Permission::Manage).await?;
    let rows=sqlx::query_as("SELECT a.id,a.filename,a.file_path,COALESCE(a.file_size,0) AS file_size,COALESCE(a.mime_type,'') AS mime_type,COALESCE(a.uploaded_by,'') AS uploaded_by,EXISTS(SELECT 1 FROM rust_expense_files f WHERE f.attachment_id=a.id) AS stored FROM expense_attachments a WHERE expense_id=$1 ORDER BY a.id DESC").bind(expense).fetch_all(&mut *tx).await?;
    tx.commit().await?;Ok(rows)
}
pub fn file_type(name:&str,content:&[u8])->Result<&'static str> {
    ensure!(!content.is_empty()&&content.len()<=10*1024*1024,"File must be between 1 byte and 10 MB");
    ensure!(!name.is_empty() && name.len()<=240 && !name.contains(['/', '\\', ':']) && !name.chars().any(char::is_control),"Invalid filename");
    let ext=name.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "pdf" if content.starts_with(b"%PDF-")=>Ok("application/pdf"),
        "png" if content.starts_with(b"\x89PNG\r\n\x1a\n")=>Ok("image/png"),
        "jpg"|"jpeg" if content.starts_with(&[0xff,0xd8,0xff])=>Ok("image/jpeg"),
        _=>anyhow::bail!("Only PDF, PNG and JPEG files with matching file signatures are accepted")
    }
}
pub async fn download(pool:&PgPool,actor:&Session,id:i32)->Result<(String,Vec<u8>)> {
    let mut tx=pool.begin().await?;actor.authorize(&mut tx,Permission::Manage).await?;
    let (name,bytes,hash):(String,Vec<u8>,String)=sqlx::query_as("SELECT a.filename,f.content,f.sha256 FROM expense_attachments a JOIN rust_expense_files f ON f.attachment_id=a.id WHERE a.id=$1 AND octet_length(f.content)<=10485760").bind(id).fetch_optional(&mut *tx).await?.context("This attachment is stored on a Main POS computer, not in the database")?;
    file_type(&name,&bytes)?;ensure!(crate::auth::fingerprint(&bytes)==hash,"Attachment checksum failed");
    tx.commit().await?;Ok((name,bytes))
}
pub async fn execute(pool:&PgPool,actor:&Session,c:&Command)->Result<i32> {
    ensure!(c.request_id.starts_with("RUST-")&&c.request_id.len()==37,"Invalid request ID");
    let hash=crate::auth::fingerprint(&serde_json::to_vec(c)?);
    let mut tx=pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='8s'").execute(&mut *tx).await?;
    sqlx::query("SET LOCAL statement_timeout='30s'").execute(&mut *tx).await?;
    actor.authorize(&mut tx,Permission::Manage).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,721007))").bind(&c.request_id).execute(&mut *tx).await?;
    let prior:Option<(String,String,i32)>=sqlx::query_as("SELECT username,payload_hash,result_id FROM rust_management_requests WHERE request_id=$1").bind(&c.request_id).fetch_optional(&mut *tx).await?;
    if let Some((user,prior,id))=prior {ensure!(user==actor.username()&&prior==hash,"Request identity mismatch");ensure!(id!=0,"This request was cancelled");tx.commit().await?;return Ok(id)}
    let id=match &c.action {
        Action::Discount{old,new}=>crate::discounts::save(&mut tx,old,new).await?,
        Action::Budget{old,new}=>{
            period(new.year,new.month)?;
            ensure!(new.budget_amount>=Decimal::ZERO&&new.budget_amount<=Decimal::from(1_000_000_000_000u64)&&new.budget_amount.scale()<=2&&new.notes.len()<=4000,"Invalid budget or notes");
            sqlx::query("SELECT pg_advisory_xact_lock(721007,1)").execute(&mut *tx).await?;
            let saved:Option<Budget>=sqlx::query_as("SELECT category,month,year,budget_amount::numeric,COALESCE(notes,'') AS notes FROM expense_budgets WHERE category=$1 AND month=$2 AND year=$3 FOR UPDATE").bind(&new.category).bind(new.month).bind(new.year).fetch_optional(&mut *tx).await?;
            ensure!(&saved==old,"Budget changed; refresh before saving");
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM expense_categories WHERE name=$1)").bind(&new.category).fetch_one(&mut *tx).await?;
            ensure!(exists||saved.is_some(),"Expense category no longer exists");
            sqlx::query_scalar("INSERT INTO expense_budgets(category,month,year,budget_amount,notes) VALUES($1,$2,$3,$4::numeric::float8,$5) ON CONFLICT(category,month,year) DO UPDATE SET budget_amount=EXCLUDED.budget_amount,notes=EXCLUDED.notes,updated_at=CURRENT_TIMESTAMP RETURNING id")
                .bind(&new.category).bind(new.month).bind(new.year).bind(new.budget_amount).bind(&new.notes).fetch_one(&mut *tx).await?
        },
        Action::Settings{old,new}=>{
            ensure!(matches!(new.enable_notifications,0|1)&&(50..=100).contains(&new.warning_threshold)&&matches!(new.check_frequency.as_str(),"daily"|"weekly"|"monthly"),"Invalid notification settings");
            let saved:Settings=sqlx::query_as(&format!("{SETTINGS} FOR UPDATE")).fetch_one(&mut *tx).await?;
            ensure!(&saved==old,"Notification settings changed; refresh first");
            sqlx::query_scalar("UPDATE expense_notification_settings SET enable_notifications=$1,warning_threshold=$2,check_frequency=$3,last_checked=NULL,updated_at=CURRENT_TIMESTAMP WHERE id=(SELECT id FROM expense_notification_settings ORDER BY id LIMIT 1) RETURNING id")
                .bind(new.enable_notifications).bind(new.warning_threshold).bind(&new.check_frequency).fetch_one(&mut *tx).await?
        },
        Action::ReadAlert{i}=>{sqlx::query("UPDATE expense_alerts_log SET is_read=1 WHERE id=$1").bind(i).execute(&mut *tx).await?;*i},
        Action::Attach{expense_id,filename,content}=>{
            let mime=file_type(filename,content)?;
            sqlx::query("SELECT id FROM expenses WHERE id=$1 FOR UPDATE").bind(expense_id).fetch_optional(&mut *tx).await?.context("Expense no longer exists")?;
            let id:i32=sqlx::query_scalar("INSERT INTO expense_attachments(expense_id,filename,file_path,file_size,mime_type,uploaded_by) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
                .bind(expense_id).bind(filename).bind(format!("rust-db://{}",c.request_id)).bind(content.len() as i32).bind(mime).bind(actor.username()).fetch_one(&mut *tx).await?;
            sqlx::query("INSERT INTO rust_expense_files(attachment_id,content,sha256) VALUES($1,$2,$3)").bind(id).bind(content).bind(crate::auth::fingerprint(content)).execute(&mut *tx).await?;id
        },
        Action::RemoveAttachment{i}=>{
            let stored:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rust_expense_files WHERE attachment_id=$1)").bind(i).fetch_one(&mut *tx).await?;
            ensure!(stored,"Remove legacy files using Main POS");
            sqlx::query("DELETE FROM expense_attachments WHERE id=$1").bind(i).execute(&mut *tx).await?;*i
        }
    };
    sqlx::query("INSERT INTO rust_management_requests(request_id,username,payload_hash,result_id) VALUES($1,$2,$3,$4)").bind(&c.request_id).bind(actor.username()).bind(hash).bind(id).execute(&mut *tx).await?;
    let kind=match &c.action{Action::Discount{..}=>"discount",Action::Budget{..}=>"budget",Action::Settings{..}=>"expense_settings",Action::ReadAlert{..}=>"alert_read",Action::Attach{..}=>"attachment_add",Action::RemoveAttachment{..}=>"attachment_remove"};
    crate::activity::record(&mut tx,actor,&format!("rust.management.{kind}"),&format!("result_id={id}; request={}",c.request_id)).await?;
    tx.commit().await?;Ok(id)
}

pub async fn check_alerts(pool:&PgPool,actor:&Session,force:bool)->Result<usize> {
    let mut tx=pool.begin().await?;actor.authorize(&mut tx,Permission::Manage).await?;
    let s:Settings=sqlx::query_as(&format!("{SETTINGS} FOR UPDATE")).fetch_one(&mut *tx).await?;
    let days=match s.check_frequency.as_str(){"daily"=>1,"weekly"=>7,"monthly"=>30,_=>anyhow::bail!("Invalid alert frequency")};
    let due:bool=sqlx::query_scalar("SELECT last_checked IS NULL OR last_checked<=CURRENT_TIMESTAMP-make_interval(days=>$1) FROM expense_notification_settings ORDER BY id LIMIT 1").bind(days).fetch_one(&mut *tx).await?;
    if s.enable_notifications!=1||(!force&&!due){tx.commit().await?;return Ok(0)}
    let now=chrono::Local::now();let month=now.month() as i32;let year=now.year();
    let rows=budget_rows(&mut tx,year,month).await?;let mut count=0;
    for r in rows {
        if let Some(level)=alert_level(r.budget_amount,r.actual,s.warning_threshold) {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM expense_alerts_log WHERE category=$1 AND year=$2 AND month=$3 AND (alert_type=$4 OR alert_type='exceeded'))").bind(&r.category).bind(year).bind(month).bind(level).fetch_one(&mut *tx).await?;
            if !exists {
                let percent=(r.actual/r.budget_amount*Decimal::from(100)).round_dp(2);
                sqlx::query("INSERT INTO expense_alerts_log(category,year,month,budget_amount,actual_amount,used_percentage,alert_type,message,is_read) VALUES($1,$2,$3,$4::numeric::float8,$5::numeric::float8,$6::numeric::float8,$7,$8,0)")
                    .bind(&r.category).bind(year).bind(month).bind(r.budget_amount).bind(r.actual).bind(percent).bind(level).bind(format!("Budget {level}: {} ({percent}%)",r.category)).execute(&mut *tx).await?;count+=1;
            }
        }
    }
    sqlx::query("UPDATE expense_notification_settings SET last_checked=CURRENT_TIMESTAMP WHERE id=(SELECT id FROM expense_notification_settings ORDER BY id LIMIT 1)").execute(&mut *tx).await?;
    tx.commit().await?;Ok(count)
}
pub async fn resolve(pool:&PgPool,actor:&Session,c:&Command)->Result<bool> {
    let mut tx=pool.begin().await?;actor.authorize(&mut tx,Permission::Manage).await?;
    sqlx::query("SET LOCAL lock_timeout='8s'").execute(&mut *tx).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,721007))").bind(&c.request_id).execute(&mut *tx).await?;
    let hash=crate::auth::fingerprint(&serde_json::to_vec(c)?);
    let row:Option<(String,String,i32)>=sqlx::query_as("SELECT username,payload_hash,result_id FROM rust_management_requests WHERE request_id=$1").bind(&c.request_id).fetch_optional(&mut *tx).await?;
    let committed=if let Some((user,h,id))=row {ensure!(user==actor.username()&&h==hash,"Request identity mismatch");id!=0} else {
        sqlx::query("INSERT INTO rust_management_requests(request_id,username,payload_hash,result_id) VALUES($1,$2,$3,0)").bind(&c.request_id).bind(actor.username()).bind(hash).execute(&mut *tx).await?;false
    };
    tx.commit().await?;Ok(committed)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn limits_and_thresholds(){
        assert_eq!(alert_level(100.into(),80.into(),80),Some("warning"));assert_eq!(alert_level(100.into(),100.into(),80),Some("exceeded"));assert_eq!(alert_level(0.into(),10.into(),80),None);
        assert!(period(2026,13).is_err());assert!(file_type("evil.exe",b"%PDF-1.4").is_err());assert!(file_type("../x.pdf",b"%PDF-1.4").is_err());assert!(file_type("x.pdf",b"not a pdf").is_err());assert!(file_type("x.pdf",b"%PDF-1.4").is_ok());
    }
}
