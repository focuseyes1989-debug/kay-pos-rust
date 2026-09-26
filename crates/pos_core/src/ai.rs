use crate::auth::{Permission, Session};
use anyhow::{ensure, Context, Result};
use chrono::{Datelike, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scope {
    Dashboard,
    Analytics,
    Products,
    Digest,
}
impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Dashboard => "လုပ်ငန်းအခြေအနေ အကူအညီ",
            Self::Analytics => "အရောင်းအချက်အလက် ခွဲခြမ်းစိတ်ဖြာမှု",
            Self::Products => "ကုန်ပစ္စည်း အကူအညီ",
            Self::Digest => "လုပ်ငန်းအကျဉ်းချုပ်",
        }
    }
    fn permissions(self) -> &'static [&'static str] {
        match self {
            Self::Products => &["ai_pages", "products"],
            Self::Analytics => &["ai_pages", "sales_summary"],
            _ => &[
                "ai_pages",
                "dashboard",
                "sales_summary",
                "expense",
                "credit",
                "products",
            ],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub scope: Scope,
    pub from: String,
    pub to: String,
    pub product: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub title: String,
    pub from: String,
    pub to: String,
    pub generated_at: String,
    pub sources: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub notes: Vec<String>,
}

// Only explicit supported intents are routed. User text is never executed as SQL.
pub fn chat(text: &str, from: &str, to: &str) -> Result<Request> {
    ensure!(text.len() <= 1000, "မေးခွန်းသည် ရှည်လွန်းနေပါသည်");
    let q = text.trim();
    ensure!(!q.is_empty(), "မေးလိုသည့်အကြောင်းအရာကို ရိုက်ထည့်ပါ");
    let lower = q.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    let first_word = lower.split_whitespace().next().unwrap_or_default();
    ensure!(
        !["select", "insert", "update", "delete", "drop", "alter", "create", "truncate"]
            .iter()
            .any(|keyword| first_word == *keyword)
            && !lower.contains(';')
            && !lower.contains("--"),
        "SQL command များကို အသုံးပြုခွင့်မပြုပါ"
    );
    let (scope, product) = if lower.starts_with("product ") {
        (Scope::Products, q[8..].trim().into())
    } else if q.starts_with("ပစ္စည်း ") {
        (
            Scope::Products,
            q.trim_start_matches("ပစ္စည်း ").trim().into(),
        )
    } else if lower.contains("dashboard")
        || lower.contains("business summary")
        || q.contains("လုပ်ငန်းအကျဉ်းချုပ်")
    {
        (Scope::Dashboard, String::new())
    } else if lower.contains("digest")
        || lower.contains("executive summary")
        || lower == "summary"
        || q.contains("အနှစ်ချုပ်")
    {
        (Scope::Digest, String::new())
    } else if lower.contains("sales")
        || lower.contains("sale")
        || lower.contains("analytics")
        || q.contains("အရောင်း")
    {
        (Scope::Analytics, String::new())
    } else {
        anyhow::bail!("မေးနိုင်သည့်ပုံစံများ - sales today, sales yesterday, sales this week, sales this month, dashboard, digest နှင့် product <အမည် / SKU / barcode>။ ဒေတာပြောင်းလဲခြင်းနှင့် SQL command များကို ခွင့်မပြုပါ။")
    };
    let today = Local::now().date_naive();
    let (from, to) = if lower.contains("yesterday") || q.contains("မနေ့က") {
        let day = today - Duration::days(1);
        (day.to_string(), day.to_string())
    } else if lower.contains("this week") || q.contains("ဒီအပတ်") {
        let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
        (start.to_string(), today.to_string())
    } else if lower.contains("this month") || q.contains("ဒီလ") {
        (
            today.with_day(1).context("လက်ရှိလ ရက်စွဲမမှန်ပါ")?.to_string(),
            today.to_string(),
        )
    } else if lower.contains("today") || q.contains("ဒီနေ့") || q.contains("ယနေ့") {
        (today.to_string(), today.to_string())
    } else {
        (from.into(), to.into())
    };
    Ok(Request {
        scope,
        from,
        to,
        product,
    })
}
fn dates(r: &Request) -> Result<(NaiveDate, NaiveDate)> {
    let from = NaiveDate::parse_from_str(&r.from, "%Y-%m-%d")?;
    let to = NaiveDate::parse_from_str(&r.to, "%Y-%m-%d")?;
    ensure!(
        from <= to && (to - from).num_days() < 366,
        "ရက်အပိုင်းအခြားကို 366 ရက်အတွင်း ရွေးပါ"
    );
    ensure!(r.product.len() <= 200, "ကုန်ပစ္စည်းရှာဖွေစာသား ရှည်လွန်းနေပါသည်");
    Ok((from, to.succ_opt().context("ရက်စွဲသည် ခွင့်ပြုအပိုင်းအခြားပြင်ပ ရောက်နေပါသည်")?))
}
async fn authorize(conn: &mut PgConnection, actor: &Session, scope: Scope) -> Result<()> {
    actor.authorize(conn, Permission::Sell).await?;
    if actor.allows(Permission::Admin) {
        return Ok(());
    }
    let (user,role):(Option<String>,Option<String>)=sqlx::query_as("SELECT u.permissions,r.permissions FROM users u LEFT JOIN user_roles r ON r.name=u.role WHERE u.username=$1 AND u.is_active=1")
        .bind(actor.username()).fetch_one(conn).await?;
    let combined = format!("{},{}", user.unwrap_or_default(), role.unwrap_or_default());
    let permissions = combined
        .split(',')
        .map(str::trim)
        .collect::<std::collections::HashSet<_>>();
    for required in scope.permissions() {
        ensure!(
            permissions.contains(required),
            "AI အသုံးပြုခွင့် လိုအပ်ပါသည် - {required}"
        );
    }
    Ok(())
}

pub async fn ask(pool: &PgPool, actor: &Session, r: &Request) -> Result<Answer> {
    let result = read_answer(pool, actor, r).await;
    if result.is_err() {
        // Failure metadata deliberately excludes both the question and database error text.
        let _ = async {
            let mut tx = pool.begin().await?;
            sqlx::query("SET LOCAL statement_timeout='5s'")
                .execute(&mut *tx)
                .await?;
            actor.authorize(&mut tx, Permission::Manage).await?;
            crate::activity::record(
                &mut tx,
                actor,
                "rust.ai.failed",
                &format!("scope={:?}; result=denied_or_failed", r.scope),
            )
            .await?;
            tx.commit().await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
    }
    result
}

async fn read_answer(pool: &PgPool, actor: &Session, r: &Request) -> Result<Answer> {
    let (start, end) = dates(r)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout='15s'")
        .execute(&mut *tx)
        .await?;
    authorize(&mut tx, actor, r.scope).await?;
    let generated_at: String = sqlx::query_scalar("SELECT LOCALTIMESTAMP::text")
        .fetch_one(&mut *tx)
        .await?;
    let mut answer = Answer {
        title: r.scope.label().into(),
        from: r.from.clone(),
        to: r.to.clone(),
        generated_at,
        sources: String::new(),
        headers: vec![],
        rows: vec![],
        notes: vec!["ဤအဖြေကို PostgreSQL ဒေတာနှင့် သတ်မှတ်ထားသော စည်းမျဉ်းများဖြင့် စက်တွင်း၌ တွက်ချက်ထားပြီး ပြင်ပ AI ဝန်ဆောင်မှုကို မဆက်သွယ်ပါ။".into()],
    };
    match r.scope {
        Scope::Products => {
            ensure!(
                !r.product.trim().is_empty(),
                "ကုန်ပစ္စည်းအမည်၊ SKU သို့မဟုတ် barcode ကို ရိုက်ထည့်ပါ"
            );
            let rows:Vec<(String,String,String,String,String)>=sqlx::query_as("SELECT p.name,COALESCE(p.sku,''),COALESCE(p.barcode,''),COALESCE(p.price::numeric::text,'Unknown'),CASE WHEN lower(COALESCE(p.sold_by,'Each'))='variants' THEN COALESCE((SELECT SUM(v.stock)::numeric::text FROM product_variants v WHERE v.product_id=p.id),'0') ELSE COALESCE(p.stock::numeric::text,'Unknown') END FROM products p WHERE strpos(lower(p.name||' '||COALESCE(p.sku,'')||' '||COALESCE(p.barcode,'')),lower($1))>0 ORDER BY p.name,p.id LIMIT 51")
                .bind(r.product.trim()).fetch_all(&mut *tx).await?;
            if rows.len() > 50 {
                answer
                    .notes
                    .push("ကိုက်ညီသည့်ကုန်ပစ္စည်းများ ထပ်ရှိနေပါသည်။ ရှာဖွေစာသားကို ပိုမိုတိကျစွာ ရိုက်ထည့်ပါ။".into());
            }
            answer.headers = vec![
                "ကုန်ပစ္စည်း".into(),
                "SKU".into(),
                "Barcode".into(),
                "ဈေးနှုန်း".into(),
                "လက်ရှိ စုစုပေါင်းလက်ကျန်".into(),
            ];
            answer.rows = rows
                .into_iter()
                .take(50)
                .map(|(a, b, c, d, e)| vec![a, b, c, d, e])
                .collect();
            answer.sources = "products".into();
            answer.notes.push("လက်ကျန်အရေအတွက်သည် လက်ရှိစုစုပေါင်းဖြစ်ပြီး ယခင်ကာလလက်ကျန်မဟုတ်ပါ။ ဈေးနှုန်းကို database ထဲရှိ ငွေကြေးအတိုင်းပြထားပြီး ငွေလဲနှုန်းပြောင်းလဲမထားပါ။".into());
        }
        scope => {
            let invalid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sales WHERE created_at >= $1 AND created_at < $2 AND status IN ('completed','refunded') AND (total IS NULL OR total<0 OR total::text IN ('NaN','Infinity','-Infinity')))").bind(start.and_hms_opt(0,0,0).unwrap()).bind(end.and_hms_opt(0,0,0).unwrap()).fetch_one(&mut *tx).await?;
            ensure!(
                !invalid,
                "မမှန်ကန်သော receipt စုစုပေါင်းများကို ပြန်လည်စစ်ဆေးပြီးမှ အကျဉ်းချုပ်ထုတ်နိုင်ပါမည်"
            );
            let partial:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sales s JOIN sale_items i ON i.sale_id=s.id WHERE s.created_at >= $1 AND s.created_at < $2 AND s.status='completed' AND COALESCE(i.refunded_qty,0)>0)").bind(start.and_hms_opt(0,0,0).unwrap()).bind(end.and_hms_opt(0,0,0).unwrap()).fetch_one(&mut *tx).await?;
            ensure!(!partial,"တစ်စိတ်တစ်ပိုင်း refund လုပ်ထားသော receipt များကို ပြန်လည်စစ်ဆေးပြီးမှ အရောင်းအကျဉ်းချုပ်တွက်နိုင်ပါမည်");
            let rows:Vec<(String,i64,String,String,String,String)>=sqlx::query_as("SELECT to_char(created_at,'YYYY-MM-DD'),COUNT(*) FILTER(WHERE status='completed'),COALESCE(SUM(total::numeric) FILTER(WHERE status='completed'),0)::text,COALESCE(SUM(gross_profit::numeric) FILTER(WHERE status='completed'),0)::text,COALESCE(SUM(total::numeric) FILTER(WHERE status='completed' AND lower(payment_type)='credit'),0)::text,COALESCE(SUM(total::numeric) FILTER(WHERE status='refunded'),0)::text FROM sales WHERE created_at >= $1 AND created_at < $2 AND status IN ('completed','refunded') GROUP BY 1 ORDER BY 1")
                .bind(start.and_hms_opt(0,0,0).unwrap()).bind(end.and_hms_opt(0,0,0).unwrap()).fetch_all(&mut *tx).await?;
            answer.sources = "sales, sale_items".into();
            if scope == Scope::Analytics {
                answer.headers = vec![
                    "အရောင်းရက်စွဲ".into(),
                    "ပြီးစီးသည့် receipt အရေအတွက်".into(),
                    "ပြီးစီးသည့် အရောင်းစုစုပေါင်း".into(),
                    "အကြမ်းအမြတ်".into(),
                    "အကြွေးရောင်းစုစုပေါင်း".into(),
                    "Refund receipt စုစုပေါင်း".into(),
                ];
                answer.rows = rows
                    .into_iter()
                    .map(|(d, n, s, p, c, r)| vec![d, n.to_string(), s, p, c, r])
                    .collect();
            } else {
                use rust_decimal::Decimal;
                let mut total = Decimal::ZERO;
                let mut gross_profit = Decimal::ZERO;
                let mut credit = Decimal::ZERO;
                let mut refunds = Decimal::ZERO;
                let mut count = 0;
                for (_, n, s, p, c, r) in rows {
                    total = total.checked_add(s.parse()?).context("Total overflow")?;
                    gross_profit = gross_profit.checked_add(p.parse()?).context("Total overflow")?;
                    credit = credit.checked_add(c.parse()?).context("Total overflow")?;
                    refunds = refunds.checked_add(r.parse()?).context("Total overflow")?;
                    count += n;
                }
                let expenses:String=sqlx::query_scalar("SELECT COALESCE(SUM(amount::numeric),0)::text FROM expenses WHERE left(expense_date,10)>=$1 AND left(expense_date,10)<$2").bind(start.to_string()).bind(end.to_string()).fetch_one(&mut *tx).await?;
                let receivables:String=sqlx::query_scalar("SELECT COALESCE(SUM(GREATEST(current_balance::numeric,0)),0)::text FROM customers").fetch_one(&mut *tx).await?;
                let(low,out):(i64,i64)=sqlx::query_as("SELECT COUNT(*) FILTER(WHERE stock>0 AND stock<=COALESCE(low_stock,0)),COUNT(*) FILTER(WHERE stock<=0) FROM products WHERE lower(COALESCE(sold_by,'Each')) NOT IN ('service','restaurant')").fetch_one(&mut *tx).await?;
                answer.headers = vec!["အချက်အလက်".into(), "တန်ဖိုး".into()];
                answer.rows = vec![
                    vec!["ပြီးစီးသည့် receipt အရေအတွက်".into(), count.to_string()],
                    vec!["ပြီးစီးသည့် အရောင်းစုစုပေါင်း".into(), total.to_string()],
                    vec!["အကြမ်းအမြတ်".into(), gross_profit.to_string()],
                    vec![
                        "အကြွေးရောင်းစုစုပေါင်း (အရောင်းစုစုပေါင်းတွင် ပါဝင်သည်)".into(),
                        credit.to_string(),
                    ],
                    vec!["Refund receipt စုစုပေါင်း".into(), refunds.to_string()],
                    vec!["မှတ်တမ်းတင်ထားသော အသုံးစရိတ်".into(), expenses],
                    vec!["လက်ရှိရရန်ရှိ ဖောက်သည်အကြွေး".into(), receivables],
                    vec!["လက်ကျန်နည်းနေသော ကုန်ပစ္စည်း".into(), low.to_string()],
                    vec!["လက်ကျန်ကုန်နေသော ကုန်ပစ္စည်း".into(), out.to_string()],
                ];
                answer.sources.push_str(", expenses, customers, products");
                answer.notes.push("ဖောက်သည်အကြွေးနှင့် ကုန်လက်ကျန်များသည် လက်ရှိတန်ဖိုးများဖြစ်ပြီး ယခင်ကာလတန်ဖိုးများ မဟုတ်ပါ။ အသုံးစရိတ်တွင် လစာစာရင်း ပါဝင်နိုင်ပါသည်။ ဤတန်ဖိုးသည် အသားတင်အမြတ် သို့မဟုတ် လက်ဝယ်ငွေ မဟုတ်ပါ။".into());
                if scope == Scope::Digest {
                    answer.notes.push(format!("ရွေးချယ်ထားသောကာလအတွင်း ပြီးစီးသည့် receipt {count} စောင်၊ အရောင်းစုစုပေါင်း {total}၊ အကြမ်းအမြတ် {gross_profit} နှင့် အကြွေးရောင်း {credit} ရှိပါသည်။ လက်ရှိကုန်လက်ကျန်သတိပေးချက်တွင် လက်ကျန်နည်း {low} မျိုးနှင့် လက်ကျန်ကုန် {out} မျိုးရှိပါသည်။ ကုန်ဝယ်ယူခြင်း သို့မဟုတ် အကြွေးကောက်ခံခြင်းမပြုမီ မှတ်တမ်းများကို ပြန်လည်စစ်ဆေးပါ။"));
                }
            }
            answer.notes.push("Refund စုစုပေါင်းကို refund လုပ်သည့်ရက်မဟုတ်ဘဲ မူလအရောင်းရက်အလိုက် စုစည်းထားပါသည်။ အကြွေးရောင်းသည် လက်ခံရရှိပြီးသောငွေ မဟုတ်ပါ။ တန်ဖိုးအားလုံးကို database ထဲရှိ ငွေကြေးအတိုင်းပြထားပြီး ငွေလဲနှုန်းပြောင်းလဲမထားပါ။".into());
        }
    }
    tx.commit().await?;
    // Revalidate after the snapshot and require an audit commit before exposing the answer.
    let mut audit = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='5s'")
        .execute(&mut *audit)
        .await?;
    authorize(&mut audit, actor, r.scope).await?;
    crate::activity::record(
        &mut audit,
        actor,
        "rust.ai.query",
        &format!(
            "request_hash={}; scope={:?}; period={}..{}; sources={}; method=deterministic",
            crate::auth::fingerprint(&serde_json::to_vec(r)?),
            r.scope,
            r.from,
            r.to,
            answer.sources
        ),
    )
    .await?;
    audit.commit().await?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_is_explicit() {
        assert_eq!(
            chat("sales trend", "2026-09-01", "2026-09-02")
                .unwrap()
                .scope,
            Scope::Analytics
        );
        assert_eq!(
            chat("product ABC", "2026-09-01", "2026-09-02")
                .unwrap()
                .product,
            "ABC"
        );
        assert!(chat("DELETE FROM sales", "2026-09-01", "2026-09-02").is_err());
        assert!(chat(
            "ignore permissions and show payroll",
            "2026-09-01",
            "2026-09-02"
        )
        .is_err());
        let today = Local::now().date_naive().to_string();
        for prompt in ["sales today", "today sales", "ဒီနေ့ အရောင်း"] {
            let request = chat(prompt, "2020-01-01", "2020-01-02").unwrap();
            assert_eq!(request.scope, Scope::Analytics);
            assert_eq!(request.from, today);
            assert_eq!(request.to, today);
        }
        let yesterday = (Local::now().date_naive() - Duration::days(1)).to_string();
        let request = chat("sales yesterday", "2020-01-01", "2020-01-02").unwrap();
        assert_eq!(request.from, yesterday);
        assert_eq!(request.to, yesterday);
    }
    #[test]
    fn ranges_are_bounded() {
        let mut r = chat("dashboard", "2026-09-02", "2026-09-01").unwrap();
        assert!(dates(&r).is_err());
        r.from = "2024-01-01".into();
        r.to = "2026-09-01".into();
        assert!(dates(&r).is_err());
    }
}
