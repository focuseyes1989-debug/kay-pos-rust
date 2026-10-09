use dioxus::prelude::*;
use serde::Serialize;
use std::collections::HashMap;

pub const BLOCKS: [(&str, &str); 8] = [
    ("title", "Shop name"),
    ("phone", "Phone"),
    ("address", "Address"),
    ("header", "Receipt header"),
    ("body", "Items / other text"),
    ("total", "Total"),
    ("footer", "Receipt footer"),
    ("message", "Footer message"),
];
pub fn defaults() -> HashMap<String, String> {
    let mut values = HashMap::from([
        ("receipt_paper_size".into(), "80".into()),
        ("receipt_printable_width".into(), "72".into()),
        ("receipt_margin_left".into(), "3".into()),
        ("receipt_margin_right".into(), "3".into()),
        ("receipt_margin_top".into(), "3".into()),
        ("receipt_margin_bottom".into(), "3".into()),
        ("receipt_logo_width".into(), "36".into()),
        ("receipt_logo_height".into(), "24".into()),
        ("receipt_logo_align".into(), "center".into()),
        ("receipt_logo_gap".into(), "2".into()),
        ("receipt_section_gap".into(), "3".into()),
    ]);
    for (key, _) in BLOCKS {
        for (suffix, value) in [
            (
                "font_size",
                if key == "title" {
                    "13"
                } else if key == "total" {
                    "11"
                } else {
                    "9"
                },
            ),
            (
                "bold",
                if key == "title" || key == "total" {
                    "1"
                } else {
                    "0"
                },
            ),
            (
                "align",
                if ["title", "phone", "address", "header", "footer", "message"].contains(&key) {
                    "center"
                } else {
                    "left"
                },
            ),
            ("line_height", "1.5"),
            ("gap", "1"),
        ] {
            values.insert(format!("receipt_{key}_{suffix}"), value.into());
        }
    }
    values.extend(crate::receipt_fields::defaults());
    values
}
pub fn number(s: &HashMap<String, String>, key: &str, default: f64, min: f64, max: f64) -> f64 {
    s.get(key)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n >= min && *n <= max)
        .unwrap_or(default)
}
#[derive(Clone, Serialize)]
pub struct Geometry {
    pub paper_mm: f64,
    pub printable_mm: f64,
    pub left_mm: f64,
    pub right_mm: f64,
    pub top_mm: f64,
    pub bottom_mm: f64,
    pub logo_width_mm: f64,
    pub logo_height_mm: f64,
    pub logo_align: String,
    pub logo_gap_mm: f64,
    pub section_gap_mm: f64,
}
impl Geometry {
    pub fn from(s: &HashMap<String, String>) -> Self {
        let paper = number(s, "receipt_paper_size", 80., 40., 120.);
        let printable = number(
            s,
            "receipt_printable_width",
            if paper <= 58. { 48. } else { paper - 8. },
            20.,
            paper,
        );
        let left = number(s, "receipt_margin_left", 3., 0., 20.);
        let right = number(s, "receipt_margin_right", 3., 0., 20.);
        Self {
            paper_mm: paper,
            printable_mm: printable,
            left_mm: left,
            right_mm: right,
            top_mm: number(s, "receipt_margin_top", 3., 0., 20.),
            bottom_mm: number(s, "receipt_margin_bottom", 3., 0., 20.),
            logo_width_mm: number(s, "receipt_logo_width", 36., 5., 120.)
                .min((printable - left - right).max(10.)),
            logo_height_mm: number(s, "receipt_logo_height", 24., 5., 80.),
            logo_align: s
                .get("receipt_logo_align")
                .filter(|v| ["left", "center", "right"].contains(&v.as_str()))
                .cloned()
                .unwrap_or("center".into()),
            logo_gap_mm: number(s, "receipt_logo_gap", 2., 0., 20.),
            section_gap_mm: number(s, "receipt_section_gap", 3., 0., 20.),
        }
    }
    pub fn content_mm(&self) -> f64 {
        (self.printable_mm - self.left_mm - self.right_mm).max(10.)
    }
}
pub fn validate(s: &HashMap<String, String>) -> anyhow::Result<()> {
    let defaults = defaults();
    for (key, value) in s.iter().filter(|(k, _)| defaults.contains_key(*k)) {
        if crate::receipt_fields::defaults().contains_key(key) {
            continue;
        }
        if key.ends_with("align") {
            anyhow::ensure!(
                ["left", "center", "right"].contains(&value.as_str()),
                "Invalid alignment"
            );
            continue;
        }
        let n = value
            .parse::<f64>()
            .map_err(|_| anyhow::anyhow!("Invalid value: {key}"))?;
        let (min, max) = if key.ends_with("font_size") {
            (6., 24.)
        } else if key.ends_with("line_height") {
            (1., 3.)
        } else if key.ends_with("bold") {
            (0., 1.)
        } else if key == "receipt_paper_size" {
            (40., 120.)
        } else if key == "receipt_printable_width" {
            (20., 120.)
        } else if key == "receipt_logo_width" {
            (5., 120.)
        } else if key == "receipt_logo_height" {
            (5., 80.)
        } else {
            (0., 20.)
        };
        anyhow::ensure!(
            n.is_finite() && n >= min && n <= max,
            "{key}: enter {min}–{max}"
        );
    }
    let g = Geometry::from(s);
    let requested = number(s, "receipt_printable_width", g.printable_mm, 20., 120.);
    anyhow::ensure!(
        requested <= g.paper_mm,
        "Printable width must not exceed paper width"
    );
    anyhow::ensure!(
        g.printable_mm - g.left_mm - g.right_mm >= 10.,
        "Margins leave too little space for receipt text"
    );
    crate::receipt_fields::validate(s)?;
    Ok(())
}
#[component]
pub fn ReceiptEditor(
    original: Vec<(String, String)>,
    mut draft: Signal<HashMap<String, String>>,
    disabled: bool,
) -> Element {
    let mut settings: HashMap<String, String> = original.into_iter().collect();
    settings.extend(draft.read().clone());
    let mut print_busy = use_signal(|| false);
    let mut notice = use_signal(String::new);
    let value = |key: &str| {
        settings
            .get(key)
            .cloned()
            .or_else(|| defaults().get(key).cloned())
            .unwrap_or_default()
    };
    rsx! {fieldset {class:"receipt_editor",legend {"Receipt Editor"}
     div {class:"receipt_editor_actions",
      button {r#type:"button",disabled:disabled||print_busy(),onclick:move |_|{{let mut values=defaults();values.remove("receipt_templates");draft.write().extend(values);}notice.set("Default layout restored. Save to apply.".into());}, crate::icons::ActionLabel { label:"Reset layout" } }
      button {r#type:"button",disabled:disabled||print_busy(),onclick:{let settings=settings.clone();move |_|{if let Err(e)=validate(&settings){notice.set(e.to_string());return;}let settings=settings.clone();print_busy.set(true);notice.set("Sending test receipt…".into());spawn(async move{let result=crate::receipt_printer::print(crate::receipt_preview::sample_receipt(),settings).await;notice.set(match result {Ok(())=>"Test receipt sent to printer".into(),Err(e)=>format!("{e:#}")});print_busy.set(false);});}}, crate::icons::ActionLabel { label:"Test Print" } }
     }
     if !notice().is_empty(){p {role:"status","{notice}"}}
     small {"All widths and gaps are in mm. Printer driver must support the selected paper width. Logo keeps its aspect ratio."}
     div {class:"receipt_editor_grid",
      label {span {"Paper preset"} select {disabled,value:if ["58","80","112"].contains(&value("receipt_paper_size").as_str()){value("receipt_paper_size")}else{"custom".into()},onchange:move |e|{let width=e.value();if width!="custom"{let printable=match width.as_str(){"58"=>"48","80"=>"72",_=>"104"};let mut v=draft.write();v.insert("receipt_paper_size".into(),width);v.insert("receipt_printable_width".into(),printable.into());}},option {value:"58","58 mm"} option {value:"80","80 mm"} option {value:"112","112 mm"} option {value:"custom","Custom (edit width below)"}}}
      for (key,label,min,max) in [("receipt_paper_size","Paper width",40,120),("receipt_printable_width","Printable width",20,120),("receipt_margin_left","Left margin",0,20),("receipt_margin_right","Right margin",0,20),("receipt_margin_top","Top margin",0,20),("receipt_margin_bottom","Bottom margin",0,20),("receipt_logo_width","Logo max width",5,120),("receipt_logo_height","Logo max height",5,80),("receipt_logo_gap","Logo gap",0,20),("receipt_section_gap","Section gap",0,20)] {label {span {"{label}"} input {r#type:"number",min:"{min}",max:"{max}",step:"0.5",disabled,value:value(key),oninput:move |e|{draft.write().insert(key.into(),e.value());}}}}
      label {span {"Logo alignment"} select {disabled,value:value("receipt_logo_align"),onchange:move |e|{draft.write().insert("receipt_logo_align".into(),e.value());},for name in ["left","center","right"] {option {value:name,"{name}"}}}}
     }
     crate::receipt_fields::ReceiptFields {settings:settings.clone(),draft,disabled:disabled||print_busy()}
     for (block,label) in BLOCKS {details {class:"receipt_text_controls",summary {"{label}"} div {class:"receipt_editor_grid",
      for (suffix,name,min,max,step) in [("font_size","Font size (pt)",6.,24.,0.5),("line_height","Line spacing",1.,3.,0.1),("gap","Gap after (mm)",0.,20.,0.5)] {{let key=format!("receipt_{block}_{suffix}");rsx!{label {span {"{name}"} input {r#type:"number",min:"{min}",max:"{max}",step:"{step}",disabled,value:value(&key),oninput:move |e|{draft.write().insert(key.clone(),e.value());}}}}}
      label {span {"Alignment"} select {disabled,value:value(&format!("receipt_{block}_align")),onchange:move |e|{draft.write().insert(format!("receipt_{block}_align"),e.value());},for name in ["left","center","right"] {option {value:name,"{name}"}}}}
      label {class:"receipt_bold_toggle",input {r#type:"checkbox",disabled,checked:value(&format!("receipt_{block}_bold"))=="1",onchange:move |e|{draft.write().insert(format!("receipt_{block}_bold"),if e.checked(){"1"}else{"0"}.into());}} span {"Bold"}}
     }}}
    }}}
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_and_validation() {
        let mut s = defaults();
        validate(&s).unwrap();
        s.insert("receipt_paper_size".into(), "112".into());
        s.insert("receipt_printable_width".into(), "104".into());
        assert_eq!(Geometry::from(&s).content_mm(), 98.);
        s.insert("receipt_printable_width".into(), "120".into());
        assert!(validate(&s).is_err());
        s.insert("receipt_printable_width".into(), "104".into());
        s.insert("receipt_title_font_size".into(), "NaN".into());
        assert!(validate(&s).is_err());
    }
}
