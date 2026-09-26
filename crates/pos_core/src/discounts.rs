use anyhow::{ensure, Context, Result};
use chrono::NaiveDate;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use crate::auth::{Permission, Session};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Discount {
    pub id: i32,
    pub product_id: i32,
    pub discount_percent: Decimal,
    pub discount_type: String,
    pub manual_price: Decimal,
    pub start_date: String,
    pub end_date: String,
    pub active: i32,
    pub note: String,
}
const SELECT: &str = "SELECT id,product_id,discount_percent::numeric,COALESCE(discount_type,'percentage') AS discount_type,COALESCE(manual_price,0)::numeric AS manual_price,start_date,end_date,active,COALESCE(note,'') AS note FROM product_discounts";

pub fn date(value: &str) -> Result<NaiveDate> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
    ensure!(date.format("%Y-%m-%d").to_string()==value, "Use YYYY-MM-DD dates");
    Ok(date)
}
impl Discount {
    pub fn status(&self, today: &str) -> &'static str {
        if self.active!=1 {"Disabled"} else if today<self.start_date.as_str() {"Scheduled"} else if today>self.end_date.as_str() {"Expired"} else {"Active now"}
    }
    pub fn price(&self, base: f64) -> f64 {
        let base=Decimal::from_f64_retain(base).unwrap_or_default();
        let price=if self.discount_type=="manual_price" {self.manual_price.min(base)} else {base*(Decimal::from(100)-self.discount_percent)/Decimal::from(100)};
        price.max(Decimal::ZERO).round_dp(2).to_f64().unwrap_or(0.0)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.product_id>0 && (self.active==0||self.active==1),"Select a product");
        ensure!(date(&self.start_date)?<=date(&self.end_date)?,"End date must not precede start date");
        ensure!(self.note.len()<=4000,"Note too long");
        match self.discount_type.as_str() {
            "percentage"=>ensure!(self.discount_percent>Decimal::ZERO && self.discount_percent<=Decimal::from(100) && self.discount_percent.scale()<=2,"Percentage must be greater than 0 and at most 100"),
            "manual_price"=>ensure!(self.manual_price>Decimal::ZERO && self.manual_price<Decimal::from(1_000_000_000u64) && self.manual_price.scale()<=2,"Enter a positive price with at most two decimals"),
            _=>anyhow::bail!("Unknown promotion type")
        }
        Ok(())
    }
}
pub async fn list(pool:&PgPool, actor:&Session)->Result<Vec<Discount>> {
    let mut tx=pool.begin().await?;
    actor.authorize(&mut tx,Permission::Manage).await?;
    let rows=sqlx::query_as(&format!("{SELECT} ORDER BY start_date DESC,id DESC")).fetch_all(&mut *tx).await?;
    tx.commit().await?; Ok(rows)
}
pub async fn current(conn:&mut PgConnection, ids:&[i32], today:&str)->Result<Vec<Discount>> {
    date(today)?;
    // Match Main's cart precedence; ID breaks otherwise equal legacy campaigns.
    Ok(sqlx::query_as(&format!("{SELECT} WHERE product_id=ANY($1) AND active=1 AND start_date<=$2 AND end_date>=$2 ORDER BY CASE WHEN COALESCE(discount_type,'percentage')='manual_price' THEN 999999999-COALESCE(manual_price,0) ELSE discount_percent END DESC,end_date,id DESC"))
        .bind(ids).bind(today).fetch_all(conn).await?)
}
pub async fn attach(pool:&PgPool, products:&mut [crate::Product])->Result<()> {
    let ids=products.iter().map(|p|p.id).collect::<Vec<_>>();
    let mut conn=pool.acquire().await?;
    let rows=current(&mut conn,&ids,&chrono::Local::now().format("%Y-%m-%d").to_string()).await?;
    for p in products {
        p.promotion=rows.iter().find(|d|d.product_id==p.id).cloned();
    }
    Ok(())
}
pub(crate) async fn validate_sale(conn:&mut PgConnection,items:&[crate::SaleItemDraft])->Result<()> {
    let exists:bool=sqlx::query_scalar("SELECT to_regclass('product_discounts') IS NOT NULL").fetch_one(&mut *conn).await?;
    if !exists {ensure!(items.iter().all(|i|i.promotion.is_none()),"Promotion table missing");return Ok(())}
    let rows=current(conn,&items.iter().map(|i|i.product_id).collect::<Vec<_>>(),&chrono::Local::now().format("%Y-%m-%d").to_string()).await?;
    for item in items {
        let active=rows.iter().find(|d|d.product_id==item.product_id);
        ensure!(active==item.promotion.as_ref(),"Promotion changed or expired. Cancel this unsaved checkout, refresh and re-add the product");
        if let Some(d)=active {
            d.validate()?;
            let (base,bulk):(f64,f64)=if let Some(id)=item.variant_id {
                sqlx::query_as("SELECT price::float8,CASE WHEN wholesale_min_qty>0 AND wholesale_min_qty<=$3 AND wholesale_price>0 THEN wholesale_price ELSE price END::float8 FROM product_variants WHERE id=$1 AND product_id=$2").bind(id).bind(item.product_id).bind(item.qty).fetch_one(&mut *conn).await?
            } else {
                sqlx::query_as("SELECT p.price::float8,COALESCE((SELECT unit_price FROM product_price_tiers WHERE product_id=p.id AND COALESCE(active,1)=1 AND min_qty<=$2 AND unit_price>0 ORDER BY min_qty DESC,unit_price LIMIT 1),p.price)::float8 FROM products p WHERE p.id=$1").bind(item.product_id).bind(item.qty).fetch_one(&mut *conn).await?
            };
            ensure!((item.price-d.price(base).min(bulk)).abs()<0.005,"Promotion price changed. Refresh and re-add the product");
        }
    }
    Ok(())
}
pub(crate) async fn save(conn:&mut PgConnection, old:&Option<Discount>, new:&Discount)->Result<i32> {
    new.validate()?;
    let (price,mode):(Decimal,String)=sqlx::query_as("SELECT price::numeric,COALESCE(sold_by,'Each') FROM products WHERE id=$1 FOR UPDATE").bind(new.product_id).fetch_optional(&mut *conn).await?.context("Product no longer exists")?;
    ensure!(!matches!(mode.to_lowercase().as_str(),"service"|"restaurant"),"Service and restaurant promotions are not supported");
    if new.discount_type=="manual_price" {ensure!(new.manual_price<price,"Promotion price must be lower than product price");}
    if let Some(old)=old {
        ensure!(old.product_id==new.product_id && old.id==new.id,"Create a new campaign to change product");
        let saved:Discount=sqlx::query_as(&format!("{SELECT} WHERE id=$1 FOR UPDATE")).bind(old.id).fetch_one(&mut *conn).await?;
        ensure!(&saved==old,"Campaign changed; refresh before editing");
    } else {ensure!(new.id==0,"Invalid new campaign");}
    let overlap:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM product_discounts WHERE product_id=$1 AND id<>$2 AND active=1 AND start_date<=$3 AND end_date>=$4)")
        .bind(new.product_id).bind(new.id).bind(&new.end_date).bind(&new.start_date).fetch_one(&mut *conn).await?;
    ensure!(new.active==0||!overlap,"An enabled campaign overlaps these dates; disable it or choose another period");
    let sql=if old.is_some() {"UPDATE product_discounts SET product_id=$1,discount_percent=$2::numeric::float8,discount_type=$3,manual_price=$4::numeric::float8,start_date=$5,end_date=$6,active=$7,note=$8,updated_at=CURRENT_TIMESTAMP WHERE id=$9 RETURNING id"} else {"INSERT INTO product_discounts(product_id,discount_percent,discount_type,manual_price,start_date,end_date,active,note) SELECT $1,$2::numeric::float8,$3,$4::numeric::float8,$5,$6,$7,$8 WHERE $9=0 RETURNING id"};
    Ok(sqlx::query_scalar(sql).bind(new.product_id).bind(new.discount_percent).bind(&new.discount_type).bind(new.manual_price).bind(&new.start_date).bind(&new.end_date).bind(new.active).bind(&new.note).bind(new.id).fetch_one(conn).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn campaign_dates_and_prices() {
        let mut d=Discount{id:0,product_id:1,discount_percent:Decimal::from(20),discount_type:"percentage".into(),manual_price:Decimal::ZERO,start_date:"2026-09-01".into(),end_date:"2026-09-30".into(),active:1,note:String::new()};
        assert!(d.validate().is_ok()); assert_eq!(d.price(100.0),80.0);
        assert_eq!(d.status("2026-09-30"),"Active now");assert_eq!(d.status("2026-10-01"),"Expired");
        d.discount_percent=Decimal::from(101);assert!(d.validate().is_err());
        assert!(date("2026-9-01").is_err());
    }
}
