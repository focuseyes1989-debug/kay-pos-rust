use crate::{ai::{self, Answer, Request, Scope}, auth::Session};
use anyhow::{ensure, Context, Result};
use chrono::{Datelike, Duration, NaiveDate};
use rust_decimal::Decimal;
use sqlx::PgPool;

#[derive(Clone, Debug, PartialEq)]
pub struct Plan { pub request: Request, pub comparison: Option<Request> }

fn normalized(value: &str) -> String {
    value.to_lowercase().replace("\u{1037}\u{103a}","\u{103a}\u{1037}").chars()
        .filter(|c| !c.is_whitespace() && !matches!(c,'\u{200b}'|'\u{200c}'|'\u{200d}'|'\u{2019}'|'\'' )).collect()
}

pub fn plan(prompt: &str, previous: Option<&Request>, today: NaiveDate) -> Result<Plan> {
    let canonical=ai::canonical_periods(prompt,today)?;
    let prompt=canonical.as_str();
    let compare=["compare","နှိုင်းယှဉ်","နှိုင်းယှဥ်"].iter().any(|s|prompt.to_lowercase().contains(s));
    let mut text=prompt.to_lowercase();
    let top_expense=prompt.contains("အများဆုံး") && ["expense","အသုံးစရိတ်","ကုန်ကျစရိတ်"].iter().any(|s|prompt.contains(s));
    if top_expense {text="expenses".into();for period in ["ဒီနေ့","မနေ့က","ဒီလ","ပြီးခဲ့တဲ့လ","ဒီအပတ်","ပြီးခဲ့တဲ့အပတ်","this year","last year"] {if prompt.contains(period){text.push(' ');text.push_str(period);}}}
    for word in ["compare","နှိုင်းယှဉ်ပေးပါ","နှိုင်းယှဉ်","နှိုင်းယှဥ်ပေးပါ","နှိုင်းယှဥ်"] {text=text.replace(word,"");}
    let period_text=text.trim().trim_end_matches(['?','။']).trim();
    let period_text=period_text.strip_suffix("ရော").unwrap_or(period_text).trim();
    let mut request=if ai::is_period_followup(period_text) {
        let prior=previous.context("အရင်မေးခွန်း မရှိသေးပါ။ ဘာအကြောင်း မေးချင်တာလဲ ထည့်ရေးပါ။")?;
        let mut value=prior.clone();
        let period=ai::chat_at(&format!("sales {period_text}"),&prior.from,&prior.to,today)?;
        value.from=period.from;value.to=period.to;value
    } else if ["အဲဒီ","အဲ့ဒီ","those","အဲဒါ"].iter().any(|s|text.contains(s)) {
        let prior=previous.context("ဘယ်အချက်အလက်ကို ဆိုလိုတာလဲ ပြန်ရေးပေးပါ။")?;
        let mut value=prior.clone();
        if ["receipt","Receipt","ဘောင်ချာ"].iter().any(|s|text.contains(s)) {
            ensure!(matches!(prior.scope,Scope::CategorySales|Scope::Receipts|Scope::Analytics),"ဤအဖြေမှ receipt သို့ မပြောင်းနိုင်ပါ။ Category သို့မဟုတ် ရက်စွဲကို ထည့်မေးပါ။");
            value.scope=Scope::Receipts;
            if prior.scope==Scope::CategorySales {value.receipt_category=prior.product.clone();value.product.clear();}
        }
        value
    } else {
        if compare {
            for connector in ["နဲ့ ပြီးခဲ့တဲ့လ","နဲ့ပြီးခဲ့တဲ့လ","နှင့် ပြီးခဲ့တဲ့လ","နဲ့ မနေ့က","နဲ့မနေ့က","နှင့် မနေ့က","and last month","vs last month","and yesterday","vs yesterday","နဲ့ yesterday","နဲ့yesterday","နှင့် yesterday","နဲ့ last year","နဲ့last year","နှင့် last year","and last year","vs last year"] {text=text.replace(connector,"");}
        }
        ai::chat_at(text.trim(),&today.to_string(),&today.to_string(),today)?
    };
    if top_expense {let period=ai::chat_at(&format!("sales {prompt}"),&today.to_string(),&today.to_string(),today)?;request.from=period.from;request.to=period.to;}
    if compare && request.scope==Scope::CategorySales && request.product.trim().is_empty() {request.scope=Scope::Analytics;}
    let comparison=if compare {
        ensure!(!prompt.contains("profit")&&!prompt.contains("အမြတ်"),"အမြတ်နှိုင်းယှဉ်မှုကို မပံ့ပိုးသေးပါ။ အရောင်း သို့မဟုတ် အသုံးစရိတ်ကို နှိုင်းယှဉ်မေးပါ။");
        ensure!(matches!(request.scope,Scope::Analytics|Scope::CategorySales|Scope::Expenses),"အရောင်း၊ Category ရောင်းရငွေနှင့် အသုံးစရိတ်ကိုသာ ကာလနှိုင်းယှဉ်နိုင်ပါသည်။");
        let a=NaiveDate::parse_from_str(&request.from,"%Y-%m-%d")?;
        let b=NaiveDate::parse_from_str(&request.to,"%Y-%m-%d")?;
        let (start,end)=if prompt.contains("ပြီးခဲ့တဲ့လ") || prompt.contains("last month") {
            let end=a.with_day(1).context("Invalid month")?-Duration::days(1);
            (end.with_day(1).context("Invalid month")?,end)
        } else if prompt.contains("last year") {
            let year=a.year()-1;
            (NaiveDate::from_ymd_opt(year,1,1).context("Invalid year")?,NaiveDate::from_ymd_opt(year,12,31).context("Invalid year")?)
        } else {let length=(b-a).num_days()+1;(a-Duration::days(length),a-Duration::days(1))};
        let mut value=request.clone();value.from=start.to_string();value.to=end.to_string();Some(value)
    } else {None};
    Ok(Plan {request,comparison})
}

