use dioxus::prelude::*;
use std::collections::{HashMap, BTreeSet};

pub const KEY: &str = "sidebar_hidden_pages";
const GROUPS: &[(&str, &[&str])] = &[
    ("Sales", &["Sales", "Receipts", "Expenses", "Service Order"]),
    ("People", &["Employees", "Customers", "Suppliers", "Purchases"]),
    ("Products & stock", &["Products", "Discounts", "Categories", "Locations", "Inventory"]),
    ("Reports", &["Kay Assistant", "Activity Log", "Reports", "Dashboard", "Sale Summary"]),
];
fn parse(settings: &HashMap<String, String>) -> BTreeSet<String> {
    settings.get(KEY).and_then(|v| serde_json::from_str::<BTreeSet<String>>(v).ok()).unwrap_or_default()
}
pub fn hidden(settings: &HashMap<String, String>, page: &str) -> bool {
    page != "Settings Center" && parse(settings).contains(page)
}
pub fn group_hidden(settings: &HashMap<String, String>, pages: &[&str]) -> bool {
    pages.iter().all(|page| hidden(settings, page))
}

#[component]
pub fn MenuVisibility(settings: HashMap<String,String>, on_saved: EventHandler<HashMap<String,String>>) -> Element {
    let mut hidden_pages=use_signal(|| parse(&settings));
    let mut notice=use_signal(String::new);
    rsx! {
        section {class:"menu_visibility_settings",
            div {class:"menu_visibility_groups",
                for (title,pages) in GROUPS {
                    fieldset {
                        legend {"{title}"}
                        for page in *pages {
                            label {class:"menu_visibility_option",
                                input {r#type:"checkbox",checked:!hidden_pages.read().contains(*page),onchange:move |event| {
                                    if event.checked(){hidden_pages.write().remove(*page);}else{hidden_pages.write().insert((*page).into());}
                                    notice.set(String::new());
                                }}
                                span {"{page}"}
                            }
                        }
                    }
                }
            }
            div {class:"settings_page_actions",
                button {onclick:move |_|{hidden_pages.set(BTreeSet::new());notice.set(String::new());},"Show all"}
                button {class:"settings_save",onclick:move |_|{
                    let values=vec![(KEY.to_string(),serde_json::to_string(&*hidden_pages.read()).unwrap_or_else(|_|"[]".into()))];
                    match crate::local_appearance::save(&values) {
                        Ok(())=>{on_saved.call(values.into_iter().collect());notice.set("Menu saved.".into());}
                        Err(error)=>notice.set(format!("{error:#}")),
                    }
                }, crate::icons::ActionLabel { label:"Save" } }
            }
            if !notice().is_empty(){p {role:"status","{notice}"}}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn defaults_and_recovery_are_visible() {
        let mut settings=HashMap::new();
        assert!(!hidden(&settings,"Sales"));
        settings.insert(KEY.into(),"invalid".into());
        assert!(!hidden(&settings,"Sales"));
        settings.insert(KEY.into(),r#"["Sales","Receipts","Settings Center"]"#.into());
        assert!(group_hidden(&settings,&["Sales","Receipts"]));
        assert!(!group_hidden(&settings,&["Sales","Expenses"]));
        assert!(!hidden(&settings,"Settings Center"));
    }
}
