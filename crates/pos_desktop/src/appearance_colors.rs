use super::*;

pub const FIELDS: [(&str, &str, &str); 9] = [
    ("ui_header_color", "Header", "#1d4ed8"),
    ("ui_status_color", "Status bar", "#1d4ed8"),
    ("ui_button_color", "Primary buttons", "#2563eb"),
    ("ui_hover_color", "Button hover", "#1e40af"),
    ("ui_focus_color", "Focus outline", "#2563eb"),
    ("ui_category_color", "Selected category / tab", "#2563eb"),
    ("ui_product_hover_color", "Product card hover", "#2563eb"),
    ("ui_sidebar_color", "Side menu", "#101827"),
    ("ui_icon_hover_color", "Icon hover", "#60a5fa"),
];
pub(crate) const PRESETS: [(&str, [&str; 9]); 9] = [
    (
        "Blue",
        [
            "#1d4ed8", "#1d4ed8", "#2563eb", "#1e40af", "#2563eb", "#2563eb", "#2563eb", "#172554", "#60a5fa",
        ],
    ),
    (
        "Teal",
        [
            "#0f766e", "#115e59", "#0f766e", "#134e4a", "#0d9488", "#0d9488", "#0d9488", "#134e4a", "#60a5fa",
        ],
    ),
    (
        "Green",
        [
            "#166534", "#14532d", "#15803d", "#14532d", "#16a34a", "#16a34a", "#16a34a", "#14532d", "#60a5fa",
        ],
    ),
    (
        "Rose",
        [
            "#be123c", "#9f1239", "#e11d48", "#881337", "#e11d48", "#e11d48", "#e11d48", "#881337", "#60a5fa",
        ],
    ),
    (
        "Graphite",
        [
            "#374151", "#1f2937", "#4b5563", "#111827", "#64748b", "#64748b", "#64748b", "#111827", "#60a5fa",
        ],
    ),
    (
        "Magenta",
        [
            "#a21caf", "#86198f", "#c026d3", "#86198f", "#c026d3", "#c026d3", "#c026d3", "#4a044e", "#60a5fa",
        ],
    ),
    (
        "Orange",
        [
            "#c2410c", "#9a3412", "#ea580c", "#9a3412", "#ea580c", "#ea580c", "#ea580c", "#431407", "#60a5fa",
        ],
    ),
    (
        "Sky Blue",
        [
            "#0284c7", "#0369a1", "#0ea5e9", "#0369a1", "#0284c7", "#0284c7", "#0284c7", "#082f49", "#60a5fa",
        ],
    ),
    (
        "Lavender",
        [
            "#a78bfa", "#8b5cf6", "#a78bfa", "#7c3aed", "#8b5cf6", "#8b5cf6", "#8b5cf6", "#2e1065", "#60a5fa",
        ],
    ),
];