fn distance(a:&str,b:&str)->usize {
    let b:Vec<_>=b.chars().collect();let mut row:Vec<_>=(0..=b.len()).collect();
    for (i,c) in a.chars().enumerate() {let mut next=vec![i+1];for (j,d) in b.iter().enumerate(){next.push((row[j+1]+1).min(next[j]+1).min(row[j]+usize::from(c!=*d)));}row=next;}
    row[b.len()]
}

/// Approximate matches are offered to the user, never silently substituted.
pub async fn candidates(pool:&PgPool,actor:&Session,r:&Request)->Result<Vec<String>> {
    let term=match r.scope {Scope::CategorySales=>&r.product,Scope::Receipts=>&r.receipt_category,Scope::Products|Scope::Customers|Scope::Credit|Scope::Employees=>&r.product,_=>return Ok(vec![])};
    if term.is_empty(){return Ok(vec![]);}
    let mut tx=pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;
    sqlx::query("SET LOCAL statement_timeout='5s'").execute(&mut *tx).await?;
    ai::authorize(&mut tx,actor,r.scope).await?;
    let sql=match r.scope {Scope::Products=>"SELECT DISTINCT name FROM products ORDER BY name LIMIT 1000",Scope::Customers|Scope::Credit=>"SELECT DISTINCT name FROM customers ORDER BY name LIMIT 1000",Scope::Employees=>"SELECT DISTINCT full_name FROM employees ORDER BY full_name LIMIT 1000",_=>"SELECT DISTINCT name FROM categories ORDER BY name LIMIT 1000"};
    let names:Vec<String>=sqlx::query_scalar(sql).fetch_all(&mut *tx).await?;
    let q=normalized(term);
    if let Some(name)=names.iter().find(|n|normalized(n)==q) {
        let choices=if name.to_lowercase().trim()==term.to_lowercase().trim(){vec![]}else{vec![name.clone()]};
        tx.commit().await?;return Ok(choices);
    }
    if matches!(r.scope,Scope::CategorySales|Scope::Receipts) && q=="service" && names.iter().any(|n|normalized(n)=="services") {tx.commit().await?;return Ok(vec![]);}
    if r.scope==Scope::Products {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE strpos(lower(name||' '||COALESCE(sku,'')||' '||COALESCE(barcode,'')),lower($1))>0)").bind(term).fetch_one(&mut *tx).await?;
        if exists {tx.commit().await?;return Ok(vec![]);}
    }
    if matches!(r.scope,Scope::Customers|Scope::Credit|Scope::Employees) {
        let sql=if r.scope==Scope::Employees {"SELECT EXISTS(SELECT 1 FROM employees WHERE strpos(lower(full_name||' '||employee_no||' '||COALESCE(phone,'')),lower($1))>0)"}else{"SELECT EXISTS(SELECT 1 FROM customers WHERE strpos(lower(name||' '||COALESCE(phone,'')||' '||id::text),lower($1))>0)"};
        let exists:bool=sqlx::query_scalar(sql).bind(term).fetch_one(&mut *tx).await?;
        if exists {tx.commit().await?;return Ok(vec![]);}
    }
    tx.commit().await?;
    let mut choices:Vec<_>=names.into_iter().filter_map(|name|{let n=normalized(&name);let d=distance(&q,&n);if n.contains(&q)||q.contains(&n)||d<=((q.chars().count()/4).max(1).min(3)){Some((d,name))}else{None}}).collect();
    choices.sort();Ok(choices.into_iter().take(5).map(|(_,n)|n).collect())
}

