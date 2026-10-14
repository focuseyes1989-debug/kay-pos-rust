use crate::storage as sqlx;
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
    let canonical=ai::canonical_periods(&ai::canonical_topics(prompt),today)?;
    let prompt=canonical.as_str();
    let compare=["compare","comparison","နှိုင်းယှဉ်","နှိုင်းယှဥ်","နှိုင်းယှင်","ယှဉ်ကြည့်","ယှဉ်ကြည့်"].iter().any(|s|prompt.to_lowercase().contains(s));
    if compare {return comparison_plan(prompt,previous,today);}
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
            ensure!(matches!(prior.scope,Scope::CategorySales|Scope::Receipts|Scope::Analytics|Scope::PaymentSales),"ဤအဖြေမှ receipt သို့ မပြောင်းနိုင်ပါ။ Category သို့မဟုတ် ရက်စွဲကို ထည့်မေးပါ။");
            value.scope=Scope::Receipts;
            if prior.scope==Scope::CategorySales {value.receipt_category=prior.product.clone();value.product.clear();}
        }
        value
    } else {
        ai::chat_at(text.trim(),&today.to_string(),&today.to_string(),today)?
    };
    if top_expense {let period=ai::chat_at(&format!("sales {prompt}"),&today.to_string(),&today.to_string(),today)?;request.from=period.from;request.to=period.to;}
    let comparison=None;
    Ok(Plan {request,comparison})
}

// Parse both periods before routing the subject so comparison words cannot become category filters.
fn comparison_plan(prompt:&str,previous:Option<&Request>,today:NaiveDate)->Result<Plan> {
    let lower=prompt.to_lowercase();
    let explicit=ai::prompt_dates(&lower)?;
    ensure!(matches!(explicit.len(),0|1|2|4),"ကာလနှစ်ခုအတွက် စရက်/ဆုံးရက် လေးရက် သတ်မှတ်ပါ။");
    let same_month=["same month last year","same month of last year","မနှစ်ကတူညီတဲ့လ","မနှစ်က တူညီတဲ့လ","မနှစ်ကဒီလ","မနှစ်က ဒီလ","last year ဒီလ"].iter().any(|v|lower.contains(v));
    let mut subject=lower.clone();
    for word in ["same month of last year","same month last year","current month","previous month","comparison","compare","နှိုင်းယှဉ်ပေးပါ","နှိုင်းယှဥ်ပေးပါ","နှိုင်းယှင်ပေးပါ","နှိုင်းယှဉ်ချက်","နှိုင်းယှဉ်","နှိုင်းယှဥ်","နှိုင်းယှင်","ယှဉ်ကြည့်ပေးပါ","ယှဉ်ကြည့်ပေးပါ","ယှဉ်ကြည့်","ယှဉ်ကြည့်","မနှစ်ကတူညီတဲ့လ","မနှစ်က တူညီတဲ့လ","မနှစ်ကဒီလ","မနှစ်က ဒီလ","ပြီးခဲ့တဲ့လ","ပြီးခဲ့တဲ့အပတ်","this month","last month","this week","last week","this year","last year","yesterday","today","ဒီနေ့","ဒီအပတ်","ဒီလ","နဲ့","နှင့်","နှင့်"," vs "," versus "," and "," မှ "," အထိ "," to "," from "] {subject=subject.replace(word," ");}
    for part in lower.split(|c:char| !(c.is_ascii_digit()||matches!(c,'-'|'.'|'/'))) {
        if part.matches('-').count()==2||part.matches('.').count()==2||part.matches('/').count()==2 {subject=subject.replace(part," ");}
    }
    let subject=subject.trim().trim_matches(['?','။']).trim();
    let mut request=if subject.is_empty(){previous.context("အရောင်း သို့မဟုတ် အသုံးစရိတ်ကို ထည့်မေးပါ။")?.clone()}else{ai::chat_at(subject,&today.to_string(),&today.to_string(),today)?};
    if request.scope==Scope::CategorySales && request.product.is_empty(){request.scope=Scope::Analytics;}
    ensure!(!request.best_sellers && !lower.contains("profit") && !lower.contains("အမြတ်"),"အရောင်း သို့မဟုတ် အသုံးစရိတ်ကို ကာလနှစ်ခု နှိုင်းယှဉ်မေးပါ။");
    ensure!(matches!(request.scope,Scope::Analytics|Scope::CategorySales|Scope::Expenses),"အရောင်း၊ Category ရောင်းရငွေနှင့် အသုံးစရိတ်ကိုသာ ကာလနှိုင်းယှဉ်နိုင်ပါသည်။");
    let month=|day:NaiveDate|->Result<(NaiveDate,NaiveDate)>{let start=day.with_day(1).context("Invalid month")?;let next=if day.month()==12{NaiveDate::from_ymd_opt(day.year()+1,1,1)}else{NaiveDate::from_ymd_opt(day.year(),day.month()+1,1)}.context("Invalid month")?;Ok((start,next-Duration::days(1)))};
    let (a,b)=match explicit.as_slice(){[a,b,..]=>(*a,*b),[a]=>(*a,*a),_=>{
        if same_month||lower.contains("month")||lower.contains("ဒီလ")||lower.contains("ပြီးခဲ့တဲ့လ"){month(today)?}
        else {let period=if lower.contains("today")||lower.contains("ဒီနေ့"){"today"}else if lower.contains("this week")||lower.contains("ဒီအပတ်"){"this week"}else if lower.contains("this year"){"this year"}else{lower.as_str()};let r=ai::chat_at(&format!("sales {period}"),&request.from,&request.to,today)?;(NaiveDate::parse_from_str(&r.from,"%Y-%m-%d")?,NaiveDate::parse_from_str(&r.to,"%Y-%m-%d")?)}
    }};
    let (c,d)=if explicit.len()==4{(explicit[2],explicit[3])}
        else if same_month {month(NaiveDate::from_ymd_opt(a.year()-1,a.month(),1).context("Invalid year")?)?}
        else if lower.contains("month")||lower.contains("ပြီးခဲ့တဲ့လ")||lower.contains("ဒီလ"){month(a.with_day(1).context("Invalid month")?-Duration::days(1))?}
        else if lower.contains("last year"){(NaiveDate::from_ymd_opt(a.year()-1,1,1).context("Invalid year")?,NaiveDate::from_ymd_opt(a.year()-1,12,31).context("Invalid year")?)}
        else {let length=(b-a).num_days()+1;(a-Duration::days(length),a-Duration::days(1))};
    for (start,end) in [(a,b),(c,d)] {ensure!(start<=end&&(end-start).num_days()<366,"ရက်အပိုင်းအခြားကို စရက်မှဆုံးရက်အထိ 366 ရက်အတွင်း ရွေးပါ။");}
    request.from=a.to_string();request.to=b.to_string();
    let mut other=request.clone();other.from=c.to_string();other.to=d.to_string();
    Ok(Plan{request,comparison:Some(other)})
}

