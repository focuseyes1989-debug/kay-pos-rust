use crate::{format_ks, format_qty, line_total, setting_value, ReceiptData, ReceiptDetail};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Row {
    pub kind: &'static str,
    pub text: String,
    pub right: String,
}
impl Row {
    fn new(kind: &'static str, text: impl Into<String>, right: impl Into<String>) -> Self {
        Self { kind, text: text.into(), right: right.into() }
    }
}

pub fn paper_mm(settings: &HashMap<String,String>) -> u16 {
    match settings.get("receipt_paper_size").map(String::as_str) {
        Some("58") => 58,
        _ => 80,
    }
}
fn header(settings: &HashMap<String,String>, invoice: &str, payment_type: &str) -> Vec<Row> {
    let mut rows=vec![Row::new("title",setting_value(settings,"shop_name","KAY POS"),"")];
    for key in ["shop_phone","shop_address","receipt_header"] {
        if let Some(text)=settings.get(key).filter(|s|!s.trim().is_empty()) {
            rows.push(Row::new("center",text,""));
        }
    }
    rows.push(Row::new("rule","",""));
    rows.push(Row::new("center",invoice,""));
    rows.push(Row::new("pair","Payment method",payment_type));
    rows.push(Row::new("rule","",""));
    rows
}
fn totals(rows: &mut Vec<Row>, subtotal: Option<f64>, discount:f64, total:f64, payment:f64, change:f64, settings:&HashMap<String,String>) {
    rows.push(Row::new("rule","",""));
    if let Some(subtotal)=subtotal {
        rows.push(Row::new("pair","Subtotal",format_ks(subtotal)));
        let tax=(total-subtotal+discount).max(0.0);
        if tax>0.0 {rows.push(Row::new("pair","Tax",format_ks(tax)));}
    }
    if discount>0.0 {rows.push(Row::new("pair","Discount",format_ks(discount)));}
    rows.push(Row::new("total","TOTAL",format_ks(total)));
    rows.push(Row::new("pair","Received",format_ks(payment)));
    rows.push(Row::new("pair","Change",format_ks(change)));
    if payment<total {rows.push(Row::new("pair","Balance due",format_ks(total-payment)));}
    rows.push(Row::new("rule","",""));
    for key in ["receipt_footer","shop_footer_message"] {
        if let Some(text)=settings.get(key).filter(|s|!s.trim().is_empty()) {rows.push(Row::new("center",text,""));}
    }
}
pub fn sale(receipt: &ReceiptData, settings:&HashMap<String,String>) -> Vec<Row> {
    let mut rows=header(settings,&receipt.invoice_no,&receipt.payment_type);
    for item in &receipt.lines {
        rows.push(Row::new("item",item.display_name(),""));
        rows.push(Row::new("pair",format!("{} x {}",format_qty(item.qty),format_ks(item.unit_price())),format_ks(line_total(item))));
    }
    totals(&mut rows,Some(receipt.subtotal),receipt.discount,receipt.total,receipt.payment,receipt.change,settings);
    rows
}
pub fn detail(receipt: &ReceiptDetail, settings:&HashMap<String,String>) -> Vec<Row> {
    let summary=&receipt.summary;
    let invoice=summary.invoice_no.clone().unwrap_or_else(||format!("Sale #{}",summary.id));
    let mut rows=header(settings,&invoice,summary.payment_type.as_deref().unwrap_or("Unknown"));
    for item in &receipt.items {
        rows.push(Row::new("item",item.product_name.as_deref().unwrap_or("Item"),""));
        rows.push(Row::new("pair",format!("{} x {}",format_qty(item.qty),format_ks(item.price)),format_ks(item.total)));
    }
    // Historic totals may include tax not separately stored in the receipt summary.
    let subtotal=receipt.items.iter().map(|item|item.total).sum();
    totals(&mut rows,Some(subtotal),summary.discount_amount,summary.total,summary.payment,summary.change_amount,settings);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_paper_sizes_and_default() {
        let mut settings=HashMap::new();
        assert_eq!(paper_mm(&settings),80);
        settings.insert("receipt_paper_size".into(),"58".into());assert_eq!(paper_mm(&settings),58);
        settings.insert("receipt_paper_size".into(),"invalid".into());assert_eq!(paper_mm(&settings),80);
    }
}