pub fn choose(mut plan:Plan,name:&str)->Plan {
    for r in std::iter::once(&mut plan.request).chain(plan.comparison.iter_mut()) {
        if r.scope==Scope::Receipts {r.receipt_category=name.into();} else {r.product=name.into();}
    }
    plan
}

pub async fn answer(pool:&PgPool,actor:&Session,plan:&Plan)->Result<Answer> {
    let mut answer=ai::ask(pool,actor,&plan.request).await?;
    if plan.request.scope==Scope::Expenses {
        if let Some(row)=answer.rows.first(){if let (Some(name),Some(amount))=(row.first(),row.get(2)){answer.narrative.push_str(&format!(" အများဆုံး category က {name} ဖြစ်ပြီး {amount} ရှိပါတယ်။"));}}
    }
    answer.notes.insert(0,format!("ရက်စွဲ {} မှ {} အထိ (နှစ်ဖက်စလုံးပါဝင်)။ ရှာဖွေစာသား: {}။ Receipt category: {}။ Payment: {}။",plan.request.from,plan.request.to,plan.request.product,plan.request.receipt_category,plan.request.receipt_payment));
    if let Some(previous)=&plan.comparison {
        let other=ai::ask(pool,actor,previous).await?;
        let a:Decimal=answer.total_amount.as_deref().context("Comparison total unavailable")?.parse()?;
        let b:Decimal=other.total_amount.as_deref().context("Comparison total unavailable")?.parse()?;
        let change=a.checked_sub(b).context("Comparison overflow")?;
        let percent=if b==Decimal::ZERO {"ယခင်ကာလ 0 ဖြစ်သောကြောင့် ရာခိုင်နှုန်း မတွက်နိုင်ပါ".into()}else{format!("{}%",change.checked_div(b).and_then(|v|v.checked_mul(Decimal::from(100))).context("Comparison percentage overflow")?.round_dp(1).normalize())};
        answer.title=format!("{} · ကာလနှစ်ခု နှိုင်းယှဉ်ချက်",plan.request.scope.label());
        answer.narrative=format!("လက်ရှိကာလ {a}၊ ယခင်ကာလ {b}။ ကွာခြားချက် {change} ({percent})။");
        answer.headers=vec!["ကာလ".into(),"ရက်အပိုင်းအခြား".into(),"ငွေပမာဏ".into()];
        answer.rows=vec![vec!["လက်ရှိကာလ".into(),format!("{} – {}",answer.from,answer.to),a.to_string()],vec!["ယခင်ကာလ".into(),format!("{} – {}",other.from,other.to),b.to_string()]];
        answer.notes.push(format!("ယခင်ကာလ {} – {}။ ကာလနှစ်ခု၏ ရက်အရေအတွက် မတူနိုင်ပါ။ ကွာခြားချက် = လက်ရှိ − ယခင်; ရာခိုင်နှုန်း = ကွာခြားချက် / ယခင် × 100။",other.from,other.to));
        answer.notes.extend(other.notes);answer.sources="comparison".into();
    }
    if answer.rows.is_empty() && answer.narrative.is_empty(){answer.narrative="သတ်မှတ်ချက်နှင့် ကိုက်ညီသည့် data မရှိပါ။ ရက်စွဲ သို့မဟုတ် အမည်ကို ပြန်စစ်ကြည့်ပါ။".into();}
    Ok(answer)
}