fn distance(a:&str,b:&str)->usize {
    let b:Vec<_>=b.chars().collect();let mut row:Vec<_>=(0..=b.len()).collect();
    for (i,c) in a.chars().enumerate() {let mut next=vec![i+1];for (j,d) in b.iter().enumerate(){next.push((row[j+1]+1).min(next[j]+1).min(row[j]+usize::from(c!=*d)));}row=next;}
    row[b.len()]
}

/// Approximate matches are offered to the user, never silently substituted.
pub async fn candidates(pool:&PgPool,actor:&Session,r:&Request)->Result<Vec<String>> {
    if r.scope==Scope::Receipts {
        let name=r.receipt_category.trim_start_matches("product:");
        if !name.is_empty() {
            let mut tx=pool.begin().await?;sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;ai::authorize(&mut tx,actor,r.scope).await?;
            let names:Vec<String>=sqlx::query_scalar("SELECT name FROM products UNION SELECT product_name AS name FROM sale_items WHERE product_name IS NOT NULL").fetch_all(&mut *tx).await?;
            tx.commit().await?;
            if names.iter().any(|n|normalized(n)==normalized(name)) {return Ok(vec![]);}
            if r.receipt_category.starts_with("product:") {
                let q=normalized(name);let mut choices:Vec<_>=names.into_iter().filter_map(|n|{let norm=normalized(&n);let d=distance(&q,&norm);(norm.contains(&q)||d<=((q.chars().count()/4).max(1).min(3))).then_some((d,n))}).collect();
                choices.sort();choices.dedup();return Ok(choices.into_iter().take(5).map(|(_,n)|n).collect());
            }

        }
    }
    let term=match r.scope {Scope::CategorySales=>&r.product,Scope::Receipts=>&r.receipt_category,Scope::Products|Scope::Suppliers|Scope::Customers|Scope::Credit|Scope::Employees=>&r.product,_=>return Ok(vec![])};
    if term.is_empty(){return Ok(vec![]);}
    let mut tx=pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;
    sqlx::query("SET LOCAL statement_timeout='5s'").execute(&mut *tx).await?;
    ai::authorize(&mut tx,actor,r.scope).await?;
    let sql=match r.scope {Scope::Products=>"SELECT DISTINCT name FROM products ORDER BY name LIMIT 1000",Scope::Customers|Scope::Credit=>"SELECT DISTINCT name FROM customers ORDER BY name LIMIT 1000",Scope::Suppliers=>"SELECT DISTINCT name FROM suppliers ORDER BY name LIMIT 1000",Scope::Employees=>"SELECT DISTINCT full_name FROM employees ORDER BY full_name LIMIT 1000",_=>"SELECT DISTINCT name FROM categories ORDER BY name LIMIT 1000"};
    let names:Vec<String>=sqlx::query_scalar(sql).fetch_all(&mut *tx).await?;
    let q=normalized(term);
    if let Some(name)=names.iter().find(|n|normalized(n)==q) {
        let choices=if name.to_lowercase().trim()==term.to_lowercase().trim(){vec![]}else{vec![name.clone()]};
        tx.commit().await?;return Ok(choices);
    }
    if matches!(r.scope,Scope::CategorySales|Scope::Receipts) && q=="service" && names.iter().any(|n|normalized(n)=="services") {tx.commit().await?;return Ok(vec![]);}
    if r.scope==Scope::CategorySales {
        // A named sales subject can be a product, not just a category.
        // Exact product matches must win over approximate category suggestions.
        ai::authorize(&mut tx,actor,Scope::Products).await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE lower(trim(name))=lower(trim($1)) OR lower(trim(sku))=lower(trim($1)) OR lower(trim(barcode))=lower(trim($1)))").bind(term.trim()).fetch_one(&mut *tx).await?;
        if exists {tx.commit().await?;return Ok(vec![]);}
    }
    if r.scope==Scope::Products {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE strpos(lower(name||' '||COALESCE(sku,'')||' '||COALESCE(barcode,'')),lower($1))>0)").bind(term).fetch_one(&mut *tx).await?;
        if exists {tx.commit().await?;return Ok(vec![]);}
    }
    if matches!(r.scope,Scope::Suppliers|Scope::Customers|Scope::Credit|Scope::Employees) {
        let sql=if r.scope==Scope::Suppliers {"SELECT EXISTS(SELECT 1 FROM suppliers WHERE strpos(lower(name||' '||COALESCE(phone,'')||' '||COALESCE(company_name,'')||' '||COALESCE(contact_person,'')||' '||id::text),lower($1))>0)"}else if r.scope==Scope::Employees {"SELECT EXISTS(SELECT 1 FROM employees WHERE strpos(lower(full_name||' '||employee_no||' '||COALESCE(phone,'')),lower($1))>0)"}else{"SELECT EXISTS(SELECT 1 FROM customers WHERE strpos(lower(name||' '||COALESCE(phone,'')||' '||id::text),lower($1))>0)"};
        let exists:bool=sqlx::query_scalar(sql).bind(term).fetch_one(&mut *tx).await?;
        if exists {tx.commit().await?;return Ok(vec![]);}
    }
    tx.commit().await?;
    let mut choices:Vec<_>=names.into_iter().filter_map(|name|{let n=normalized(&name);let d=distance(&q,&n);if n.contains(&q)||q.contains(&n)||d<=((q.chars().count()/4).max(1).min(3)){Some((d,name))}else{None}}).collect();
    choices.sort();Ok(choices.into_iter().take(5).map(|(_,n)|n).collect())
}

