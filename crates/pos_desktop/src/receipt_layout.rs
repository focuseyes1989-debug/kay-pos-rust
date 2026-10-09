use crate::{format_qty, line_total, setting_value, ReceiptData, ReceiptDetail};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Cell {
    pub text: String,
    pub weight: f64,
    pub align: String,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Row {
    pub kind: &'static str,
    pub text: String,
    pub right: String,
    pub block: String,
    pub field: String,
    pub cells: Vec<Cell>,
    pub pair_percent: f64,
    pub font_size: f64,
    pub bold: bool,
    pub align: String,
    pub line_height: f64,
    pub gap_mm: f64,
}
impl Row {
    fn new(kind: &'static str, text: impl Into<String>, right: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            right: right.into(),
            field: kind.into(),
            cells: vec![],
            pair_percent: 55.,
            block: if kind == "title" || kind == "total" {
                kind.into()
            } else {
                "body".into()
            },
            font_size: 9.,
            bold: false,
            align: "left".into(),
            line_height: 1.5,
            gap_mm: 1.,
        }
    }
}

pub fn paper_mm(settings: &HashMap<String, String>) -> u16 {
    crate::receipt_editor::Geometry::from(settings)
        .paper_mm
        .round() as u16
}
pub fn logo(settings: &HashMap<String, String>) -> Option<String> {
    settings
        .get("receipt_logo_image")
        .filter(|v| v.starts_with("data:image/png;base64,") && !v.is_empty())
        .cloned()
}
fn header(settings: &HashMap<String, String>, invoice: &str, payment_type: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some(logo) = logo(settings) {
        rows.push(Row::new("logo", logo, ""));
    }
    rows.push(Row::new(
        "title",
        setting_value(settings, "shop_name", "KAY POS"),
        "",
    ));
    for key in ["shop_phone", "shop_address", "receipt_header"] {
        if let Some(text) = settings.get(key).filter(|s| !s.trim().is_empty()) {
            let mut row = Row::new("center", text, "");
            row.field = match key {
                "shop_phone" => "phone",
                "shop_address" => "address",
                "receipt_header" => "header",
                "receipt_footer" => "footer",
                _ => "message",
            }
            .into();
            row.block = match key {
                "shop_phone" => "phone",
                "shop_address" => "address",
                "receipt_header" => "header",
                "receipt_footer" => "footer",
                _ => "message",
            }
            .into();
            rows.push(row);
        }
    }
    rows.push(Row::new("rule", "", ""));
    let mut invoice_row = Row::new("center", invoice, "");
    invoice_row.block = "header".into();
    invoice_row.field = "invoice".into();
    rows.push(invoice_row);
    rows.push(field_row("payment", "pair", "Payment method", payment_type));
    rows.push(Row::new("rule", "", ""));
    rows
}
fn totals(
    rows: &mut Vec<Row>,
    subtotal: Option<f64>,
    discount: f64,
    total: f64,
    payment: f64,
    change: f64,
    settings: &HashMap<String, String>,
) {
    let currency = crate::regional::Currency::from_settings(settings);
    let format_ks = |value| currency.format(value);
    rows.push(Row::new("rule", "", ""));
    if let Some(subtotal) = subtotal {
        rows.push(field_row(
            "subtotal",
            "pair",
            "Subtotal",
            format_ks(subtotal),
        ));
        let tax = (total - subtotal + discount).max(0.0);
        if tax > 0.0 {
            rows.push(field_row("tax", "pair", "Tax", format_ks(tax)));
        }
    }
    if discount > 0.0 {
        rows.push(field_row(
            "discount",
            "pair",
            "Discount",
            format_ks(discount),
        ));
    }
    rows.push(Row::new("total", "TOTAL", format_ks(total)));
    rows.push(field_row(
        "received",
        "pair",
        "Received",
        format_ks(payment),
    ));
    rows.push(field_row("change", "pair", "Change", format_ks(change)));
    if payment < total {
        rows.push(field_row(
            "balance",
            "pair",
            "Balance due",
            format_ks(total - payment),
        ));
    }
    rows.push(Row::new("rule", "", ""));
    for key in ["receipt_footer", "shop_footer_message"] {
        if let Some(text) = settings.get(key).filter(|s| !s.trim().is_empty()) {
            let mut row = Row::new("center", text, "");
            row.field = match key {
                "shop_phone" => "phone",
                "shop_address" => "address",
                "receipt_header" => "header",
                "receipt_footer" => "footer",
                _ => "message",
            }
            .into();
            row.block = match key {
                "shop_phone" => "phone",
                "shop_address" => "address",
                "receipt_header" => "header",
                "receipt_footer" => "footer",
                _ => "message",
            }
            .into();
            rows.push(row);
        }
    }
}
pub fn sale(receipt: &ReceiptData, settings: &HashMap<String, String>) -> Vec<Row> {
    let mut rows = header(settings, &receipt.invoice_no, &receipt.payment_type);
    item_header(&mut rows, settings);
    for item in &receipt.lines {
        item_rows(
            &mut rows,
            &item.display_name(),
            item.qty,
            item.unit_price(),
            line_total(item),
            settings,
        );
    }
    totals(
        &mut rows,
        Some(receipt.subtotal),
        receipt.discount,
        receipt.total,
        receipt.payment,
        receipt.change,
        settings,
    );
    arrange(&mut rows, settings);
    style_rows(&mut rows, settings);
    rows
}
pub fn detail(receipt: &ReceiptDetail, settings: &HashMap<String, String>) -> Vec<Row> {
    let summary = &receipt.summary;
    let invoice = summary
        .invoice_no
        .clone()
        .unwrap_or_else(|| format!("Sale #{}", summary.id));
    let mut rows = header(
        settings,
        &invoice,
        summary.payment_type.as_deref().unwrap_or("Unknown"),
    );
    item_header(&mut rows, settings);
    for item in &receipt.items {
        item_rows(
            &mut rows,
            item.product_name.as_deref().unwrap_or("Item"),
            item.qty,
            item.price,
            item.total,
            settings,
        );
    }
    // Historic totals may include tax not separately stored in the receipt summary.
    let subtotal = receipt.items.iter().map(|item| item.total).sum();
    totals(
        &mut rows,
        Some(subtotal),
        summary.discount_amount,
        summary.total,
        summary.payment,
        summary.change_amount,
        settings,
    );
    arrange(&mut rows, settings);
    style_rows(&mut rows, settings);
    rows
}

