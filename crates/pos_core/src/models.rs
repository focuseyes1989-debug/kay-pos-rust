use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub type Money = Decimal;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub parent_id: Option<i32>,
    pub color: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppSetting {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaymentType {
    pub id: i32,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Product {
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub category_id: Option<i32>,
    pub category_name: Option<String>,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub price: f64,
    pub cost: f64,
    pub stock: f64,
    pub low_stock: f64,
    pub sold_by: Option<String>,
    pub image_filename: Option<String>,
    pub image_data_url: Option<String>,
    #[sqlx(skip)]
    pub variants: Vec<ProductVariant>,
    #[sqlx(skip)]
    pub price_tiers: Vec<ProductPriceTier>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProductVariant {
    pub variant_id: i32,
    pub product_id: i32,
    pub size: Option<String>,
    pub color: Option<String>,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub price: f64,
    pub cost: f64,
    pub stock: f64,
    pub low_stock: f64,
    pub wholesale_min_qty: i32,
    pub wholesale_price: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProductPriceTier {
    pub unit_multiplier: i32,
    pub id: i32,
    pub product_id: i32,
    pub min_qty: i32,
    pub unit_label: Option<String>,
    pub unit_price: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaleItemDraft {
    pub product_id: i32,
    pub variant_id: Option<i32>,
    pub product_name: String,
    pub qty: f64,
    pub price: f64,
    pub cost: f64,
    pub is_service: bool,
    pub wholesale_regular_price: f64,
    pub wholesale_savings: f64,
    pub wholesale_tier_min_qty: Option<i32>,
    pub wholesale_unit_label: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaleDraft {
    pub invoice_no: String,
    pub customer_id: Option<i32>,
    pub payment_type: String,
    pub payment: f64,
    pub discount_amount: f64,
    pub expected_total: f64,
    pub created_by: String,
    pub items: Vec<SaleItemDraft>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct CompletedSale {
    pub id: i32,
    pub invoice_no: String,
    pub total: f64,
    pub payment: f64,
    pub change_amount: f64,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SaleSummary {
    pub id: i32,
    pub invoice_no: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub total: f64,
    pub payment: f64,
    pub change_amount: f64,
    pub discount_amount: f64,
    pub payment_type: Option<String>,
    pub status: Option<String>,
    pub customer_name: Option<String>,
    pub item_count: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReceiptItemRow {
    pub product_name: Option<String>,
    pub qty: f64,
    pub price: f64,
    pub total: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReceiptDetail {
    pub summary: SaleSummary,
    pub items: Vec<ReceiptItemRow>,
}
