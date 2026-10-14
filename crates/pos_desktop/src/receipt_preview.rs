use crate::{receipt_layout, CartLine, Product, ReceiptData};
use dioxus::prelude::*;
use std::collections::HashMap;

pub(crate) fn sample_receipt() -> ReceiptData {
    let product = Product {
        promotion: None,
        id: 0, name: "Sample item".into(), description: None, category_id: None, category_name: None,
        sku: None, barcode: None, price: 2500.0, cost: 0.0, stock: 0.0,
        low_stock: 0.0, sold_by: Some("Each".into()), image_filename: None,
        image_data_url: None, variants: vec![], price_tiers: vec![],
    };
    ReceiptData {
        serials:Vec::new(),
        invoice_no: "PREVIEW-001".into(),
        lines: vec![CartLine { product, variant: None, qty: 2.0, unit_price_override: None, serials:Vec::new() }],
        subtotal: 5000.0, discount: 0.0, total: 5000.0,
        payment_type: "Cash".into(), payment: 10000.0, change: 5000.0,
    }
}

#[component]
pub fn ReceiptPreview(original: Vec<(String, String)>, draft: Signal<HashMap<String, String>>, #[props(default=false)] disabled:bool) -> Element {
    let mut settings: HashMap<String,String> = original.into_iter().collect();
    settings.extend(draft.read().clone());
    let paper_mm=crate::receipt_editor::Geometry::from(&settings).paper_mm;
    rsx!{section {class:"settings_receipt_preview",aria_label:"Receipt preview",
        header {h3 {"Receipt Preview"} span {"Sample / {paper_mm} mm"}}
        div {class:"settings_receipt_preview_scroll",ReceiptPaper {receipt:sample_receipt(),settings,editable:!disabled}}
    }}
}
#[component]
pub fn ReceiptPaper(receipt:ReceiptData,settings:HashMap<String,String>,#[props(default=false)] editable:bool)->Element {
    let rows=receipt_layout::sale(&receipt,&settings);
    let geometry=crate::receipt_editor::Geometry::from(&settings);
    let paper_mm=geometry.paper_mm;
    let content_mm=geometry.content_mm();
    let logo_style=format!("max-width:{}mm;max-height:{}mm;margin-left:{};margin-right:{};",geometry.logo_width_mm,geometry.logo_height_mm,if geometry.logo_align=="left"{"0"}else{"auto"},if geometry.logo_align=="right"{"0"}else{"auto"});
    rsx!{
                div { class:if editable{"settings_receipt_paper receipt_reorder_preview"}else{"settings_receipt_paper"},style:format!("width:{paper_mm}mm;padding:{}mm {}mm {}mm {}mm;",geometry.top_mm,geometry.right_mm,geometry.bottom_mm,geometry.left_mm),
                    div {class:"receipt_printable_content",style:format!("width:{content_mm}mm;max-width:100%;margin:auto;"),
                    for (index, row) in rows.iter().enumerate() {
                        div { key: "{index}", class: "settings_receipt_print_line", "data-kind":row.kind,"data-receipt-domain":if editable&&row.kind!="rule"{"fields"}else{""},"data-receipt-key":row.field.clone(),style:format!("--receipt-pair-left:{}fr;--receipt-pair-right:{}fr;font-size:{}pt!important;font-weight:{}!important;line-height:{}!important;text-align:{};margin:0 0 {}mm;",row.pair_percent,100.-row.pair_percent,row.font_size,if row.bold{700}else{400},row.line_height,row.align,if row.kind=="rule"{geometry.section_gap_mm}else if row.kind=="logo"{geometry.logo_gap_mm}else{row.gap_mm}),
                            if editable&&row.kind!="rule"&&!rows[..index].iter().any(|r|r.field==row.field) {div {class:"receipt_preview_grip",crate::receipt_fields::DragHandle {domain:"fields",field:row.field.clone(),label:crate::receipt_fields::FIELDS.iter().find(|(key,_)|*key==row.field).map(|(_,label)|*label).unwrap_or("Block"),disabled:false}}}
                            if row.kind == "logo" {img {class:"receipt_logo",style:logo_style.clone(),src:"{row.text}",alt:"Receipt logo"}}else if row.kind=="columns" {div {class:"receipt_item_columns",style:format!("grid-template-columns:{};",row.cells.iter().map(|c|format!("minmax(0,{}fr)",c.weight)).collect::<Vec<_>>().join(" ")),for (i,cell) in row.cells.iter().enumerate(){span {key:"{i}",style:format!("text-align:{};",cell.align),"{cell.text}"}}}}else if row.kind != "rule" {
                                span {"{row.text}"}
                                if !row.right.is_empty()||row.kind=="pair"||row.kind=="total"{span {"{row.right}"}}
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

#[component]
pub fn ReceiptLogoPicker(original:Vec<(String,String)>,mut draft:Signal<HashMap<String,String>>,disabled:bool)->Element {
 let mut busy=use_signal(||false);let mut error=use_signal(String::new);
 let logo=draft.read().get("receipt_logo_image").cloned().or_else(||original.iter().find(|(k,_)|k=="receipt_logo_image").map(|(_,v)|v.clone())).unwrap_or_default();
 let filename=draft.read().get("receipt_logo_filename").cloned().or_else(||original.iter().find(|(k,_)|k=="receipt_logo_filename").map(|(_,v)|v.clone())).filter(|v|!v.is_empty()).unwrap_or_else(||if logo.is_empty(){"No file chosen".into()}else{"Selected logo".into()});
 rsx!{div {class:"receipt_logo_picker",
  strong {"Receipt Logo"}
  if logo.starts_with("data:image/png;base64,"){img {class:"receipt_logo",src:logo.clone(),alt:"Selected receipt logo"}}
  div {class:"product_image_picker receipt_logo_actions",
   button {class:"product_image_choose",r#type:"button",disabled:disabled||busy(),onclick:move |_|{busy.set(true);error.set(String::new());spawn(async move {if let Some(file)=rfd::AsyncFileDialog::new().add_filter("Logo image",&["png","jpg","jpeg","webp"]).pick_file().await {let bytes=file.read().await;match normalize_logo(&bytes){Ok(value)=>{let mut values=draft.write();values.insert("receipt_logo_image".into(),value);values.insert("receipt_logo_filename".into(),file.file_name());},Err(e)=>error.set(format!("{e:#}"))}}busy.set(false);});},crate::icons::ActionLabel {label:"Choose File"}}
   span {class:"product_image_filename",title:filename.clone(),"{filename}"}
  if !logo.is_empty() {button {class:"receipt_logo_remove",r#type:"button",aria_label:"Remove logo",title:"Remove logo",disabled:disabled||busy(),onclick:move |_|{{let mut values=draft.write();values.insert("receipt_logo_image".into(),String::new());values.insert("receipt_logo_filename".into(),String::new());}error.set(String::new());},crate::icons::Icon {name:"delete"}}
  }
  }
  small {"PNG / JPG / WebP · up to 4 MB. Save settings to apply the logo to all receipts."}
  if !error().is_empty(){p {role:"alert","{error}"}}
 }}
}
fn normalize_logo(bytes:&[u8])->anyhow::Result<String> {
 use base64::Engine;
 anyhow::ensure!(bytes.len()<=4*1024*1024,"Logo must be below 4 MB");
 let mut reader=image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
 let mut limits=image::Limits::default();limits.max_image_width=Some(8000);limits.max_image_height=Some(8000);limits.max_alloc=Some(64*1024*1024);reader.limits(limits);
 let img=reader.decode()?.thumbnail(512,320);let mut out=std::io::Cursor::new(Vec::new());img.write_to(&mut out,image::ImageFormat::Png)?;
 Ok(format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(out.into_inner())))
}
#[cfg(test)]
mod logo_tests {
 use super::*;
 #[test] fn logo_is_bounded_png_and_shared_by_preview_print_rows() {
  use base64::Engine;
  let img=image::DynamicImage::new_rgba8(1000,500);let mut bytes=std::io::Cursor::new(Vec::new());img.write_to(&mut bytes,image::ImageFormat::Png).unwrap();
  let logo=normalize_logo(bytes.get_ref()).unwrap();let png=base64::engine::general_purpose::STANDARD.decode(logo.strip_prefix("data:image/png;base64,").unwrap()).unwrap();let image=image::load_from_memory(&png).unwrap();assert_eq!((image.width(),image.height()),(512,256));
  let settings=HashMap::from([("receipt_logo_image".into(),logo.clone())]);let rows=receipt_layout::sale(&sample_receipt(),&settings);assert_eq!(rows[0].kind,"logo");assert_eq!(rows[0].text,logo);
  assert!(normalize_logo(b"invalid").is_err());assert!(normalize_logo(&vec![0;4*1024*1024+1]).is_err());
  assert!(receipt_layout::logo(&HashMap::from([("receipt_logo_image".into(),String::new())])).is_none());
 }
}