pub fn choose(mut plan:Plan,name:&str)->Plan {
    for r in std::iter::once(&mut plan.request).chain(plan.comparison.iter_mut()) {
        if r.scope==Scope::Receipts {r.receipt_category=if r.receipt_category.starts_with("product:"){format!("product:{name}")}else{name.into()};} else {r.product=name.into();}
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
    ensure!(matches!(r.scope,Scope::Products|Scope::Inventory|Scope::Customers|Scope::Credit|Scope::Employees|Scope::Suppliers|Scope::Expenses),"Detail မပံ့ပိုးသော အကြောင်းအရာပါ။");
    let mut tx=pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY").execute(&mut *tx).await?;
    sqlx::query("SET LOCAL statement_timeout='5s'").execute(&mut *tx).await?;
    ai::authorize(&mut tx,actor,r.scope).await?;
    let name=row.first().context("Missing detail key")?;
    let (headers,sql)=match r.scope {
        Scope::Products|Scope::Inventory=>(vec!["ID","Product","SKU","Barcode","Price","Stock","Category"],"SELECT json_build_array(p.id::text,p.name,COALESCE(p.sku,''),COALESCE(p.barcode,''),p.price::numeric::text,CASE WHEN lower(COALESCE(p.sold_by,'Each'))='variants' THEN COALESCE((SELECT SUM(stock)::numeric::text FROM product_variants WHERE product_id=p.id),'0') ELSE p.stock::numeric::text END,COALESCE(c.name,''))::text FROM products p LEFT JOIN categories c ON c.id=p.category_id WHERE p.name=$1 AND COALESCE(p.sku,'')=$2 ORDER BY p.id LIMIT 50"),
        Scope::Suppliers=>(vec!["ID","Supplier","Phone","Company","Contact person","Email","Address","Website","Payment terms","Status"],"SELECT json_build_array(id::text,name,COALESCE(phone,''),COALESCE(company_name,''),COALESCE(contact_person,''),COALESCE(email,''),COALESCE(address,''),COALESCE(website,''),COALESCE(payment_terms,''),COALESCE(status,'Active'))::text FROM suppliers WHERE name=$1 AND COALESCE(phone,'')=$2 ORDER BY id LIMIT 50"),
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
        let previous=Request {best_sellers:false,best_seller_limit:None,scope:Scope::Receipts,from:"2026-10-05".into(),to:"2026-10-05".into(),product:"amount:10000".into(),receipt_category:"Services".into(),receipt_payment:"kpay".into()};
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

#[cfg(test)]
mod payment_followup_tests {
 use super::*;
 #[test]
 fn payment_method_survives_dates_and_receipt_followup() {
  let today=NaiveDate::from_ymd_opt(2026,10,6).unwrap();
  let first=plan("ဒီနေ့ Kpay နဲ့ ဘယ်လောက်ရောင်းရလဲ?",None,today).unwrap();
  let next=plan("မနေ့ကရော",Some(&first.request),today).unwrap();
  assert_eq!(next.request.scope,Scope::PaymentSales);assert_eq!(next.request.receipt_payment,"KPay");assert_eq!(next.request.from,"2026-10-05");
  let receipts=plan("အဲဒီ receipts ပြပါ",Some(&next.request),today).unwrap();
  assert_eq!(receipts.request.scope,Scope::Receipts);assert_eq!(receipts.request.receipt_payment,"KPay");assert_eq!(receipts.request.from,next.request.from);
 }
}

#[cfg(test)]
mod topic_alias_tests {
 use super::*;
 #[test] fn aliases_work_in_conversation_planning() {
  let today=NaiveDate::from_ymd_opt(2026,10,6).unwrap();
  assert_eq!(plan("ဒီနေ့ ဝင်ငွေ ပြပါ",None,today).unwrap(),plan("ဒီနေ့ ရောင်းရငွေ ပြပါ",None,today).unwrap());
  for word in ["အထွက်ငွေ","သုံးစွဲငွေ"] {
   assert_eq!(plan(&format!("ဒီလ {word} အများဆုံး ပြပါ"),None,today).unwrap(),plan("ဒီလ အသုံးစရိတ် အများဆုံး ပြပါ",None,today).unwrap());
  }
 }
}

#[cfg(test)]
mod bilingual_comparison_tests {
 use super::*;
 #[test] fn monthly_comparisons_in_both_languages() {
  let today=NaiveDate::from_ymd_opt(2026,10,8).unwrap();
  for (scope,prompts) in [(Scope::Analytics,vec!["sales comparison this month vs last month","Compare sales current month and previous month","ဒီလနဲ့ အရင်လ ရောင်းအား နှိုင်းယှဉ်ပေးပါ"]),(Scope::Expenses,vec!["expense comparison this month vs last month","ဒီလနဲ့ ပြီးခဲ့တဲ့လ အသုံးစရိတ် နှိုင်းယှဉ်ပေးပါ","ယခုလနဲ့ အရင်လ အထွက်ငွေ ယှဉ်ကြည့်ပေးပါ"])] {
   for prompt in prompts {let p=plan(prompt,None,today).unwrap();assert_eq!(p.request.scope,scope,"{prompt}");assert!(p.request.product.is_empty(),"{prompt}: {}",p.request.product);assert_eq!((&*p.request.from,&*p.request.to),("2026-10-01","2026-10-31"));let other=p.comparison.unwrap();assert_eq!((&*other.from,&*other.to),("2026-09-01","2026-09-30"));}
  }
 }
 #[test] fn same_month_last_year_and_custom_ranges() {
  let today=NaiveDate::from_ymd_opt(2024,2,15).unwrap();
  for prompt in ["sales comparison this month vs same month last year","ဒီလနဲ့ မနှစ်ကတူညီတဲ့လ အသုံးစရိတ် နှိုင်းယှဉ်ပေးပါ"] {let p=plan(prompt,None,today).unwrap();assert_eq!(p.request.to,"2024-02-29");let other=p.comparison.unwrap();assert_eq!((&*other.from,&*other.to),("2023-02-01","2023-02-28"));assert!(p.request.product.is_empty());}
  for prompt in ["compare sales 2026-10-01 to 2026-10-07 vs 2026-09-01 to 2026-09-07","၂၀၂၆-၁၀-၀၁ မှ ၂၀၂၆-၁၀-၀၇ အထိ နဲ့ ၂၀၂၆-၀၉-၀၁ မှ ၂၀၂၆-၀၉-၀၇ အထိ အသုံးစရိတ် နှိုင်းယှဉ်ပေးပါ"] {let p=plan(prompt,None,today).unwrap();assert!(p.request.product.is_empty(),"{}",p.request.product);assert_eq!((&*p.request.from,&*p.request.to),("2026-10-01","2026-10-07"));let other=p.comparison.unwrap();assert_eq!((&*other.from,&*other.to),("2026-09-01","2026-09-07"));}
  assert!(plan("compare sales 2026-10-07 to 2026-10-01 vs 2026-09-01 to 2026-09-07",None,today).is_err());
 }
 #[test] fn comparisons_keep_category_filters_and_daily_dates() {
  let today=NaiveDate::from_ymd_opt(2026,10,8).unwrap();
  for prompt in ["compare sales CCTV this month vs last month","ဒီလနဲ့ ပြီးခဲ့တဲ့လ CCTV ရောင်းအား နှိုင်းယှဉ်ပေးပါ"] {let p=plan(prompt,None,today).unwrap();assert_eq!(p.request.product,"cctv");assert_eq!(p.comparison.unwrap().product,"cctv");}
  let p=plan("compare sales today vs yesterday",None,today).unwrap();assert_eq!(p.request.from,"2026-10-08");assert_eq!(p.comparison.unwrap().from,"2026-10-07");
 }
}

#[cfg(test)]
mod requested_topic_equivalents {
 use super::*;
 #[test] fn aliases_share_normal_and_comparison_plans() {
  let today=NaiveDate::from_ymd_opt(2026,10,8).unwrap();
  for (canonical,aliases) in [("ရောင်းရငွေ",vec!["အရောင်း","ရောင်းအား","ရောင်းနှုန်း","ဝင်ငွေ"]),("အသုံးစရိတ်",vec!["အသုံးစရိတ်","ထွက်ငွေ","သုံးစွဲငွေ","အထွက်","အထွက်ငွေ","အထွက်စာရင်း"])] {
   for (prefix,suffix) in [("ဒီနေ့ "," ပြပါ"),("ဒီလနဲ့ ပြီးခဲ့တဲ့လ "," နှိုင်းယှဉ်ပေးပါ"),("ဒီလနဲ့ မနှစ်ကဒီလ "," နှိုင်းယှဉ်ပေးပါ")] {
    let expected=plan(&format!("{prefix}{canonical}{suffix}"),None,today).unwrap();
    for alias in &aliases {assert_eq!(plan(&format!("{prefix}{alias}{suffix}"),None,today).unwrap(),expected,"{alias}");}
   }
  }
 }
}
