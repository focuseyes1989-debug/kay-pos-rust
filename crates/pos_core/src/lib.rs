pub mod catalog;
pub mod categories;
pub mod customer_credit;
pub mod customers;
pub mod db;
pub mod expenses;
pub mod inventory;
pub mod locations;
pub mod models;
pub mod numbers;
mod sale_stock;
pub mod sale_summary;
pub mod sales;
pub mod service_orders;
pub mod stock_alerts;
pub mod suppliers;
pub mod users;

pub use db::{connect, DatabaseConfig};
pub use models::{
    AppSetting, Category, Money, PaymentType, Product, ProductPriceTier, ProductVariant,
    ReceiptDetail, ReceiptItemRow, SaleDraft, SaleItemDraft, SaleSummary,
};
pub use sales::{complete_sale, get_receipt_detail, list_receipts, refund_sale};
pub mod auth;