pub fn valid(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

pub(crate) fn foreground(color: &str) -> &'static str {
    let rgb = u32::from_str_radix(&color[1..], 16).unwrap_or(0);
    let channel = |shift: u32| {
        let v = f64::from((rgb >> shift) & 255u32) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    if luminance > 0.179 {
        "#000000"
    } else {
        "#ffffff"
    }
}

pub fn style(settings: &HashMap<String, String>) -> String {
    let names = [
        "header",
        "status",
        "button",
        "hover",
        "focus",
        "category",
        "product-hover",
        "sidebar",
        "icon-hover",
    ];
    FIELDS
        .iter()
        .zip(names)
        .map(|((key, _, default), name)| {
            let color = settings
                .get(*key)
                .map(String::as_str)
                .filter(|v| valid(v))
                .or_else(|| {
                    matches!(*key, "ui_category_color" | "ui_product_hover_color")
                        .then(|| settings.get("ui_focus_color").map(String::as_str).filter(|v| valid(v)))
                        .flatten()
                })
                .unwrap_or(default);
            format!(
                "--shop-{name}:{color};--shop-{name}-text:{};",
                foreground(color)
            )
        })
        .collect()
}

#[component]
pub fn ColorPreferences(
    draft: Signal<HashMap<String, String>>,
    original: Vec<(String, String)>,
    saving: bool,
) -> Element {
    let mut values: HashMap<String, String> = original.into_iter().collect();
    values.extend(draft.read().clone());
    for key in ["ui_category_color", "ui_product_hover_color"] {
        if !values.get(key).is_some_and(|v| valid(v)) {
            let fallback = values.get("ui_focus_color").filter(|v| valid(v)).cloned().unwrap_or_else(|| "#2563eb".into());
            values.insert(key.into(), fallback);
        }
    }
    let preview = style(&values);
    rsx! {
        div {class:"shop_color_columns",
        fieldset { class: "shop_color_preferences",
            legend { "Shop colors" }
            div { class: "shop_color_presets",
                for (name, colors) in PRESETS {
                    button { r#type: "button", class: "shop_color_preset", disabled: saving,
                        title: name, aria_label: name,
                        aria_pressed: FIELDS.iter().zip(colors).all(|((key,_,default), color)| values.get(*key).map(String::as_str).filter(|v| valid(v)).unwrap_or(default).eq_ignore_ascii_case(color)),
                        onclick: move |_| {
                            let mut current = draft.write();
                            for ((key,_,_), color) in FIELDS.iter().zip(colors) { current.insert((*key).into(), color.into()); }
                        },
                        span { class: "shop_palette_swatch", style: "background:{colors[0]}", aria_hidden: "true" }
                        "{name}"
                    }
                }
            }
            div { class: "shop_color_fields",
                for (key, label, default) in FIELDS {
                    label {
                        span {class:"shop_color_field_text", "{label}"
                            code {"{values.get(key).map(String::as_str).filter(|v| valid(v)).unwrap_or(default)}"}
                        }
                        input { r#type: "color", disabled: saving, aria_label: label,
                            value: values.get(key).map(String::as_str).filter(|v| valid(v)).unwrap_or(default),
                            oninput: move |event| { let color = event.value(); if valid(&color) { draft.write().insert(key.into(),color); } }
                        }
                    }
                }
            }
        }
        section {class:"shop_color_preview_panel",aria_label:"Appearance preview",
            header {h3 {"Live Preview"} p {"Color changes appear here before saving."}}
            div { class: "shop_color_preview", style: preview,
                div { class: "shop_preview_header", "KAY POS"
                    button {r#type:"button",class:"shop_preview_icon_hover",title:"Preview icon hover",aria_label:"Preview icon hover",crate::icons::Icon {name:"refresh"}}
                }
                div { class: "shop_preview_sidebar", "Side menu" }
                div { class: "shop_preview_buttons",
                    button { r#type: "button", class: "shop_preview_primary", crate::icons::ActionLabel { label:"Save" } }
                    button { r#type: "button", class: "shop_preview_hover", "Hover" }
                    span { class: "shop_preview_category", "Selected category" }
                    span { class: "shop_preview_product", "Product hover" }
                }
                div { class: "shop_preview_status", "Database connected" }
            }
        }
        }
    }
}

#[test]
fn colors_validate_and_keep_text_readable() {
    assert!(valid("#12abEF"));
    assert!(!valid("red;display:none"));
    assert!(!valid("#123"));
    assert_eq!(foreground("#ffffff"), "#000000");
    assert_eq!(foreground("#000000"), "#ffffff");
    assert_eq!(foreground("#1d4ed8"), "#ffffff");
    assert!(style(&HashMap::from([(
        "ui_header_color".into(),
        "invalid".into()
    )]))
    .contains("--shop-header:#1d4ed8;"));
}

#[test]
fn icon_hover_color_is_configurable_and_validated() {
 assert!(style(&HashMap::new()).contains("--shop-icon-hover:#60a5fa;"));
 assert!(style(&HashMap::from([("ui_icon_hover_color".into(),"#ff8800".into())])).contains("--shop-icon-hover:#ff8800;"));
 assert!(style(&HashMap::from([("ui_icon_hover_color".into(),"invalid".into())])).contains("--shop-icon-hover:#60a5fa;"));
 assert!(crate::local_appearance::is_key("ui_icon_hover_color"));
}