fn field_row(
    field: &str,
    kind: &'static str,
    text: impl Into<String>,
    right: impl Into<String>,
) -> Row {
    let mut r = Row::new(kind, text, right);
    r.field = field.into();
    r
}
fn item_header(rows: &mut Vec<Row>, settings: &HashMap<String, String>) {
    if settings
        .get("receipt_item_layout")
        .is_some_and(|v| v == "table")
    {
        let mut r = field_row("items", "columns", "", "");
        r.cells = crate::receipt_fields::columns(settings)
            .into_iter()
            .map(|(key, weight)| Cell {
                text: crate::receipt_fields::COLUMNS
                    .iter()
                    .find(|(k, _)| *k == key)
                    .unwrap()
                    .1
                    .into(),
                weight,
                align: if key == "name" { "left" } else { "right" }.into(),
            })
            .collect();
        rows.push(r);
    }
}
fn item_rows(
    rows: &mut Vec<Row>,
    name: &str,
    qty: f64,
    price: f64,
    total: f64,
    settings: &HashMap<String, String>,
) {
    let currency = crate::regional::Currency::from_settings(settings);
    if settings
        .get("receipt_item_layout")
        .is_some_and(|v| v == "table")
    {
        let mut r = field_row("items", "columns", "", "");
        r.cells = crate::receipt_fields::columns(settings)
            .into_iter()
            .map(|(key, weight)| Cell {
                text: match key.as_str() {
                    "name" => name.into(),
                    "qty" => format_qty(qty),
                    "price" => currency.format(price),
                    _ => currency.format(total),
                },
                weight,
                align: if key == "name" { "left" } else { "right" }.into(),
            })
            .collect();
        rows.push(r);
    } else {
        if crate::receipt_fields::visible(settings, "column_name") {
            rows.push(field_row("items", "item", name, ""));
        }
        let mut parts = Vec::new();
        if crate::receipt_fields::visible(settings, "column_qty") {
            parts.push(format_qty(qty));
        }
        if crate::receipt_fields::visible(settings, "column_price") {
            parts.push(currency.format(price));
        }
        let amount = if crate::receipt_fields::visible(settings, "column_amount") {
            currency.format(total)
        } else {
            String::new()
        };
        if !parts.is_empty() || !amount.is_empty() {
            rows.push(field_row("items", "pair", parts.join(" x "), amount));
        }
    }
}
fn arrange(rows: &mut Vec<Row>, settings: &HashMap<String, String>) {
    rows.retain(|r| r.kind == "rule" || crate::receipt_fields::visible(settings, &r.field));
    let order = crate::receipt_fields::order(settings);
    let default_order = crate::receipt_fields::FIELDS
        .iter()
        .map(|(k, _)| k.to_string())
        .collect::<Vec<_>>();
    if order != default_order {
        rows.retain(|r| r.kind != "rule");
        rows.sort_by_key(|r| {
            order
                .iter()
                .position(|k| k == &r.field)
                .unwrap_or(usize::MAX)
        });
        let group = |field: &str| match field {
            "logo" | "title" | "phone" | "address" | "header" => 0,
            "invoice" | "payment" => 1,
            "items" => 2,
            "footer" | "message" => 4,
            _ => 3,
        };
        let mut arranged = Vec::new();
        let mut last = None;
        for row in rows.drain(..) {
            let current = group(&row.field);
            if last.is_some_and(|previous| previous != current) {
                arranged.push(Row::new("rule", "", ""));
            }
            last = Some(current);
            arranged.push(row);
        }
        *rows = arranged;
    } else {
        let mut last_rule = true;
        rows.retain(|r| {
            let is_rule = r.kind == "rule";
            let keep = !is_rule || !last_rule;
            last_rule = is_rule;
            keep
        });
        if rows.last().is_some_and(|r| r.kind == "rule") {
            rows.pop();
        }
    }
}

