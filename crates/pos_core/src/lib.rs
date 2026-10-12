extern crate self as pos_core;
pub mod catalog;
pub mod dashboard;
pub mod reports;
pub mod purchases;
pub mod compatibility;
pub mod categories;
pub mod customer_credit;
pub mod customers;
pub mod db;
pub mod expenses;
pub mod discounts;
pub mod expense_management;
pub mod held_sales;
pub mod loyalty;
pub mod employees;
pub mod erp_master;
pub mod erp_operations;
pub mod erp_accounting;
pub mod erp_procurement;
pub mod erp_reconciliation;
pub mod erp_sync;
pub mod inventory;
pub mod inventory_reversal;
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
pub mod zkteco;
pub mod attendance_sync;

pub use db::{connect, DatabaseConfig};
pub use models::{
    AppSetting, Category, Money, PaymentType, Product, ProductPriceTier, ProductVariant,
    ReceiptDetail, ReceiptItemRow, SaleDraft, SaleItemDraft, SaleSummary,
};
pub use sales::{complete_sale, get_receipt_detail, list_receipts, refund_sale};
pub mod auth;
pub mod activity;
pub mod ai;
pub mod assistant;
pub mod category_groups;

pub mod ai_chat;
pub const ERP_ENABLED: bool = cfg!(feature = "erp-integration");

#[cfg(all(test, not(feature = "erp-integration")))]
mod pos_only_tests {
    #[test]
    fn erp_workers_are_disabled_in_default_builds() {
        assert!(!super::ERP_ENABLED);
        assert!(super::erp_sync::ErpSyncConfig::from_env().unwrap().is_none());
        assert!(super::erp_master::MasterSyncConfig::from_env().unwrap().is_none());
        assert!(super::erp_reconciliation::ReconciliationConfig::from_env().unwrap().is_none());
    }
}

pub mod stock_transfer;

pub mod sales_comparison;

pub mod storage;
mod sqlite_dialect;
pub mod sqlite;

pub mod data_transfer;
