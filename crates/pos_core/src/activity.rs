use crate::storage as sqlx;
use crate::auth::{Permission, Session};
use anyhow::{ensure, Result};
use chrono::NaiveDate;
use sqlx::{PgConnection, PgPool};

// Call inside the business transaction. Never log credentials or attachment bodies.
pub(crate) async fn record(
    conn: &mut PgConnection,
    actor: &Session,
    action: &str,
    details: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO user_activity_log(user_id,username,action,details,ip_address,created_at) VALUES((SELECT id FROM users WHERE username=$1),$1,$2,$3,NULL,LOCALTIMESTAMP)")
        .bind(actor.username()).bind(action).bind(details).execute(conn).await?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, sqlx::FromRow)]
pub struct Entry {
    pub id: i32,
    pub username: String,
    pub action: String,
    pub details: String,
    pub ip_address: String,
    pub created_at: String,
}

pub async fn list(
    pool: &PgPool,
    actor: &Session,
    from: &str,
    to: &str,
    query: &str,
    page: i64,
) -> Result<Vec<Entry>> {
    let from = NaiveDate::parse_from_str(from, "%Y-%m-%d")?;
    let to = NaiveDate::parse_from_str(to, "%Y-%m-%d")?;
    ensure!(
        from <= to && (0..=1_000_000).contains(&page),
        "Invalid date range or page"
    );
    ensure!(query.len() <= 300, "Search is too long");
    let mut tx = pool.begin().await?;
    actor.authorize(&mut tx, Permission::Manage).await?;
    let rows=sqlx::query_as("SELECT id,username,action,COALESCE(details,'') AS details,COALESCE(ip_address,'') AS ip_address,created_at::text AS created_at FROM user_activity_log WHERE created_at >= $1::date AND created_at < $2::date + INTERVAL '1 day' AND ($3='' OR strpos(lower(username||' '||action||' '||COALESCE(details,'')),lower($3))>0) ORDER BY created_at DESC,id DESC LIMIT 51 OFFSET $4")
        .bind(from).bind(to).bind(query.trim()).bind(page*50).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows)
}

/// Display labels only: stored audit records remain unchanged.
pub fn action_label(action: &str) -> String {
    match action {
        "rust.ai.query" => "Assistant မေးခွန်း",
        "rust.ai.chat" => "Assistant AI စကားပြော",
        "rust.ai.failed" => "Assistant မေးခွန်း မအောင်မြင်",
        "rust.ai.detail" => "Assistant အသေးစိတ်ကြည့်ခြင်း",
        "rust.sale.complete" => "အရောင်းပြီးစီး",
        "rust.sale.refund" => "အရောင်းပြန်အမ်း",
        "rust.expense.save" => "အသုံးစရိတ် သိမ်းဆည်း",
        "rust.expense.delete" => "အသုံးစရိတ် ဖျက်ခြင်း",
        "rust.category.save" => "Category သိမ်းဆည်း",
        "rust.category.delete" => "Category ဖျက်ခြင်း",
        "rust.category_group.save" => "Category group သိမ်းဆည်း",
        "rust.credit.collect" => "အကြွေးငွေ ကောက်ခံ",
        "rust.credit.create" => "အကြွေးစာရင်း ဖွင့်ခြင်း",
        "rust.held.save" => "အရောင်း ယာယီသိမ်း",
        "rust.held.resume" => "ယာယီအရောင်း ပြန်ဖွင့်",
        "rust.inventory.adjust" => "ကုန်လက်ကျန် ညှိခြင်း",
        "rust.inventory.receive" => "ကုန်ပစ္စည်း လက်ခံ",
        "rust.inventory.stock_out" => "ကုန်ပစ္စည်း ထုတ်ခြင်း",
        "rust.inventory.reverse" => "ကုန်လက်ကျန်စာရင်း ပြန်ပယ်ဖျက်",
        "rust.locations.transfer" => "ကုန်ပစ္စည်း နေရာပြောင်း",
        "rust.service_order.edit" => "Service order ပြင်ဆင်",
        "rust.service_order.delete" => "Service order ဖျက်ခြင်း",
        "rust.service_order.status" => "Service order အခြေအနေပြောင်း",
        _ => action,
    }.into()
}

pub fn readable_details(details: &str) -> Vec<(String, String)> {
    details.split(';').filter_map(|part| {
        let (key,value)=part.trim().split_once('=')?;
        let label=match key.trim() {
            "request_hash" => return None,
            "scope" => "အကြောင်းအရာ", "period" => "ကာလ",
            "sources" => "အချက်အလက်ရင်းမြစ်", "method" => "ဖြေဆိုနည်း",
            "result" => "ရလဒ်", "sale_id" => "အရောင်း ID", "invoice" => "ဘောင်ချာ",
            "expense_id" => "အသုံးစရိတ် ID", "expense_no" => "အသုံးစရိတ်အမှတ်",
            "product_id" => "ပစ္စည်း ID", "qty" | "quantity" => "အရေအတွက်",
            "amount" => "ငွေပမာဏ", "customer_id" => "ဖောက်သည် ID",
            "reason" => "အကြောင်းရင်း", "status" => "အခြေအနေ",
            other => other,
        };
        let value=value.trim();
        let display=match (key.trim(),value) {
            ("scope","UnsoldProducts") => "အရောင်းထိုင်း / မရောင်းရသော ပစ္စည်းများ".into(),
            ("scope","CategorySales") => "Category အလိုက် အရောင်း".into(),
            ("scope","ParentSales") => "Parent category အလိုက် အရောင်း".into(),
            ("scope","Analytics") => "အရောင်းအနှစ်ချုပ်".into(),
            ("scope","Inventory") => "ကုန်လက်ကျန်".into(),
            ("scope","Products") => "ကုန်ပစ္စည်း".into(),
            ("scope","Receipts") => "အရောင်းဘောင်ချာများ".into(),
            ("scope","Expenses") => "အသုံးစရိတ်".into(),
            ("method","deterministic") => "POS ဒေတာမှ တိုက်ရိုက်တွက်ချက်".into(),
            ("method","model") => "AI model ဖြင့် ဖြေဆို".into(),
            ("result","denied_or_failed") => "အသုံးပြုခွင့်မရှိခြင်း သို့မဟုတ် လုပ်ဆောင်မှုမအောင်မြင်ခြင်း".into(),
            ("period",_) => value.replace(".."," → "),
            ("sources",_) => value.split(',').map(|source| match source.trim() {
                "products"=>"ကုန်ပစ္စည်း", "product_variants"=>"ပစ္စည်းအမျိုးကွဲ",
                "sales"=>"အရောင်း", "sale_items"=>"ရောင်းထားသောပစ္စည်း",
                "categories"=>"Category", "expenses"=>"အသုံးစရိတ်", other=>other,
            }).collect::<Vec<_>>().join("၊ "),
            _ => value.into(),
        };
        Some((label.into(),display))
    }).collect()
}

#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn audit_display_preserves_unknown_values_and_hides_hash_in_summary() {
        let raw="request_hash=abc; scope=UnsoldProducts; period=2026-10-01..2026-10-09; method=deterministic; future=a=b";
        let rows=readable_details(raw);
        assert_eq!(rows.len(),4);
        assert!(rows[0].1.contains("မရောင်းရ"));
        assert_eq!(rows[1].1,"2026-10-01 → 2026-10-09");
        assert_eq!(rows[3],("future".into(),"a=b".into()));
        assert!(readable_details("Product: Milk").is_empty());
        assert_eq!(action_label("Edit Product"),"Edit Product");
    }
}
