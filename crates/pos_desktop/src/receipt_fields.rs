use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FIELDS: [(&str, &str); 19] = [
    ("logo", "Logo"),
    ("title", "Shop name"),
    ("phone", "Phone"),
    ("address", "Address"),
    ("header", "Header"),
    ("invoice", "Receipt number"),
    ("payment", "Payment method"),
    ("customer", "Customer name"),
    ("customer_address", "Customer address"),
    ("items", "Items"),
    ("subtotal", "Subtotal"),
    ("tax", "Tax"),
    ("discount", "Discount"),
    ("total", "Total"),
    ("received", "Received"),
    ("change", "Change"),
    ("balance", "Balance due"),
    ("footer", "Footer"),
    ("message", "Footer message"),
];
pub const COLUMNS: [(&str, &str); 4] = [
    ("name", "Item"),
    ("qty", "Qty"),
    ("price", "Price"),
    ("amount", "Amount"),
];
pub fn defaults() -> HashMap<String, String> {
    let mut s = HashMap::from([
        (
            "receipt_field_order".into(),
            FIELDS.iter().map(|(k, _)| *k).collect::<Vec<_>>().join(","),
        ),
        ("receipt_item_layout".into(), "stacked".into()),
        ("receipt_pair_percent".into(), "55".into()),
        (
            "receipt_column_order".into(),
            "name,qty,price,amount".into(),
        ),
        ("receipt_templates".into(), "[]".into()),
    ]);
    for (k, _) in FIELDS {
        s.insert(format!("receipt_show_{k}"), if matches!(k,"customer"|"customer_address"){"0"}else{"1"}.into());
    }
    for ((k, _), width) in COLUMNS.into_iter().zip([40, 15, 20, 25]) {
        s.insert(format!("receipt_column_{k}"), width.to_string());
        s.insert(format!("receipt_show_column_{k}"), "1".into());
    }
    s
}
pub fn visible(s: &HashMap<String, String>, key: &str) -> bool {
    s.get(&format!("receipt_show_{key}"))
        .map(|v|v!="0").unwrap_or(!matches!(key,"customer"|"customer_address"))
}
fn ordered(s: &HashMap<String, String>, key: &str, known: &[(&str, &str)]) -> Vec<String> {
    let mut order = Vec::new();
    for k in s
        .get(key)
        .map(|v| v.as_str())
        .unwrap_or("")
        .split(',')
        .chain(known.iter().map(|(k, _)| *k))
    {
        if known.iter().any(|(x, _)| *x == k) && !order.iter().any(|x| x == k) {
            order.push(k.to_string());
        }
    }
    order
}
pub fn order(s: &HashMap<String, String>) -> Vec<String> {
    let mut values=ordered(s, "receipt_field_order", &FIELDS);
    for key in ["customer","customer_address"] {
        if s.get("receipt_field_order").is_some_and(|order|!order.split(',').any(|field|field==key)) {
            values.retain(|field|field!=key);
            let index=values.iter().position(|field|field=="items").unwrap_or(values.len());
            values.insert(index,key.into());
        }
    }
    values
}
pub fn columns(s: &HashMap<String, String>) -> Vec<(String, f64)> {
    ordered(s, "receipt_column_order", &COLUMNS)
        .into_iter()
        .filter(|k| visible(s, &format!("column_{k}")))
        .map(|k| {
            let w = crate::receipt_editor::number(s, &format!("receipt_column_{k}"), 25., 5., 80.);
            (k, w)
        })
        .collect()
}
pub fn move_key(s: &HashMap<String, String>, key: &str, field: &str, up: bool) -> String {
    let known = if key == "receipt_field_order" {
        &FIELDS[..]
    } else {
        &COLUMNS[..]
    };
    let mut values = ordered(s, key, known);
    if let Some(i) = values.iter().position(|v| v == field) {
        let next = if up {
            i.saturating_sub(1)
        } else {
            (i + 1).min(values.len() - 1)
        };
        values.swap(i, next);
    }
    values.join(",")
}
#[derive(Debug, Deserialize)]
pub struct DropRequest {
    pub domain: String,
    pub source: String,
    pub target: String,
    pub after: bool,
}
pub fn drop_key(s: &HashMap<String, String>, request: &DropRequest) -> Option<(String, String)> {
    let (key, known) = match request.domain.as_str() {
        "fields" => ("receipt_field_order", &FIELDS[..]),
        "columns" => ("receipt_column_order", &COLUMNS[..]),
        _ => return None,
    };
    if request.source == request.target
        || !known.iter().any(|(k, _)| *k == request.source)
        || !known.iter().any(|(k, _)| *k == request.target)
    {
        return None;
    }
    let mut values = ordered(s, key, known);
    values.retain(|k| k != &request.source);
    let target = values.iter().position(|k| k == &request.target)?;
    values.insert(target + usize::from(request.after), request.source.clone());
    let result = values.join(",");
    if result == ordered(s, key, known).join(",") {
        None
    } else {
        Some((key.into(), result))
    }
}
#[component]
pub fn DragHandle(domain: &'static str, field: String, label: String, disabled: bool) -> Element {
    rsx! {button {class:"receipt_drag_handle",r#type:"button",disabled,"data-receipt-handle":domain,"data-receipt-key":field.clone(),"data-receipt-label":label.clone(),aria_label:format!("Move {label}. Drag, or press Space and use arrow keys."),title:format!("Drag {label} to reorder"),svg {width:16,height:20,view_box:"0 0 16 20",fill:"currentColor","aria-hidden":"true",for (x,y) in [(5,5),(11,5),(5,10),(11,10),(5,15),(11,15)] {circle {cx:x,cy:y,r:1.5}}}}}
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub settings: HashMap<String, String>,
}
pub fn templates(s: &HashMap<String, String>) -> Vec<Template> {
    s.get("receipt_templates")
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or_default()
}
pub fn snapshot(s: &HashMap<String, String>) -> HashMap<String, String> {
    let mut values = crate::receipt_editor::defaults();
    for (k, v) in values.iter_mut() {
        if k != "receipt_templates" {
            if let Some(value) = s.get(k) {
                *v = value.clone();
            }
        }
    }
    values.remove("receipt_templates");
    values
}
pub fn save_template(s: &HashMap<String, String>, name: &str) -> anyhow::Result<String> {
    let name = name.trim();
    anyhow::ensure!(
        !name.is_empty() && name.chars().count() <= 60,
        "Template name must contain 1–60 characters"
    );
    crate::receipt_editor::validate(&snapshot(s))?;
    let mut list = templates(s);
    let template = Template {
        name: name.into(),
        settings: snapshot(s),
    };
    if let Some(old) = list.iter_mut().find(|t| t.name.eq_ignore_ascii_case(name)) {
        *old = template;
    } else {
        anyhow::ensure!(list.len() < 20, "Maximum 20 templates");
        list.push(template);
    }
    Ok(serde_json::to_string(&list)?)
}
pub fn builtin(name: &str, current: &HashMap<String, String>) -> HashMap<String, String> {
    let mut s = crate::receipt_editor::defaults();
    for k in [
        "receipt_paper_size",
        "receipt_printable_width",
        "receipt_margin_left",
        "receipt_margin_right",
        "receipt_margin_top",
        "receipt_margin_bottom",
    ] {
        if let Some(v) = current.get(k) {
            s.insert(k.into(), v.clone());
        }
    }
    s.remove("receipt_templates");
    if name == "compact" {
        for (block, _) in crate::receipt_editor::BLOCKS {
            s.insert(format!("receipt_{block}_gap"), "0.5".into());
        }
        s.insert("receipt_section_gap".into(), "1.5".into());
        s.insert("receipt_show_address".into(), "0".into());
        s.insert("receipt_show_header".into(), "0".into());
    }
    if name == "table" {
        s.insert("receipt_item_layout".into(), "table".into());
    }
    s
}
pub fn validate(s: &HashMap<String, String>) -> anyhow::Result<()> {
    for (k, v) in s {
        if k.starts_with("receipt_show_") {
            anyhow::ensure!(v == "0" || v == "1", "{k}: choose shown or hidden");
        }
        if k.starts_with("receipt_column_") && k != "receipt_column_order" {
            let n = v.parse::<f64>().unwrap_or(f64::NAN);
            anyhow::ensure!(
                n.is_finite() && (5.0..=80.0).contains(&n),
                "Column width must be 5–80"
            );
        }
    }
    if let Some(v) = s.get("receipt_pair_percent") {
        let n = v.parse::<f64>().unwrap_or(f64::NAN);
        anyhow::ensure!(
            n.is_finite() && (20.0..=80.0).contains(&n),
            "Label width must be 20–80 percent"
        );
    }
    if let Some(v) = s.get("receipt_item_layout") {
        anyhow::ensure!(
            ["stacked", "table"].contains(&v.as_str()),
            "Invalid item layout"
        );
    }
    for (key, known) in [
        ("receipt_field_order", &FIELDS[..]),
        ("receipt_column_order", &COLUMNS[..]),
    ] {
        if let Some(v) = s.get(key) {
            let parts = v.split(',').collect::<Vec<_>>();
            anyhow::ensure!(
                parts.iter().all(|part|known.iter().any(|(k,_)|k==part))
                    && parts.iter().all(|part|parts.iter().filter(|other|*other==part).count()==1)
                    && known.iter().all(|(k,_)| {
                        (key=="receipt_field_order" && matches!(*k,"customer"|"customer_address"))
                            || parts.contains(k)
                    }),
                "Invalid field/column order"
            );
        }
    }
    anyhow::ensure!(!columns(s).is_empty(), "Show at least one item column");
    if s.get("receipt_item_layout").is_some_and(|v| v == "table") {
        let columns = columns(s);
        let available = crate::receipt_editor::Geometry::from(s).content_mm()
            - 2. * (columns.len().saturating_sub(1)) as f64;
        let total: f64 = columns.iter().map(|(_, w)| w).sum();
        anyhow::ensure!(columns.iter().all(|(_,w)|available*w/total>=3.),"Item columns are too narrow. Increase printable width, adjust column weights or hide a column.");
    }

    if let Some(v) = s.get("receipt_templates") {
        anyhow::ensure!(v.len() <= 200_000, "Templates are too large");
        let list: Vec<Template> = serde_json::from_str(v)?;
        anyhow::ensure!(list.len() <= 20, "Maximum 20 templates");
        for t in list {
            anyhow::ensure!(
                !t.name.trim().is_empty() && t.name.chars().count() <= 60,
                "Invalid template name"
            );
            anyhow::ensure!(
                t.settings
                    .keys()
                    .all(|k| crate::receipt_editor::defaults().contains_key(k)
                        && k != "receipt_templates"),
                "Invalid template setting"
            );
            crate::receipt_editor::validate(&t.settings)?;
        }
    }
    Ok(())
}
#[component]
pub fn ReceiptFields(
    settings: HashMap<String, String>,
    mut draft: Signal<HashMap<String, String>>,
    disabled: bool,
) -> Element {
    let mut name = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let list = templates(&settings);
    let order = order(&settings);
    let column_order = ordered(&settings, "receipt_column_order", &COLUMNS);
    rsx! {
    input {r#type:"hidden","data-receipt-reorder-input":"true",disabled,oninput:{let settings=settings.clone();move |e|{if disabled{return;}if let Ok(request)=serde_json::from_str::<DropRequest>(&e.value()){if let Some((key,value))=drop_key(&settings,&request){draft.write().insert(key,value);notice.set("Layout reordered. Save settings to keep it.".into());}}}}}
    p {class:"receipt_drag_instructions","Drag the grip beside a field, column or preview block to reorder. Space → arrow keys → Enter also works. Save settings to keep changes."}
    div {class:"receipt_reorder_status",role:"status",aria_live:"polite","data-receipt-drag-status":"true"}
    details {class:"receipt_text_controls",open:true,summary {"Templates"}
    div {class:"receipt_template_presets",for (key,label) in [("standard","Standard"),("compact","Compact"),("table","Item columns")] {button {r#type:"button",disabled,onclick:{let settings=settings.clone();move |_|{draft.write().extend(builtin(key,&settings));notice.set("Template applied to preview. Save settings to keep it.".into());}},"{label}"}}}
    div {class:"receipt_template_save",input {placeholder:"Template name",aria_label:"Template name",maxlength:60,disabled,value:name(),oninput:move |e|name.set(e.value())} button {r#type:"button",disabled:disabled||name().trim().is_empty(),onclick:{let settings=settings.clone();move |_|{match save_template(&settings,&name()){Ok(json)=>{draft.write().insert("receipt_templates".into(),json);notice.set("Template stored in draft. Save settings to keep it.".into());},Err(e)=>notice.set(e.to_string())}}}, crate::icons::ActionLabel { label:"Store template" } }}
    for t in list {div {class:"receipt_template_row",span {"{t.name}"} button {r#type:"button",disabled,onclick:{let values=snapshot(&t.settings);move |_|{draft.write().extend(values.clone());notice.set("Template applied to preview. Save settings to keep it.".into());}}, crate::icons::ActionLabel { label:"Apply" } } button {class:"receipt_template_delete",r#type:"button",disabled,aria_label:format!("Delete template {}",t.name),onclick:{let settings=settings.clone();let template_name=t.name.clone();move |_|{let values=templates(&settings).into_iter().filter(|t|t.name!=template_name).collect::<Vec<_>>();draft.write().insert("receipt_templates".into(),serde_json::to_string(&values).unwrap());}},crate::icons::Icon {name:"delete"}}}}
    if !notice().is_empty(){small {role:"status","{notice}"}}
    }
    details {class:"receipt_text_controls",summary {"Fields & order"} small {"Show/hide fields; drag the grip or use the arrows. Empty fields stay omitted."}
    for (i,key) in order.iter().enumerate(){div {key:"{key}",class:"receipt_field_row","data-receipt-domain":"fields","data-receipt-key":key.clone(),DragHandle {domain:"fields",field:key.clone(),label:FIELDS.iter().find(|(k,_)|*k==key).unwrap().1,disabled} label {input {r#type:"checkbox",disabled,checked:visible(&settings,key),onchange:{let key=key.clone();move |e|{draft.write().insert(format!("receipt_show_{key}"),if e.checked(){"1"}else{"0"}.into());}}} span {"{FIELDS.iter().find(|(k,_)|*k==key).unwrap().1}"}}
    for (up,label) in [(true,"↑"),(false,"↓")] {button {r#type:"button",disabled:disabled||(up&&i==0)||(!up&&i+1==order.len()),aria_label:format!("Move {} {}",key,if up{"up"}else{"down"}),onclick:{let settings=settings.clone();let key=key.clone();move |_|{draft.write().insert("receipt_field_order".into(),move_key(&settings,"receipt_field_order",&key,up));}},"{label}"}}
    }} }
    details {class:"receipt_text_controls",summary {"Item columns"}
    label {span {"Item layout"} select {disabled,value:settings.get("receipt_item_layout").cloned().unwrap_or("stacked".into()),onchange:move |e|{draft.write().insert("receipt_item_layout".into(),e.value());},option {value:"stacked","Two lines per item"} option {value:"table","Columns"}}}
    label {span {"Totals / label width (%)"} input {r#type:"number",min:20,max:80,disabled,value:settings.get("receipt_pair_percent").cloned().unwrap_or("55".into()),oninput:move |e|{draft.write().insert("receipt_pair_percent".into(),e.value());}}}
    small {"Widths and column order apply to Columns layout; visibility also applies to two-line items. Widths are relative weights. Narrow paper may wrap long names."}
    for (i,key) in column_order.iter().enumerate(){div {key:"{key}",class:"receipt_column_row","data-receipt-domain":"columns","data-receipt-key":key.clone(),DragHandle {domain:"columns",field:key.clone(),label:COLUMNS.iter().find(|(k,_)|*k==key).unwrap().1,disabled} label {input {r#type:"checkbox",disabled,checked:visible(&settings,&format!("column_{key}")),onchange:{let key=key.clone();move |e|{draft.write().insert(format!("receipt_show_column_{key}"),if e.checked(){"1"}else{"0"}.into());}}} span {"{COLUMNS.iter().find(|(k,_)|*k==key).unwrap().1}"}}
    input {r#type:"number",aria_label:format!("{} column width",key),min:5,max:80,disabled,value:settings.get(&format!("receipt_column_{key}")).cloned().unwrap_or("25".into()),oninput:{let key=key.clone();move |e|{draft.write().insert(format!("receipt_column_{key}"),e.value());}}}
    for (up,label) in [(true,"↑"),(false,"↓")] {button {r#type:"button",disabled:disabled||(up&&i==0)||(!up&&i+1==column_order.len()),aria_label:format!("Move {} column {}",key,if up{"up"}else{"down"}),onclick:{let settings=settings.clone();let key=key.clone();move |_|{draft.write().insert("receipt_column_order".into(),move_key(&settings,"receipt_column_order",&key,up));}},"{label}"}}
    }} }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_receipt_orders_allow_customer_fields_without_resetting_layout() {
        let legacy=FIELDS.iter().filter(|(k,_)|!matches!(*k,"customer"|"customer_address")).map(|(k,_)|*k).collect::<Vec<_>>().join(",");
        let mut settings=defaults();
        settings.insert("receipt_field_order".into(),legacy.clone());
        settings.insert("receipt_show_customer_address".into(),"1".into());
        validate(&settings).unwrap();
        let upgraded=order(&settings);
        assert_eq!(upgraded.len(),FIELDS.len());
        assert_eq!(upgraded.iter().filter(|k|!matches!(k.as_str(),"customer"|"customer_address")).cloned().collect::<Vec<_>>().join(","),legacy);
        settings.insert("receipt_field_order".into(),upgraded.join(","));
        validate(&settings).unwrap();
        for bad in [legacy.replace("items,",""),format!("{legacy},items"),format!("{legacy},unknown")] {
            settings.insert("receipt_field_order".into(),bad);assert!(validate(&settings).is_err());
        }
        settings.insert("receipt_field_order".into(),legacy);
        settings.insert("receipt_column_order".into(),"name,qty,price".into());
        assert!(validate(&settings).is_err());
    }
    #[test]
    fn drag_drop_preserves_each_field_and_column_once() {
        let defaults = crate::receipt_editor::defaults();
        for (domain, key, known) in [
            ("fields", "receipt_field_order", &FIELDS[..]),
            ("columns", "receipt_column_order", &COLUMNS[..]),
        ] {
            for (source, _) in known {
                for (target, _) in known {
                    for after in [false, true] {
                        let request = DropRequest {
                            domain: domain.into(),
                            source: (*source).into(),
                            target: (*target).into(),
                            after,
                        };
                        let mut changed = defaults.clone();
                        if let Some((result_key, result)) = drop_key(&changed, &request) {
                            assert_eq!(result_key, key);
                            changed.insert(result_key, result);
                        }
                        validate(&changed).unwrap();
                        let values = ordered(&changed, key, known);
                        assert_eq!(values.len(), known.len());
                        for (field, _) in known {
                            assert_eq!(values.iter().filter(|k| k.as_str() == *field).count(), 1);
                        }
                        if source != target {
                            let source_index = values.iter().position(|k| k == source).unwrap();
                            let target_index = values.iter().position(|k| k == target).unwrap();
                            assert_eq!(
                                source_index,
                                if after {
                                    target_index + 1
                                } else {
                                    target_index - 1
                                }
                            );
                        }
                        let others = values
                            .iter()
                            .filter(|k| k.as_str() != *source)
                            .cloned()
                            .collect::<Vec<_>>();
                        assert_eq!(
                            others,
                            known
                                .iter()
                                .filter(|(k, _)| k != source)
                                .map(|(k, _)| k.to_string())
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn invalid_drop_and_self_drop_do_not_change_layout() {
        let settings = crate::receipt_editor::defaults();
        for (domain, source, target) in [
            ("fields", "title", "title"),
            ("fields", "title", "qty"),
            ("columns", "name", "total"),
            ("other", "title", "phone"),
        ] {
            assert!(drop_key(
                &settings,
                &DropRequest {
                    domain: domain.into(),
                    source: source.into(),
                    target: target.into(),
                    after: false
                }
            )
            .is_none());
        }
    }
    #[test]
    fn dragged_order_survives_template_round_trip() {
        let mut settings = crate::receipt_editor::defaults();
        let (key, value) = drop_key(
            &settings,
            &DropRequest {
                domain: "fields".into(),
                source: "message".into(),
                target: "title".into(),
                after: false,
            },
        )
        .unwrap();
        settings.insert(key, value);
        let encoded = save_template(&settings, "Dragged layout").unwrap();
        settings.insert("receipt_templates".into(), encoded);
        let restored = templates(&settings).pop().unwrap().settings;
        assert_eq!(order(&restored), order(&settings));
        assert_eq!(order(&restored)[1], "message");
        crate::receipt_editor::validate(&restored).unwrap();
    }
    #[test]
    fn template_round_trip_preserves_layout_without_shop_data() {
        let mut s = crate::receipt_editor::defaults();
        s.insert("shop_name".into(), "Private shop".into());
        s.insert("receipt_logo_image".into(), "image".into());
        s.insert("receipt_printer_name".into(), "Printer".into());
        s.insert("receipt_pair_percent".into(), "70".into());
        s.insert("receipt_item_layout".into(), "table".into());
        s.insert(
            "receipt_templates".into(),
            save_template(&s, "My layout").unwrap(),
        );
        crate::receipt_editor::validate(&s).unwrap();
        let t = templates(&s).pop().unwrap();
        assert_eq!(t.settings["receipt_pair_percent"], "70");
        assert_eq!(t.settings["receipt_item_layout"], "table");
        assert!(!t.settings.contains_key("shop_name"));
        assert!(!t.settings.contains_key("receipt_logo_image"));
        assert!(!t.settings.contains_key("receipt_printer_name"));
        assert!(!t.settings.contains_key("receipt_templates"));
        s.insert(
            "receipt_templates".into(),
            save_template(&s, "MY LAYOUT").unwrap(),
        );
        assert_eq!(templates(&s).len(), 1);
    }
    #[test]
    fn invalid_columns_order_and_templates_are_rejected() {
        let mut s = crate::receipt_editor::defaults();
        for (k, _) in COLUMNS {
            s.insert(format!("receipt_show_column_{k}"), "0".into());
        }
        assert!(crate::receipt_editor::validate(&s).is_err());
        s.insert("receipt_show_column_name".into(), "1".into());
        s.insert("receipt_field_order".into(), "total,total".into());
        assert!(crate::receipt_editor::validate(&s).is_err());
        s.insert(
            "receipt_field_order".into(),
            defaults()["receipt_field_order"].clone(),
        );
        s.insert(
            "receipt_templates".into(),
            r#"[{"name":"bad","settings":{"receipt_templates":"[]"}}]"#.into(),
        );
        assert!(crate::receipt_editor::validate(&s).is_err());
    }
    #[test]
    fn builtin_keeps_custom_paper_and_template_library() {
        let mut s = crate::receipt_editor::defaults();
        s.insert("receipt_paper_size".into(), "76.5".into());
        s.insert("receipt_printable_width".into(), "65".into());
        let t = builtin("table", &s);
        assert_eq!(t["receipt_paper_size"], "76.5");
        assert_eq!(t["receipt_item_layout"], "table");
        assert!(!t.contains_key("receipt_templates"));
        crate::receipt_editor::validate(&t).unwrap();
    }
}

#[component]
pub fn CustomerVisibility(original:Vec<(String,String)>,mut draft:Signal<HashMap<String,String>>,disabled:bool)->Element {
    let mut settings:HashMap<String,String>=original.into_iter().collect();
    settings.extend(draft.read().clone());
    rsx! {fieldset {class:"receipt_customer_visibility",
        legend {"Customer on receipt"}
        for (key,label) in [("customer","Show customer name"),("customer_address","Show customer address")] {
            label {input {r#type:"checkbox",disabled,checked:visible(&settings,key),onchange:move |event|{draft.write().insert(format!("receipt_show_{key}"),if event.checked(){"1"}else{"0"}.into());}} span {"{label}"}}
        }
        small {"Select a customer at checkout. Empty names or addresses are omitted. Save settings to apply."}
    }}
}
