use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{
    connect,
    locations::{self, Location},
};

#[component]
pub fn LocationsPage(db_form: DbForm, actor:pos_core::auth::Session, on_sales: EventHandler<()>) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        async move {
            async { locations::list(&connect(&source.database_config()?).await?).await }
                .await
                .map_err(|e| format!("{e:#}"))
        }
    });
    let mut transfer_stock=use_signal(||None::<locations::Stock>);
    let mut transferring=use_signal(||false);
    let mut notice=use_signal(String::new);
    let mut manage = use_signal(|| false);
    let mut search = use_signal(String::new);
    let mut filter = use_signal(String::new);
    let mut applied = use_signal(|| (String::new(), String::new()));
    let mut page = use_signal(|| 0usize);
    let mut editor = use_signal(|| None::<Location>);
    let mut deletion = use_signal(|| None::<Location>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    use_effect(move || {
        let values = (search(), filter());
        if *applied.peek() != values { applied.set(values); page.set(0); }
    });
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
                && (!s.location.trim().eq_ignore_ascii_case("Legacy / unassigned") || s.quantity != 0.0)
                && format!("{} {}", s.name, s.sku)
                    .to_lowercase()
                    .contains(&query)
        })
        .collect::<Vec<_>>();
    let start = (page() * 50).min(filtered.len().saturating_sub(1) / 50 * 50);
    let end = (start + 50).min(filtered.len());
    rsx! {
        section {class:"customers_page products_touch locations_touch",
            div {class:"location_toolbar",
                div {class:"receipt_tabs location_tabs",role:"tablist",
                    button {role:"tab",aria_selected:!manage(),class:if !manage(){"active"}else{""},onclick:move |_|manage.set(false),crate::icons::TabLabel {label:"Stock by location"}}
                    button {role:"tab",aria_selected:manage(),class:if manage(){"active"}else{""},onclick:move |_|manage.set(true),crate::icons::TabLabel {label:"Manage locations"}}
                }
                div {class:"customers_actions",
                    if actor.allows(pos_core::auth::Permission::Manage) {button {onclick:move |_|{transfer_stock.set(None);transferring.set(true);}, crate::icons::ActionLabel { label:"Stock Transfer" } }}
                    button {class:"customer_primary",onclick:move |_|editor.set(Some(Location::default())),"Add location"}
                }
            }
            if !notice().is_empty(){p {class:"customers_notice",role:"status","{notice}"}}
            if !manage() {
                div {class:"location_filters",
                    label {"Search" input {r#type:"search",placeholder:"Product name or SKU",value:"{search}",oninput:move |e|search.set(e.value()),onkeydown:move |e|if e.key()==Key::Enter {applied.set((search(),filter()));page.set(0);}}}
                    label {"Location" select {value:"{filter}",onchange:move |e|filter.set(e.value()),option {value:"","All locations"} for name in names {option {value:"{name}","{name}"}}}}
                    button {class:if filter()=="Legacy / unassigned"{"customer_primary"}else{""},onclick:move |_|{manage.set(false);filter.set("Legacy / unassigned".into());search.set(String::new());page.set(0);},"Legacy / unassigned"}
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
                            article {class:"location_stock_row",
                                div {class:"location_product_identity",
                                    div {class:"product_manage_image",crate::CatalogImage {id:row.id,name:row.name.clone()}}
                                    div {strong {"{row.name}"} small {"{row.sku} · {row.category} · {row.location}"}}
                                }
                                div {class:"customers_actions",div {class:"location_quantity",strong {{crate::format_qty(row.quantity)}} small {"stock units"}}
                                    if actor.allows(pos_core::auth::Permission::Manage) && row.quantity>0.0 {button {onclick:{let row=(*row).clone();move |_|{transfer_stock.set(Some(row.clone()));transferring.set(true);}}, crate::icons::ActionLabel { label:"Transfer" } }}
                                }
                            }
                        }
                    }
                }
                if !manage() && filtered.len()>50 {footer {class:"location_pagination",button { title:"Previous page", aria_label:"Previous page",disabled:start==0,onclick:move |_|page.set(start/50-1),crate::icons::Icon { name:"arrow_circle_left" }} button { title:"Next page", aria_label:"Next page",disabled:end>=filtered.len(),onclick:move |_|page.set(start/50+1),crate::icons::Icon { name:"arrow_circle_right" }}}}
            }
        }
        if transferring(){crate::stock_transfer::StockTransferDialog {db_form:db_form.clone(),actor,initial_stock:transfer_stock(),on_close:move |_|transferring.set(false),on_saved:move |reference|{transferring.set(false);notice.set(format!("Stock transferred: {reference}"));data.restart();}}}
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
