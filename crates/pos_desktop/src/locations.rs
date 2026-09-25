use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{
    connect,
    locations::{self, Location},
};

#[component]
pub fn LocationsPage(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        async move {
            async { locations::list(&connect(&source.database_config()?).await?).await }
                .await
                .map_err(|e| format!("{e:#}"))
        }
    });
    let mut manage = use_signal(|| false);
    let mut search = use_signal(String::new);
    let mut filter = use_signal(String::new);
    let mut applied = use_signal(|| (String::new(), String::new()));
    let mut page = use_signal(|| 0usize);
    let mut editor = use_signal(|| None::<Location>);
    let mut deletion = use_signal(|| None::<Location>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let state = data.read();
    let (locations, stock) = state
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let mut names = locations
        .iter()
        .map(|l| l.name.clone())
        .chain(stock.iter().map(|s| s.location.clone()))
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let (query, selected) = applied();
    let query = query.trim().to_lowercase();
    let filtered = stock
        .iter()
        .filter(|s| {
            (selected.is_empty() || s.location.trim().eq_ignore_ascii_case(selected.trim()))
                && format!("{} {}", s.name, s.sku)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect::<Vec<_>>();
    let start = (page() * 50).min(filtered.len().saturating_sub(1) / 50 * 50);
    let end = (start + 50).min(filtered.len());
    rsx! {
        section {class:"customers_page products_touch locations_touch",
            header {class:"customers_header customers_touch_header",h2 {crate::icons::ActionLabel { label:"Locations" }}
                div {class:"customers_actions",button {class:"customer_primary",onclick:move |_|editor.set(Some(Location::default())),"Add location"}}
            }
            div {class:"receipt_tabs location_tabs",role:"tablist",
                button {role:"tab",aria_selected:!manage(),class:if !manage(){"active"}else{""},onclick:move |_|manage.set(false),"Stock by location"}
                button {role:"tab",aria_selected:manage(),class:if manage(){"active"}else{""},onclick:move |_|manage.set(true),"Manage locations"}
            }
            if !manage() {
                div {class:"location_filters",
                    label {"Search" input {placeholder:"Product name or SKU",value:"{search}",oninput:move |e|search.set(e.value()),onkeydown:move |e|if e.key()==Key::Enter {applied.set((search(),filter()));page.set(0);}}}
                    label {"Location" select {value:"{filter}",onchange:move |e|filter.set(e.value()),option {value:"","All locations"} for name in names {option {value:"{name}","{name}"}}}}
                    button {class:"customer_primary",onclick:move |_|{applied.set((search(),filter()));page.set(0);},"Search"}
                    button {onclick:move |_|{search.set(String::new());filter.set(String::new());applied.set((String::new(),String::new()));page.set(0);},"Reset"}
                }
            }
            section {class:"products_items",
                header {class:"products_items_header",h3 {if manage(){"Manage locations"}else{"Products & stock"}}
                    button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}
                    if !manage() && data.finished() {small {{format!("{}-{} of {} stock records",if filtered.is_empty(){0}else{start+1},end,filtered.len())}}}
                }
                div {class:"products_manage_list",
                    if !data.finished() {div {class:"product_loading",role:"status",span {class:"loading_spinner"} strong {"Loading..."}}}
                    else if let Some(Err(message))=state.as_ref() {p {role:"alert","{message}"}}
                    else if manage() {
                        if locations.is_empty() {p {"No locations found."}}
                        for location in &locations {
                            article {class:"location_stock_row",key:"{location.id}",div {strong {"{location.name}"} small {{format!("{} stock records",stock.iter().filter(|s|s.location.trim().eq_ignore_ascii_case(location.name.trim())).count())}}}
                                div {class:"customers_actions",button {onclick:{let l=location.clone();move |_|editor.set(Some(l.clone()))},crate::icons::ActionLabel { label:"Edit" }} button {class:"product_delete",onclick:{let l=location.clone();move |_|deletion.set(Some(l.clone()))},crate::icons::ActionLabel { label:"Delete" }}}
                            }
                        }
                    } else {
                        if filtered.is_empty() {p {"No stock records found."}}
                        for row in filtered.iter().skip(start).take(50) {
                            article {class:"location_stock_row",div {strong {"{row.name}"} small {"{row.sku} · {row.category} · {row.location}"}}
                                div {class:"location_quantity",strong {{crate::format_qty(row.quantity)}} small {"stock units"}}
                            }
                        }
                    }
                }
                if !manage() && filtered.len()>50 {footer {class:"location_pagination",button {disabled:start==0,onclick:move |_|page.set(start/50-1),"Previous"} button {disabled:end>=filtered.len(),onclick:move |_|page.set(start/50+1),"Next"}}}
            }
        }
        if let Some(location)=editor() {LocationEditor {location,db_form:db_form.clone(),on_close:move |_|editor.set(None),on_saved:move |_|{editor.set(None);data.restart();},on_error:move |e|error.set(e)}}
        if let Some(location)=deletion() {div {class:"modal_backdrop",section {class:"customer_dialog",role:"dialog",aria_modal:"true",aria_label:"Delete location",h2 {"Delete location?"} p {"{location.name}"}
            div {class:"customers_actions",button {disabled:busy(),onclick:move |_|deletion.set(None),crate::icons::ActionLabel { label:"Cancel" }}
                button {class:"product_delete",disabled:busy(),onclick:move |_|{if busy(){return;}busy.set(true);let l=location.clone();let source=db_form.clone();spawn(async move {let result=async {locations::save(&connect(&source.database_config()?).await?,&l,true).await}.await;busy.set(false);match result {Ok(())=>{deletion.set(None);data.restart();},Err(e)=>error.set(format!("{e:#}"))}});},crate::icons::ActionLabel { label:"Delete" }}
            }
        }}}
        if !error().is_empty() {MessageBox {title:"Location Error".to_string(),message:error(),on_close:move |_|error.set(String::new())}}
    }
}

#[component]
fn LocationEditor(
    location: Location,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut name = use_signal(|| location.name.clone());
    let mut busy = use_signal(|| false);
    rsx! {div {class:"modal_backdrop",section {class:"customer_dialog",role:"dialog",aria_modal:"true",aria_label:"Location",
        h2 {if location.id==0 {"Add location"}else{"Edit location"}}
        div {class:"customer_form",label {"Location name" input {value:"{name}",disabled:busy(),oninput:move |e|name.set(e.value()),onmounted:move |e|async move {let _=e.set_focus(true).await;}}}}
        div {class:"customers_actions",button {disabled:busy(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel { label:"Cancel" }}
            button {class:"customer_primary",disabled:busy()||name().trim().is_empty(),onclick:move |_|{if busy(){return;}busy.set(true);let l=Location {id:location.id,name:name()};let source=db_form.clone();spawn(async move {let result=async {locations::save(&connect(&source.database_config()?).await?,&l,false).await}.await;busy.set(false);match result {Ok(())=>on_saved.call(()),Err(e)=>on_error.call(format!("{e:#}"))}});},if busy(){"Saving..."}else{"Save location"}}
        }
    }}}
}
