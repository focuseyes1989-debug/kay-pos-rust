use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const KEY: &str = "ui_theme_editor_v1";
pub const LIBRARY_KEY: &str = "ui_theme_preset_library_v1";
type Palette = BTreeMap<String, Paint>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Paint {
    color: String,
    end: String,
    kind: String,
    angle: u16,
}
impl Default for Paint {
    fn default() -> Self {
        Self {
            color: "#2563eb".into(),
            end: "#7c3aed".into(),
            kind: "solid".into(),
            angle: 135,
        }
    }
}
impl Paint {
    fn solid(color: &str) -> Self {
        Self {
            color: color.into(),
            end: color.into(),
            ..Self::default()
        }
    }
    fn css(&self) -> String {
        match self.kind.as_str() {
            "linear" => format!(
                "linear-gradient({}deg,{}, {})",
                self.angle, self.color, self.end
            ),
            "radial" => format!("radial-gradient(circle,{}, {})", self.color, self.end),
            _ => self.color.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    name: String,
    light: Palette,
    dark: Palette,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    light: Palette,
    dark: Palette,
    presets: Vec<Preset>,
}

// id, label, group, light default, dark default, supports gradients
const FIELDS: &[(&str, &str, &str, &str, &str, bool)] = &[
    (
        "bg",
        "Workspace background",
        "Surfaces",
        "#eef2f7",
        "#101827",
        true,
    ),
    ("panel", "Panels", "Surfaces", "#ffffff", "#1c283b", true),
    (
        "input",
        "Input background",
        "Surfaces",
        "#ffffff",
        "#172337",
        true,
    ),
    ("modal", "Dialogs", "Surfaces", "#ffffff", "#1c283b", true),
    (
        "table",
        "Table heading",
        "Surfaces",
        "#f1f5f9",
        "#243249",
        true,
    ),
    (
        "ink",
        "Main text",
        "Text & borders",
        "#172033",
        "#e6edf7",
        false,
    ),
    (
        "muted",
        "Secondary text",
        "Text & borders",
        "#687386",
        "#a8b8cf",
        false,
    ),
    (
        "line",
        "Borders / dividers",
        "Text & borders",
        "#dbe2ea",
        "#3a4a62",
        false,
    ),
    (
        "focus",
        "Keyboard focus",
        "Text & borders",
        "#2563eb",
        "#60a5fa",
        false,
    ),
    (
        "price",
        "Product prices",
        "Text & borders",
        "#1d4ed8",
        "#93c5fd",
        false,
    ),
    (
        "header",
        "Menu bar",
        "Navigation",
        "#1d4ed8",
        "#1d4ed8",
        true,
    ),
    (
        "header-text",
        "Menu bar text",
        "Navigation",
        "#ffffff",
        "#ffffff",
        false,
    ),
    (
        "sidebar",
        "Side menu",
        "Navigation",
        "#101827",
        "#101827",
        true,
    ),
    (
        "sidebar-text",
        "Side menu text",
        "Navigation",
        "#ffffff",
        "#ffffff",
        false,
    ),
    (
        "status",
        "Status bar",
        "Navigation",
        "#1d4ed8",
        "#1d4ed8",
        true,
    ),
    (
        "status-text",
        "Status bar text",
        "Navigation",
        "#ffffff",
        "#ffffff",
        false,
    ),
    (
        "nav-selected",
        "Selected menu item",
        "Navigation",
        "#456de0",
        "#456de0",
        true,
    ),
    (
        "nav-text",
        "Selected menu text",
        "Navigation",
        "#ffffff",
        "#ffffff",
        false,
    ),
    (
        "icon-hover",
        "Icon hover",
        "Navigation",
        "#60a5fa",
        "#60a5fa",
        false,
    ),
    (
        "button",
        "Primary buttons",
        "Buttons & selection",
        "#2563eb",
        "#2563eb",
        true,
    ),
    (
        "button-text",
        "Primary button text",
        "Buttons & selection",
        "#ffffff",
        "#ffffff",
        false,
    ),
    (
        "hover",
        "Button hover",
        "Buttons & selection",
        "#1e40af",
        "#1e40af",
        true,
    ),
    (
        "hover-text",
        "Button hover text",
        "Buttons & selection",
        "#ffffff",
        "#ffffff",
        false,
    ),
    (
        "category",
        "Selected category / tab",
        "Buttons & selection",
        "#e8efff",
        "#244773",
        true,
    ),
    (
        "category-text",
        "Selected category text",
        "Buttons & selection",
        "#1d4ed8",
        "#bfdbfe",
        false,
    ),
    (
        "selected",
        "Selected list row",
        "Buttons & selection",
        "#eef5ff",
        "#244773",
        true,
    ),
    (
        "product",
        "Product cards",
        "Sales",
        "#ffffff",
        "#1c283b",
        true,
    ),
    (
        "product-hover",
        "Product card hover",
        "Sales",
        "#eff6ff",
        "#243d60",
        true,
    ),
    (
        "cart",
        "Cart background",
        "Sales",
        "#f8fafc",
        "#172337",
        true,
    ),
    ("cart-row", "Cart item", "Sales", "#ffffff", "#1c283b", true),
    (
        "good",
        "Success text",
        "Status colors",
        "#16834a",
        "#4ade80",
        false,
    ),
    (
        "warning",
        "Warning text",
        "Status colors",
        "#92400e",
        "#fcd34d",
        false,
    ),
    (
        "bad",
        "Error text",
        "Status colors",
        "#c02f35",
        "#fca5a5",
        false,
    ),
];
const GROUPS: &[&str] = &[
    "Surfaces",
    "Text & borders",
    "Navigation",
    "Buttons & selection",
    "Sales",
    "Status colors",
];

fn tint(color: &str, background: &str, percent: u32) -> String {
    let rgb = u32::from_str_radix(&color[1..], 16).unwrap();
    let bg = u32::from_str_radix(&background[1..], 16).unwrap();
    let channel = |shift: u32| {
        (((rgb >> shift) & 255u32) * percent + ((bg >> shift) & 255u32) * (100 - percent)) / 100
    };
    format!("#{:02x}{:02x}{:02x}", channel(16), channel(8), channel(0))
}

fn palette(settings: &HashMap<String, String>, dark: bool) -> Palette {
    let mut result: Palette = FIELDS
        .iter()
        .map(|(id, _, _, light, night, _)| {
            ((*id).into(), Paint::solid(if dark { night } else { light }))
        })
        .collect();
    for (key, _, _) in appearance_colors::FIELDS {
        let id = key
            .trim_start_matches("ui_")
            .trim_end_matches("_color")
            .replace('_', "-");
        if let Some(color) = settings.get(key).filter(|v| appearance_colors::valid(v)) {
            let fill = if matches!(id.as_str(), "category" | "product-hover") {
                tint(
                    color,
                    if dark { "#1c283b" } else { "#ffffff" },
                    if dark { 30 } else { 12 },
                )
            } else {
                color.clone()
            };
            result.insert(id.clone(), Paint::solid(&fill));
            let text_id = format!("{id}-text");
            if result.contains_key(&text_id) {
                result.insert(text_id, Paint::solid(appearance_colors::foreground(&fill)));
            }
        }
    }
    result
}

fn valid_palette(p: &Palette) -> bool {
    p.len() <= FIELDS.len()
        && p.iter().all(|(key, paint)| {
            FIELDS.iter().find(|f| f.0 == key).is_some_and(|f| {
                appearance_colors::valid(&paint.color)
                    && appearance_colors::valid(&paint.end)
                    && paint.angle <= 360
                    && (paint.kind == "solid"
                        || (f.5 && matches!(paint.kind.as_str(), "linear" | "radial")))
            })
        })
}

fn parse(value: &str) -> Result<Config, String> {
    if value.len() > 512_000 {
        return Err("Appearance presets are too large.".into());
    }
    let config: Config =
        serde_json::from_str(value).map_err(|_| "Invalid appearance settings.".to_string())?;
    if !valid_palette(&config.light) || !valid_palette(&config.dark) || config.presets.len() > 40 {
        return Err("Invalid theme colors or too many presets (maximum 40).".into());
    }
    let mut names = std::collections::HashSet::new();
    for preset in &config.presets {
        if preset.name.trim().is_empty()
            || preset.name.chars().count() > 60
            || !names.insert(preset.name.trim().to_lowercase())
            || !valid_palette(&preset.light)
            || !valid_palette(&preset.dark)
        {
            return Err("Preset names must be unique and no longer than 60 characters.".into());
        }
    }
    Ok(config)
}
pub fn validate(value: &str) -> Result<(), String> {
    parse(value).map(|_| ())
}
fn library(value: &str) -> Result<Vec<Preset>, String> {
    if value.len() > 512_000 {
        return Err("Appearance presets are too large.".into());
    }
    let presets: Vec<Preset> =
        serde_json::from_str(value).map_err(|_| "Invalid preset library.".to_string())?;
    let config = Config {
        presets,
        ..Config::default()
    };
    parse(&serde_json::to_string(&config).map_err(|e| e.to_string())?).map(|c| c.presets)
}
pub fn validate_library(value: &str) -> Result<(), String> {
    library(value).map(|_| ())
}

fn save_library_at(
    presets: &[Preset],
    save: impl FnOnce(&[(String, String)]) -> anyhow::Result<()>,
) -> Result<(String, String), String> {
    let value = serde_json::to_string(presets).map_err(|e| e.to_string())?;
    validate_library(&value)?;
    let entry = (LIBRARY_KEY.to_string(), value);
    save(std::slice::from_ref(&entry)).map_err(|e| format!("Could not save presets: {e:#}"))?;
    Ok(entry)
}
fn persist_presets(
    mut draft: Signal<HashMap<String, String>>,
    config: &Config,
    on_saved: EventHandler<HashMap<String, String>>,
) -> Result<(), String> {
    let (key, value) = save_library_at(&config.presets, local_appearance::save)?;
    draft.write().insert(key.clone(), value.clone());
    on_saved.call(HashMap::from([(key, value)]));
    Ok(())
}

fn resolved(settings: &HashMap<String, String>) -> Config {
    let mut config = settings
        .get(KEY)
        .and_then(|v| parse(v).ok())
        .unwrap_or_default();
    if let Some(presets) = settings.get(LIBRARY_KEY).and_then(|v| library(v).ok()) {
        config.presets = presets;
    }
    let mut light = palette(settings, false);
    light.extend(config.light);
    config.light = light;
    let mut dark = palette(settings, true);
    dark.extend(config.dark);
    config.dark = dark;
    config
}
fn builtin(colors: [&str; 9], gradient: bool) -> Config {
    let legacy = appearance_colors::FIELDS
        .iter()
        .zip(colors)
        .map(|((key, _, _), color)| ((*key).to_string(), color.to_string()))
        .collect();
    let mut c = resolved(&legacy);
    if gradient {
        for p in [&mut c.light, &mut c.dark] {
            for id in ["header", "status", "button"] {
                let paint = p.get_mut(id).unwrap();
                paint.kind = "linear".into();
                paint.end = colors[3].into();
            }
        }
    }
    c
}
fn write(mut draft: Signal<HashMap<String, String>>, config: Config) {
    if let Ok(value) = serde_json::to_string(&config) {
        draft.write().insert(KEY.into(), value);
    }
}

fn variables(p: &Palette) -> String {
    let mut css = String::new();
    for (id, paint) in p {
        css.push_str(&format!(
            "--ae-{id}:{};--ae-{id}-color:{};",
            paint.css(),
            paint.color
        ));
    }
    for (id, var) in [
        ("bg", "bg"),
        ("panel", "panel"),
        ("ink", "ink"),
        ("muted", "muted"),
        ("line", "line"),
        ("good", "good"),
        ("bad", "bad"),
        ("button", "blue"),
        ("selected", "list-card-selected"),
        ("product-hover", "list-card-hover"),
    ] {
        css.push_str(&format!("--{var}:{};", p[id].color));
    }
    for id in [
        "header",
        "header-text",
        "status",
        "status-text",
        "button",
        "button-text",
        "hover",
        "hover-text",
        "focus",
        "category",
        "category-text",
        "product-hover",
        "sidebar",
        "sidebar-text",
        "icon-hover",
    ] {
        css.push_str(&format!("--shop-{id}:{};", p[id].color));
    }
    css
}

// Colors stay separate from background paints: gradients never enter border/color CSS.
fn rules(scope: &str) -> String {
    let mut result = format!("{scope}{{background:var(--ae-bg);color:var(--ink);}}");
    for (selector, declarations) in [
        (".topbar", "background:var(--ae-header);color:var(--ae-header-text)"),
        (".app_status_bar", "background:var(--ae-status);color:var(--ae-status-text)"),
        (".side_menu", "background:var(--ae-sidebar);color:var(--ae-sidebar-text)"),
        (".side_menu .side_menu_profile", "background:transparent;color:var(--ae-sidebar-text)"),
        (".topbar .top_nav_link[aria-current=page]", "background:var(--ae-nav-selected);color:var(--ae-nav-text);border-color:var(--ae-nav-selected-color)"),
        (".topbar .top_nav_link[aria-current=page]:hover:not(:disabled)", "background:var(--ae-nav-selected);color:var(--ae-nav-text);border-color:var(--ae-nav-selected-color)"),
        (":is(.panel,.settings_card,.products_items,.customer_list,.customer_detail,.phase4_panel,.phase6_panel)", "background:var(--ae-panel);border-color:var(--line);color:var(--ink)"),
        (":is(.workspace,.customers_touch,.products_touch,.expense_touch,.reports_page,.purchases_page,.phase4_page,.phase6_page,.dashboard_page,.employees_page)", "background:var(--ae-bg);color:var(--ink)"),
        (":is(input:not([type=color]):not([type=checkbox]):not([type=radio]):not(:is(.settings_search,.catalog_search,.category_search) input),select,textarea)", "background:var(--ae-input);border-color:var(--line);color:var(--ink)"),
        (":is(.settings_search,.catalog_search,.category_search)", "background:var(--ae-input);border-color:var(--line)"),
        (":is(.settings_search,.catalog_search,.category_search):focus-within", "border-color:var(--shop-focus)"),
        (":is(.settings_search,.catalog_search,.category_search) input", "background-color:transparent;border:0;box-shadow:none"),
        (":is(.checkout_dialog,.receipt_dialog,.customer_dialog,.product_editor,.service_dialog,.message_box,[role=dialog])", "background:var(--ae-modal);color:var(--ink);border-color:var(--line)"),
        ("table thead th", "background:var(--ae-table);color:var(--ink);border-color:var(--line)"),
        (":is(.primary,.customer_primary,.settings_save,.users_add,.checkout:not(:disabled))", "background:var(--ae-button);color:var(--ae-button-text);border-color:var(--ae-button-color)"),
        ("button.category.active", "background:var(--ae-category);color:var(--ae-category-text);border-color:var(--ae-category-color)"),
        (":is(.receipt_tabs,.inventory_action_tabs,.settings_sidebar nav) button.active", "background:var(--ae-category);color:var(--ae-category-text);border-color:var(--ae-category-color)"),
        (":is(.transaction_item.active,.customer_list_item.selected,.inventory_product.selected,.settings_table_row.active)", "background:var(--ae-selected);color:var(--ink);border-color:var(--line)"),
        ("button.product_card", "background:var(--ae-product);color:var(--ink);border-color:var(--line)"),
        ("button.product_card:hover:not(:disabled)", "background:var(--ae-product-hover);border-color:var(--ae-product-hover-color)"),
        (".cart", "background:var(--ae-cart);color:var(--ink)"),
        (".cart .cart_line", "background:var(--ae-cart-row);border-color:var(--line)"),
        (".product_price", "color:var(--ae-price)"),
        (":is(.stock_alert_low,.warning)", "color:var(--ae-warning)"),
        (":is(.stock_alert_out,.error,.cart_quantity_error)", "color:var(--bad)"),
    ] { result.push_str(&format!("{scope} {selector}{{{declarations};}}")); }
    result.push_str(&format!("{scope} button:not(:disabled):not(.top_nav_link):not(.danger):not(.product_delete):not(.expense_delete):not(.users_delete):not(.shop_color_preset):not(.product_card):not(.inventory_product):not(.category.active):not(.customer_list_item):not(.side_group > button):not(.date_range_picker button):hover{{background:var(--ae-hover);color:var(--ae-hover-text);border-color:var(--ae-hover-color);}}"));
    result
}

pub fn stylesheet(settings: &HashMap<String, String>) -> String {
    if !settings.get(KEY).is_some_and(|v| validate(v).is_ok()) {
        return String::new();
    }
    let config = resolved(settings);
    let light = variables(&config.light);
    let dark = variables(&config.dark);
    // The shell's legacy inline variables must be superseded only when the editor is saved.
    let light = important_variables(&light);
    let dark = important_variables(&dark);
    format!(".shell[data-theme=light],.shell[data-theme=system]{{{light}}}.shell[data-theme=dark]{{{dark}}}@media(prefers-color-scheme:dark){{.shell[data-theme=system]{{{dark}}}}}{}", rules(".shell[data-theme]"))
}
fn important_variables(value: &str) -> String {
    value.replace(';', "!important;")
}

#[component]
pub fn Editor(
    draft: Signal<HashMap<String, String>>,
    original: Vec<(String, String)>,
    saving: bool,
    on_saved: EventHandler<HashMap<String, String>>,
) -> Element {
    let mut mode = use_signal(|| "light".to_string());
    let mut group = use_signal(|| "Surfaces".to_string());
    let mut preset_name = use_signal(String::new);
    let mut selected = use_signal(String::new);
    let mut feedback = use_signal(String::new);
    let settings = use_memo(use_reactive((&original,), move |(original,)| {
        let mut values: HashMap<String, String> = original.iter().cloned().collect();
        values.extend(draft.read().clone());
        values
    }));
    let config = resolved(&settings.read());
    let colors = if mode() == "dark" {
        &config.dark
    } else {
        &config.light
    };
    let preview = format!("{}color-scheme:{};", variables(colors), mode());
    let raw_error = settings
        .read()
        .get(KEY)
        .filter(|v| !v.trim().is_empty())
        .and_then(|v| validate(v).err());
    rsx! {
        section { class: "appearance_editor",
            header { h3 { "Appearance Editor" } p { "Edit Light and Dark separately. Presets contain both themes and are saved immediately on this computer. Save Settings applies your theme colors." } }
            if let Some(message) = raw_error { p { role:"alert", "{message}" } }
            div {class:"ae_builtin",span {"Starter palettes"}
                for (name,colors) in appearance_colors::PRESETS {
                    button {disabled:saving,onclick:move |_| {
                        let old=resolved(&settings_from(&draft,&settings));let mut c=builtin(colors,false);c.presets=old.presets;write(draft,c);feedback.set(format!("{name} palette loaded for both themes. Save Settings to apply."));
                    },"{name}"}
                }
                for (name,colors) in [("Ocean gradient",appearance_colors::PRESETS[0].1),("Forest gradient",appearance_colors::PRESETS[1].1),("Sunset gradient",appearance_colors::PRESETS[6].1)] {
                    button {disabled:saving,onclick:move |_| {
                        let old=resolved(&settings_from(&draft,&settings));let mut c=builtin(colors,true);c.presets=old.presets;write(draft,c);feedback.set(format!("{name} loaded for both themes. Save Settings to apply."));
                    },"{name}"}
                }
            }
            div { class: "ae_presets",
                label { "Saved presets"
                    select { aria_label:"Saved presets", disabled:saving, value:"{selected}", onchange:move |e| {selected.set(e.value());feedback.set(String::new());},
                        option {value:"",selected:selected().is_empty(), "Choose preset"}
                        for preset in &config.presets { option {key:"{preset.name}",value:preset.name.clone(),selected:selected()==preset.name, "{preset.name}"} }
                    }
                }
                button {disabled:saving || selected().is_empty(), onclick:move |_| {
                    let mut c=resolved(&settings_from(&draft,&settings));
                    if let Some(p)=c.presets.iter().find(|p|p.name==selected()).cloned(){c.light=p.light;c.dark=p.dark;write(draft,c);feedback.set("Preset loaded into preview. Save Settings to apply.".into());}
                }, "Load" }
                label { "Preset name"
                    input {r#type:"text",class:"ae_preset_name",disabled:saving, maxlength:"60", placeholder:"e.g. Ocean gradient", value:"{preset_name}", oninput:move |e|preset_name.set(e.value())}
                }
                button {disabled:saving || preset_name().trim().is_empty(), onclick:move |_| {
                    let mut c=resolved(&settings_from(&draft,&settings));let name=preset_name().trim().to_string();
                    if c.presets.iter().any(|p|p.name.eq_ignore_ascii_case(&name)){feedback.set("Name already exists. Select it and use Replace selected.".into());return;}
                    if c.presets.len()>=40 {feedback.set("Maximum 40 presets. Delete one before adding another.".into());return;}
                    c.presets.push(Preset{name:name.clone(),light:c.light.clone(),dark:c.dark.clone()});
                    if let Err(message)=persist_presets(draft,&c,on_saved){feedback.set(message);return;}
                    write(draft,c);selected.set(name.clone());feedback.set(format!("Preset '{name}' saved and selected."));
                }, "Save as preset" }
            }
            div {class:"ae_preset_actions",
                button {disabled:saving || selected().is_empty(), onclick:move |_| {
                    let mut c=resolved(&settings_from(&draft,&settings));let light=c.light.clone();let dark=c.dark.clone();
                    if let Some(p)=c.presets.iter_mut().find(|p|p.name==selected()){p.light=light;p.dark=dark;
                        if let Err(message)=persist_presets(draft,&c,on_saved){feedback.set(message);return;}
                        write(draft,c);feedback.set("Selected preset replaced and saved.".into());}
                }, "Replace selected"}
                button {disabled:saving || selected().is_empty() || preset_name().trim().is_empty(), onclick:move |_| {
                    let mut c=resolved(&settings_from(&draft,&settings));let name=preset_name().trim().to_string();
                    if c.presets.iter().any(|p|p.name!=selected()&&p.name.eq_ignore_ascii_case(&name)){feedback.set("That preset name already exists.".into());return;}
                    if let Some(p)=c.presets.iter_mut().find(|p|p.name==selected()){p.name=name.clone();
                        if let Err(message)=persist_presets(draft,&c,on_saved){feedback.set(message);return;}
                        write(draft,c);selected.set(name);feedback.set("Preset renamed and saved.".into());}
                },"Rename selected"}
                button {disabled:saving || selected().is_empty(), onclick:move |_| {
                    let mut c=resolved(&settings_from(&draft,&settings));c.presets.retain(|p|p.name!=selected());
                    if let Err(message)=persist_presets(draft,&c,on_saved){feedback.set(message);return;}
                    write(draft,c);selected.set(String::new());feedback.set("Preset deleted. Current theme colors are kept.".into());
                },"Delete selected"}
            }
            if !feedback().is_empty(){p {class:"ae_feedback",role:"status","{feedback}"}}
            div {class:"ae_toolbar",
                div {class:"ae_modes",role:"group",aria_label:"Theme to edit",
                    for value in ["light","dark"] {button {disabled:saving,aria_pressed:mode()==value,onclick:move |_|mode.set(value.into()),"{value.to_uppercase()}"}}
                }
                label {"Section"
                    select {value:"{group}",onchange:move |e|group.set(e.value()),for name in GROUPS {option {value:*name,"{name}"}}}
                }
                button {disabled:saving,onclick:move |_| {
                    let mut c=resolved(&settings_from(&draft,&settings));if mode()=="dark"{c.dark=palette(&HashMap::new(),true);}else{c.light=palette(&HashMap::new(),false);}write(draft,c);feedback.set("Current theme reset to defaults in preview.".into());
                },"Reset this theme"}
            }
            div {class:"ae_columns",
                div {class:"ae_fields",
                    for (id,label,_,_,_,gradient) in FIELDS.iter().filter(|f|f.2==group()) {
                        PaintControl {key:"{mode}-{id}", id:*id, label:*label, gradient:*gradient, paint:colors[*id].clone(), disabled:saving,
                            on_change:move |(id,paint):(String,Paint)| {let mut c=resolved(&settings_from(&draft,&settings));if mode()=="dark"{c.dark.insert(id,paint);}else{c.light.insert(id,paint);}write(draft,c);}
                        }
                    }
                }
                aside {class:"ae_preview_wrap",
                    h4 {"{mode().to_uppercase()} preview"}
                    p {"Changes stay in this preview until you Save Settings."}
                    div {class:"ae_preview",style:preview,
                        div {class:"ae_sample_header", "KAY POS" span {"Sales"} small {"Kay Assistant"}}
                        div {class:"ae_sample_workspace",
                            div {class:"ae_sample_sidebar", "Menu" p {"Dashboard"} p {"Products"} p {"Settings"}}
                            div {class:"ae_sample_panel",
                                strong {"Products"}
                                div {class:"ae_sample_input","Search product, SKU or barcode"}
                                div {class:"ae_sample_category","All products"}
                                div {class:"ae_sample_product",strong {"Nestle Butterfly"} small {"Ice cream"} b {"2,000 Ks"}}
                                div {class:"ae_sample_product ae_sample_product_hover",strong {"Product hover"} b {"1,000 Ks"}}
                                div {class:"ae_sample_table","Product · Stock · Price"}
                                div {class:"ae_sample_selected","Selected row"}
                            }
                            div {class:"ae_sample_cart",strong {"Cart"} div {"1 × Nestle Butterfly"} b {"Total: 2,000 Ks"}
                                span {class:"ae_sample_button","Checkout"} span {class:"ae_sample_hover","Button hover"}
                            }
                        }
                        div {class:"ae_sample_dialog",strong {"Dialog preview"} small {"Main text and secondary text"} div {span {class:"ae_sample_good","Success"} span {class:"ae_sample_warning","Warning"} span {class:"ae_sample_bad","Error"}}}
                        div {class:"ae_sample_status","Database connected · Ready"}
                    }
                }
            }
        }
    }
}

fn settings_from(
    draft: &Signal<HashMap<String, String>>,
    original: &Memo<HashMap<String, String>>,
) -> HashMap<String, String> {
    let mut values = original.read().clone();
    values.extend(draft.read().clone());
    values
}

#[component]
fn PaintControl(
    id: &'static str,
    label: &'static str,
    gradient: bool,
    paint: Paint,
    disabled: bool,
    on_change: EventHandler<(String, Paint)>,
) -> Element {
    let current = paint.clone();
    let mut invalid = use_signal(String::new);
    rsx! {fieldset {class:"ae_paint",
        legend {"{label}"}
        div {class:"ae_paint_swatch",style:format!("background:{}",paint.css()),aria_hidden:"true"}
        if gradient {label {"Fill"
            select {disabled,value:paint.kind.clone(),onchange: {let p=current.clone();move |e:Event<FormData>|{let mut p=p.clone();p.kind=e.value();on_change.call((id.into(),p));}},
                option {value:"solid","Solid"} option {value:"linear","Linear gradient"} option {value:"radial","Radial gradient"}
            }
        }}
        label { if paint.kind=="solid" {"Color"}else{"Start color"}
            div {class:"ae_color_pair",
                input {r#type:"color",disabled,aria_label:format!("{label} color"),value:paint.color.clone(),oninput:{let p=current.clone();move |e:Event<FormData>|{let mut p=p.clone();p.color=e.value();on_change.call((id.into(),p));}}}
                input {r#type:"text",disabled,aria_label:format!("{label} hex color"),maxlength:"7",value:paint.color.clone(),onchange:{let p=current.clone();move |e:Event<FormData>|{if appearance_colors::valid(&e.value()){invalid.set(String::new());let mut p=p.clone();p.color=e.value();on_change.call((id.into(),p));}else{invalid.set("Use #RRGGBB. The last valid color remains in use.".into());}}}}
            }
        }
        if gradient && paint.kind!="solid" {label {"End color"
            div {class:"ae_color_pair",
                input {r#type:"color",disabled,aria_label:format!("{label} end color"),value:paint.end.clone(),oninput:{let p=current.clone();move |e:Event<FormData>|{let mut p=p.clone();p.end=e.value();on_change.call((id.into(),p));}}}
                input {r#type:"text",disabled,aria_label:format!("{label} end hex color"),maxlength:"7",value:paint.end.clone(),onchange:{let p=current.clone();move |e:Event<FormData>|{if appearance_colors::valid(&e.value()){invalid.set(String::new());let mut p=p.clone();p.end=e.value();on_change.call((id.into(),p));}else{invalid.set("Use #RRGGBB. The last valid color remains in use.".into());}}}}
            }
        }}
        if gradient && paint.kind=="linear" {label {"Direction ({paint.angle}°)"
            input {r#type:"range",min:"0",max:"360",step:"1",disabled,aria_label:format!("{label} gradient direction"),value:paint.angle.to_string(),oninput:{let p=current.clone();move |e:Event<FormData>|{if let Ok(angle)=e.value().parse::<u16>(){let mut p=p.clone();p.angle=angle.min(360);on_change.call((id.into(),p));}}}}
        }}
        if !invalid().is_empty() {small {role:"alert","{invalid}"}}
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::dioxus_core::{DynamicNode, TemplateNode};
    #[test]
    fn preset_library_saves_without_applying_theme_and_survives_reload() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("appearance.json");
        let active = resolved(&HashMap::new());
        let active_json = serde_json::to_string(&active).unwrap();
        std::fs::write(
            &path,
            serde_json::to_vec(&HashMap::from([(KEY.to_string(), active_json.clone())])).unwrap(),
        )
        .unwrap();
        let mut edited = active.clone();
        edited.light.get_mut("header").unwrap().color = "#123456".into();
        edited.presets.push(Preset {
            name: "Ocean".into(),
            light: edited.light.clone(),
            dark: edited.dark.clone(),
        });
        let (key, value) = save_library_at(&edited.presets, |changes| {
            let mut stored: HashMap<String, String> =
                serde_json::from_slice(&std::fs::read(&path)?)?;
            stored.extend(changes.iter().cloned());
            std::fs::write(&path, serde_json::to_vec(&stored)?)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(key, LIBRARY_KEY);
        assert!(validate_library(&value).is_ok());
        let stored: HashMap<String, String> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            stored[KEY], active_json,
            "Saving a preset must not apply its colors"
        );
        let reloaded = resolved(&stored);
        assert_eq!(reloaded.presets[0].name, "Ocean");
        assert_eq!(reloaded.presets[0].light["header"].color, "#123456");
        assert_ne!(reloaded.light["header"].color, "#123456");
        assert!(save_library_at(&edited.presets, |_| anyhow::bail!("disk full")).is_err());
        assert!(local_appearance::is_key(LIBRARY_KEY));
    }
    #[test]
    fn newly_saved_presets_render_in_dropdown_without_save_settings() {
        #[derive(Clone, Copy)]
        struct State(
            Signal<HashMap<String, String>>,
            Signal<HashMap<String, String>>,
        );
        fn probe() -> Element {
            let draft = use_signal(HashMap::<String, String>::new);
            let mut original = use_signal(HashMap::<String, String>::new);
            use_context_provider(move || State(draft, original));
            rsx! {Editor {draft,original:original.read().iter().map(|(k,v)|(k.clone(),v.clone())).collect(),saving:false,on_saved:move |values|{original.write().extend(values);}}}
        }
        fn has_option(dom: &VirtualDom, node: &VNode, name: &str) -> bool {
            if node.key.as_deref() == Some(name)
                && matches!(
                    node.template.roots.first(),
                    Some(TemplateNode::Element { tag: "option", .. })
                )
            {
                return true;
            }
            node.dynamic_nodes
                .iter()
                .enumerate()
                .any(|(index, child)| match child {
                    DynamicNode::Fragment(nodes) => {
                        nodes.iter().any(|node| has_option(dom, node, name))
                    }
                    DynamicNode::Component(component) => component
                        .mounted_scope(index, node, dom)
                        .is_some_and(|scope| has_option(dom, scope.root_node(), name)),
                    _ => false,
                })
        }
        let mut dom = VirtualDom::new(probe);
        dom.rebuild_in_place();
        let state = dom.in_scope(ScopeId::APP, consume_context::<State>);
        assert!(!has_option(&dom, dom.base_scope().root_node(), "Ocean"));
        let mut config = resolved(&HashMap::new());
        config.presets.push(Preset {
            name: "Ocean".into(),
            light: config.light.clone(),
            dark: config.dark.clone(),
        });
        let (_, value) = save_library_at(&config.presets, |_| Ok(())).unwrap();
        dom.in_scope(ScopeId::APP, || {
            let mut draft = state.0;
            let mut original = state.1;
            original.write().insert(LIBRARY_KEY.into(), value.clone());
            draft.write().insert(LIBRARY_KEY.into(), value);
            write(draft, config);
        });
        dom.render_immediate_to_vec();
        assert!(
            has_option(&dom, dom.base_scope().root_node(), "Ocean"),
            "Saved preset must appear immediately"
        );
        dom.in_scope(ScopeId::APP, || {
            let mut draft = state.0;
            draft.write().clear();
        });
        dom.render_immediate_to_vec();
        assert!(
            has_option(&dom, dom.base_scope().root_node(), "Ocean"),
            "Reset Changes must retain saved presets"
        );
        dom.in_scope(ScopeId::APP, || {
            let mut original = state.1;
            original.write().insert(LIBRARY_KEY.into(), "[]".into());
        });
        dom.render_immediate_to_vec();
        assert!(
            !has_option(&dom, dom.base_scope().root_node(), "Ocean"),
            "Saved-list prop changes must refresh dropdown even with no draft changes"
        );
    }
    #[test]
    fn validates_structured_gradients_and_rejects_css_injection() {
        let mut c = resolved(&HashMap::new());
        c.dark.get_mut("header").unwrap().kind = "linear".into();
        let encoded = serde_json::to_string(&c).unwrap();
        assert!(validate(&encoded).is_ok());
        c.dark.get_mut("header").unwrap().end = "red;background:url(x)".into();
        assert!(validate(&serde_json::to_string(&c).unwrap()).is_err());
        c = resolved(&HashMap::new());
        c.light.get_mut("ink").unwrap().kind = "radial".into();
        assert!(validate(&serde_json::to_string(&c).unwrap()).is_err());
    }
    #[test]
    fn themes_and_presets_roundtrip_independently() {
        let mut c = resolved(&HashMap::new());
        c.light.get_mut("header").unwrap().color = "#123456".into();
        c.dark.get_mut("header").unwrap().kind = "linear".into();
        c.dark.get_mut("header").unwrap().end = "#abcdef".into();
        c.presets.push(Preset {
            name: "Ocean".into(),
            light: c.light.clone(),
            dark: c.dark.clone(),
        });
        let encoded = serde_json::to_string(&c).unwrap();
        assert_eq!(parse(&encoded).unwrap(), c);
        let settings = HashMap::from([(KEY.into(), encoded)]);
        let css = stylesheet(&settings);
        assert!(css.contains("--ae-header:#123456"));
        assert!(css.contains("linear-gradient(135deg,#1d4ed8, #abcdef)"));
        assert!(css.contains("prefers-color-scheme:dark"));
        c.presets.push(c.presets[0].clone());
        assert!(validate(&serde_json::to_string(&c).unwrap()).is_err());
    }
    #[test]
    fn defaults_preserve_legacy_and_do_not_change_unsaved_clients() {
        let settings = HashMap::from([("ui_header_color".into(), "#ffffff".into())]);
        assert!(stylesheet(&settings).is_empty());
        let c = resolved(&settings);
        assert_eq!(c.light["header"].color, "#ffffff");
        assert_eq!(c.dark["header-text"].color, "#000000");
        assert_ne!(c.light["panel"].color, c.dark["panel"].color);
        assert!(local_appearance::is_key(KEY));
    }
    #[test]
    fn editor_builds_and_exports_optional_visual_fixture() {
        fn probe() -> Element {
            let draft = use_signal(HashMap::<String, String>::new);
            rsx! { Editor {draft, original:vec![(KEY.into(),String::new())], saving:false,on_saved:move |_| {}} }
        }
        let mut dom = VirtualDom::new(probe);
        dom.rebuild_in_place();
        if let Ok(path) = std::env::var("KAY_THEME_TEST_FIXTURE") {
            let mut c = builtin(appearance_colors::PRESETS[1].1, true);
            c.dark.get_mut("header").unwrap().color = "#312e81".into();
            c.dark.get_mut("header").unwrap().end = "#0f766e".into();
            c.light.get_mut("nav-selected").unwrap().kind = "radial".into();
            c.light.get_mut("nav-selected").unwrap().end = "#1e40af".into();
            let settings = HashMap::from([(KEY.into(), serde_json::to_string(&c).unwrap())]);
            let fixture = serde_json::json!({"css":stylesheet(&settings),"light":variables(&c.light),"dark":variables(&c.dark),"fields":FIELDS.iter().map(|f|serde_json::json!({"id":f.0,"label":f.1,"gradient":f.5})).collect::<Vec<_>>()});
            std::fs::write(path, serde_json::to_vec(&fixture).unwrap()).unwrap();
        }
    }
}