pub async fn detail(pool:&PgPool,actor:&Session,r:&Request,row:&[String])->Result<Answer> {
    ensure!(matches!(r.scope,Scope::Products|Scope::Inventory|Scope::Customers|Scope::Credit|Scope::Employees|Scope::Expenses),"Detail မပံ့ပိုးသော အကြောင်းအရာပါ။");
    let mut tx=pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;
    sqlx::query("SET LOCAL statement_timeout='5s'").execute(&mut *tx).await?;
    ai::authorize(&mut tx,actor,r.scope).await?;
    let name=row.first().context("Missing detail key")?;
    let (headers,sql)=match r.scope {
        Scope::Products|Scope::Inventory=>(vec!["ID","Product","SKU","Barcode","Price","Stock","Category"],"SELECT json_build_array(p.id::text,p.name,COALESCE(p.sku,''),COALESCE(p.barcode,''),p.price::numeric::text,CASE WHEN lower(COALESCE(p.sold_by,'Each'))='variants' THEN COALESCE((SELECT SUM(stock)::numeric::text FROM product_variants WHERE product_id=p.id),'0') ELSE p.stock::numeric::text END,COALESCE(c.name,''))::text FROM products p LEFT JOIN categories c ON c.id=p.category_id WHERE p.name=$1 AND COALESCE(p.sku,'')=$2 ORDER BY p.id LIMIT 50"),
        Scope::Customers=>(vec!["ID","Customer","Phone","Email","Address","Total spent"],"SELECT json_build_array(id::text,name,COALESCE(phone,''),COALESCE(email,''),COALESCE(address,''),total_spent::numeric::text)::text FROM customers WHERE name=$1 AND COALESCE(phone,'')=$2 ORDER BY id LIMIT 50"),
        Scope::Employees=>(vec!["ID","Employee ID","Name","Phone","Position","Department","Branch","Hire date","Status","Address","Notes"],"SELECT json_build_array(id::text,employee_no,full_name,COALESCE(phone,''),COALESCE(position,''),COALESCE(department,''),COALESCE(branch,''),COALESCE(hire_date::text,''),COALESCE(employment_status,''),COALESCE(address,''),COALESCE(notes,''))::text FROM employees WHERE employee_no=$1 AND full_name=$2 ORDER BY id LIMIT 50"),
        Scope::Credit=>(vec!["ID","Customer","Phone","Email","Address","Current balance","Total spent"],"SELECT json_build_array(id::text,name,COALESCE(phone,''),COALESCE(email,''),COALESCE(address,''),current_balance::numeric::text,total_spent::numeric::text)::text FROM customers WHERE name=$1 AND COALESCE(phone,'')=$2 ORDER BY id LIMIT 50"),
        _=>(vec!["Expense","Date","Category","Description","Amount","Payment","Reference","Notes"],"SELECT json_build_array(expense_no,expense_date,category,description,amount::numeric::text,payment_method,reference_no,notes)::text FROM expenses WHERE category=$1 AND left(expense_date,10)>=$2 AND left(expense_date,10)<=$3 AND strpos(lower(COALESCE(category,'')||' '||COALESCE(description,'')),lower($4))>0 ORDER BY expense_date DESC,id DESC LIMIT 51"),
    };
    let mut query=sqlx::query_scalar::<_,String>(sql).bind(name);
    if r.scope==Scope::Expenses {query=query.bind(&r.from).bind(&r.to).bind(&r.product);} else {query=query.bind(row.get(1).context("Missing detail key")?);}
    let records=query.fetch_all(&mut *tx).await?;tx.commit().await?;
    let mut audit=pool.begin().await?;
    ai::authorize(&mut audit,actor,r.scope).await?;
    crate::activity::record(&mut audit,actor,"rust.ai.detail",&format!("scope={:?}; key_hash={}",r.scope,crate::auth::fingerprint(&serde_json::to_vec(row)?))).await?;
    audit.commit().await?;
    let rows=records.iter().take(50).map(|s|{let values:Vec<serde_json::Value>=serde_json::from_str(s)?;Ok(values.iter().map(|v|v.as_str().unwrap_or("").to_owned()).collect())}).collect::<Result<Vec<_>>>()?;
    Ok(Answer {total_amount:None,receipt_ids:vec![],narrative:if records.len()>50{"ပထမ 50 မှတ်တမ်းသာ ပြထားသည်။".into()}else{"လက်ရှိ database မှ အသေးစိတ်အချက်အလက်။ တူညီသောအမည်ရှိပါက ID အလိုက် စစ်ကြည့်ပါ။".into()},title:name.clone(),from:r.from.clone(),to:r.to.clone(),generated_at:chrono::Local::now().to_string(),sources:"detail".into(),headers:headers.into_iter().map(String::from).collect(),rows,notes:vec![]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversation_keeps_category_and_switches_to_receipts() {
        let today=NaiveDate::from_ymd_opt(2026,10,5).unwrap();
        let first=plan("today Service sales",None,today).unwrap();
        let next=plan("မနေ့ကရော",Some(&first.request),today).unwrap();
        assert_eq!(next.request.product,first.request.product);
        let receipts=plan("အဲဒီ Receipt တွေပြပါ",Some(&next.request),today).unwrap();
        assert_eq!(receipts.request.scope,Scope::Receipts);assert_eq!(receipts.request.receipt_category,first.request.product);assert_eq!(receipts.request.from,next.request.from);
    }
    #[test]
    fn comparisons_keep_filters_and_have_previous_month() {
        let today=NaiveDate::from_ymd_opt(2026,10,5).unwrap();
        let value=plan("ဒီလနဲ့ ပြီးခဲ့တဲ့လ အရောင်း နှိုင်းယှဉ်ပေးပါ",None,today).unwrap();
        assert!(matches!(value.request.scope,Scope::Analytics));
        assert!(value.comparison.is_some());
        assert_eq!(distance("kitkat","kitkat"),0);assert_eq!(distance("kitkat","kit kat"),1);
    }
    #[test]
    fn amount_payment_and_category_filters_survive_followups() {
        let today=NaiveDate::from_ymd_opt(2026,10,5).unwrap();
        let previous=Request {scope:Scope::Receipts,from:"2026-10-05".into(),to:"2026-10-05".into(),product:"amount:10000".into(),receipt_category:"Services".into(),receipt_payment:"kpay".into()};
        let next=plan("မနေ့ကရော?",Some(&previous),today).unwrap();
        assert_eq!(next.request.product,previous.product);assert_eq!(next.request.receipt_category,previous.receipt_category);assert_eq!(next.request.receipt_payment,previous.receipt_payment);
        assert!(plan("today",None,today).is_err());
        assert!(plan("အဲဒီ Receipt တွေပြပါ",None,today).is_err());
        let selected=choose(Plan {request:previous.clone(),comparison:Some(previous)},"Other");
        assert_eq!(selected.request.receipt_category,"Other");assert_eq!(selected.comparison.unwrap().receipt_category,"Other");
    }
    #[test]
    fn unsupported_comparisons_and_sql_commands_do_not_route() {
        let today=NaiveDate::from_ymd_opt(2026,10,5).unwrap();
        assert!(plan("compare products",None,today).is_err());
        assert!(plan("compare profit this month",None,today).is_err());
        assert!(plan("SELECT * FROM sales",None,today).is_err());
        assert_eq!(normalized("ရေခဲမုန့်"),normalized("ရေခဲမုန့်"));
    }
}
