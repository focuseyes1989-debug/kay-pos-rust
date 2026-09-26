use crate::{receipt_layout, CartLine, Product, ReceiptData};
use dioxus::prelude::*;
use std::collections::HashMap;

fn sample_receipt() -> ReceiptData {
    let product = Product {
        promotion: None,
        id: 0, name: "Sample item".into(), description: None, category_id: None, category_name: None,
        sku: None, barcode: None, price: 2500.0, cost: 0.0, stock: 0.0,
        low_stock: 0.0, sold_by: Some("Each".into()), image_filename: None,
        image_data_url: None, variants: vec![], price_tiers: vec![],
    };
    ReceiptData {
        invoice_no: "PREVIEW-001".into(),
        lines: vec![CartLine { product, variant: None, qty: 2.0, unit_price_override: None }],
        subtotal: 5000.0, discount: 0.0, total: 5000.0,
        payment_type: "Cash".into(), payment: 10000.0, change: 5000.0,
    }
}

#[component]
pub fn ReceiptPreview(original: Vec<(String, String)>, draft: Signal<HashMap<String, String>>) -> Element {
    let mut settings: HashMap<String,String> = original.into_iter().collect();
    settings.extend(draft.read().clone());
    let rows = receipt_layout::sale(&sample_receipt(), &settings);
    let paper_mm = receipt_layout::paper_mm(&settings);
    rsx! {
        section { class: "settings_receipt_preview", aria_label: "Receipt preview",
            header { h3 { "Receipt Preview" } span { "Sample / {paper_mm} mm" } }
            div { class: "settings_receipt_preview_scroll",
                div { class: "settings_receipt_paper",style:"width:{paper_mm}mm",
                    for (index, row) in rows.iter().enumerate() {
                        div { key: "{index}", class: "settings_receipt_print_line", "data-kind":row.kind,
                            if row.kind != "rule" {
                                span {"{row.text}"}
                                if !row.right.is_empty(){span {"{row.right}"}}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn printed_receipt_uses_explicit_currency_without_ui_context() {
        for (currency, symbol, amount) in [("Kyats (Ks)", "Ks", "5,000 Ks"), ("Dollar ($)", "$", "5,000.00 $"), ("Baht (B)", "B", "5,000.00 B")] {
            let settings = HashMap::from([("currency".into(), currency.into()), ("currency_symbol".into(), "Ks".into())]);
            let rows = receipt_layout::sale(&sample_receipt(), &settings);
            assert!(rows.iter().any(|r| r.kind == "total" && r.right == amount));
            for row in rows.iter().filter(|r| r.kind == "total" || (r.kind == "pair" && r.text != "Payment method")) {
                assert!(row.right.ends_with(symbol), "{}", row.right);
            }
            if symbol != "Ks" {
                assert!(!rows.iter().any(|r| r.text.contains(" Ks") || r.right.contains(" Ks")));
            }
        }
    }
    #[test]
    fn preview_uses_printer_lines_and_draft_values() {
        let mut settings = HashMap::from([("shop_name".into(), "Original".into())]);
        settings.extend(HashMap::from([
            ("shop_name".into(), "Shop <&>".into()),
            ("receipt_footer".into(), "Thank you".into()),
        ]));
        let rows = receipt_layout::sale(&sample_receipt(), &settings);
        assert_eq!(rows[0].text, "Shop <&>");
        assert_eq!(rows[0].kind,"title");
        assert!(rows.iter().any(|r|r.kind=="item"&&r.text=="Sample item"));
        assert!(rows.iter().any(|r|r.kind=="total"&&r.text=="TOTAL"));
        assert!(rows.iter().any(|r|r.text=="Payment method"&&r.right=="Cash"));
        let mut credit=sample_receipt();
        credit.payment_type="Credit".into();
        credit.payment=1000.0; credit.change=0.0;
        let credit_rows=receipt_layout::sale(&credit,&settings);
        assert!(credit_rows.iter().any(|r|r.text=="Payment method"&&r.right=="Credit"));
        assert!(credit_rows.iter().any(|r|r.text=="Balance due"));
        assert_eq!(rows.last().unwrap().text, "Thank you");
        settings.insert("receipt_footer".into(), String::new());
        assert!(!receipt_layout::sale(&sample_receipt(), &settings).iter().any(|r|r.text=="Thank you"));
    }
}