fn style_rows(rows: &mut [Row], settings: &HashMap<String, String>) {
    let defaults = crate::receipt_editor::defaults();
    let mut values = defaults;
    values.extend(settings.clone());
    for row in rows {
        let key = |suffix: &str| format!("receipt_{}_{}", row.block, suffix);
        row.pair_percent =
            crate::receipt_editor::number(settings, "receipt_pair_percent", 55., 20., 80.);
        row.font_size = crate::receipt_editor::number(&values, &key("font_size"), 9., 6., 24.);
        row.bold = values.get(&key("bold")).is_some_and(|v| v == "1");
        row.align = values.get(&key("align")).cloned().unwrap_or("left".into());
        row.line_height = crate::receipt_editor::number(&values, &key("line_height"), 1.5, 1., 3.);
        row.gap_mm = crate::receipt_editor::number(&values, &key("gap"), 1., 0., 20.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_receipt_and_historic_reprint_share_custom_layout() {
        let sale = crate::receipt_preview::sample_receipt();
        let receipt = crate::ReceiptDetail {
            summary: crate::SaleSummary {
                id: 1,
                invoice_no: Some(sale.invoice_no.clone()),
                created_at: None,
                total: sale.total,
                payment: sale.payment,
                change_amount: sale.change,
                discount_amount: sale.discount,
                payment_type: Some(sale.payment_type.clone()),
                status: None,
                customer_name: None,
                item_count: 1,
            },
            items: vec![pos_core::ReceiptItemRow {
                product_name: Some(sale.lines[0].display_name()),
                qty: 2.,
                price: 2500.,
                total: 5000.,
            }],
        };
        let mut s = crate::receipt_editor::defaults();
        s.insert("receipt_item_layout".into(), "table".into());
        s.insert("receipt_show_invoice".into(), "0".into());
        s.insert("receipt_pair_percent".into(), "65".into());
        s.insert(
            "receipt_field_order".into(),
            crate::receipt_fields::move_key(&s, "receipt_field_order", "items", true),
        );
        assert_eq!(super::sale(&sale, &s), detail(&receipt, &s));
    }
    #[test]
    fn field_visibility_order_and_table_keep_sale_amounts() {
        let receipt = crate::receipt_preview::sample_receipt();
        let mut s = crate::receipt_editor::defaults();
        s.insert("receipt_item_layout".into(), "table".into());
        s.insert("receipt_show_payment".into(), "0".into());
        s.insert("receipt_show_column_price".into(), "0".into());
        s.insert(
            "receipt_column_order".into(),
            "qty,name,amount,price".into(),
        );
        s.insert("receipt_column_name".into(), "60".into());
        let mut order = crate::receipt_fields::order(&s);
        order.retain(|k| k != "total");
        order.insert(0, "total".into());
        s.insert("receipt_field_order".into(), order.join(","));
        crate::receipt_editor::validate(&s).unwrap();
        let rows = sale(&receipt, &s);
        assert_eq!(rows[0].field, "total");
        assert_eq!(rows[0].right, "5,000 Ks");
        assert!(!rows.iter().any(|r| r.field == "payment"));
        let table = rows
            .iter()
            .filter(|r| r.kind == "columns")
            .collect::<Vec<_>>();
        assert_eq!(table.len(), 2);
        assert_eq!(
            table[1]
                .cells
                .iter()
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "Sample item", "5,000 Ks"]
        );
        assert_eq!(table[1].cells[1].weight, 60.);
        assert_eq!(receipt.total, 5000.);
    }
    #[test]
    fn hidden_items_leave_no_table_header_or_orphan_rules() {
        let mut s = crate::receipt_editor::defaults();
        s.insert("receipt_item_layout".into(), "table".into());
        s.insert("receipt_show_items".into(), "0".into());
        let rows = sale(&crate::receipt_preview::sample_receipt(), &s);
        assert!(!rows.iter().any(|r| r.kind == "columns"));
        assert!(!rows
            .windows(2)
            .any(|r| r[0].kind == "rule" && r[1].kind == "rule"));
        assert_ne!(rows.last().unwrap().kind, "rule");
    }
    #[test]
    fn editor_styles_apply_to_each_text_block() {
        let settings = HashMap::from([
            ("shop_phone".into(), "123".into()),
            ("shop_address".into(), "First line\nSecond line".into()),
            ("receipt_title_font_size".into(), "18".into()),
            ("receipt_title_bold".into(), "0".into()),
            ("receipt_address_line_height".into(), "2.4".into()),
            ("receipt_address_gap".into(), "4".into()),
            ("receipt_address_align".into(), "right".into()),
            ("receipt_total_font_size".into(), "16".into()),
            ("receipt_paper_size".into(), "112".into()),
        ]);
        let rows = sale(&crate::receipt_preview::sample_receipt(), &settings);
        let title = rows.iter().find(|r| r.block == "title").unwrap();
        assert_eq!(title.font_size, 18.);
        assert!(!title.bold);
        let address = rows.iter().find(|r| r.block == "address").unwrap();
        assert_eq!(address.line_height, 2.4);
        assert_eq!(address.gap_mm, 4.);
        assert_eq!(address.align, "right");
        assert!(address.text.contains('\n'));
        assert_eq!(
            rows.iter().find(|r| r.kind == "total").unwrap().font_size,
            16.
        );
        assert_eq!(paper_mm(&settings), 112);
    }
    #[test]
    fn valid_paper_sizes_and_default() {
        let mut settings = HashMap::new();
        assert_eq!(paper_mm(&settings), 80);
        settings.insert("receipt_paper_size".into(), "58".into());
        assert_eq!(paper_mm(&settings), 58);
        settings.insert("receipt_paper_size".into(), "invalid".into());
        assert_eq!(paper_mm(&settings), 80);
    }
}
