use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{connect, db, inventory, Product};

#[component]
pub fn InventoryPage(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        async move {
            async {
                let pool = connect(&source.database_config()?).await?;
                tokio::try_join!(
                    db::search_product_metadata(&pool, "", 0),
                    db::list_categories(&pool),
                    pos_core::locations::list_names(&pool),
                    pos_core::suppliers::list(&pool)
                )
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let mut search = use_signal(String::new);
    let mut category = use_signal(String::new);
    let mut applied = use_signal(|| (String::new(), String::new()));
    let mut count = use_signal(|| 50usize);
    let mut selected = use_signal(|| None::<i32>);
    let mut receiving = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut revision = use_signal(|| 0u64);
    let state = data.read();
    let (products, categories, locations, suppliers) = state
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let (query, cat) = applied();
    let query = query.trim().to_lowercase();
    let rows = products
        .iter()
        .filter(|p| {
            (cat.is_empty()
                || p.category_id.map(|id| id.to_string()).as_deref() == Some(cat.as_str()))
                && format!(
                    "{} {} {}",
                    p.name,
                    p.sku.as_deref().unwrap_or(""),
                    p.barcode.as_deref().unwrap_or("")
                )
                .to_lowercase()
                .contains(&query)
        })
        .collect::<Vec<_>>();
    let current = products.iter().find(|p| Some(p.id) == selected()).cloned();
    rsx! {
        section {class:"customers_page products_touch inventory_touch",
            header {class:"customers_header customers_touch_header",h2 {crate::icons::ActionLabel { label:"Inventory" }} div {class:"customers_actions",button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|{data.restart();revision+=1;},"Refresh"}}}
            div {class:"location_filters inventory_filters",
                label {"Search inventory" input {placeholder:"Product name, SKU or barcode",value:"{search}",oninput:move |e|search.set(e.value()),onkeydown:move |e|if e.key()==Key::Enter{applied.set((search(),category()));count.set(50);}}}
                label {"Category" select {value:"{category}",onchange:move |e|category.set(e.value()),option {value:"","All categories"} for c in categories {option {value:"{c.id}","{c.name}"}}}}
                button {class:"customer_primary",onclick:move |_|{applied.set((search(),category()));count.set(50);},"Search"}
                button {onclick:move |_|{search.set(String::new());category.set(String::new());applied.set((String::new(),String::new()));count.set(50);},"Reset"}
            }
            div {class:"inventory_columns",
                section {class:"inventory_list",h3 {"Products & stock"}
                    if !data.finished() {div {class:"product_loading",role:"status",span {class:"loading_spinner"} strong {"Loading..."}}}
                    else if let Some(Err(e))=state.as_ref(){p {role:"alert","{e}"}}
                    else {
                        if rows.is_empty(){p {"No products found."}}
                        for p in rows.iter().take(count()) {
                            button {class:if selected()==Some(p.id){"inventory_product selected"}else{"inventory_product"},onclick:{let id=p.id;move |_|selected.set(Some(id))},
                                span {class:"product_manage_image",crate::CatalogImage {key:"{p.id}",id:p.id,name:p.name.clone()}}
                                span {class:"inventory_identity",span {class:"inventory_product_title",strong {"{p.name}"} span {class:"product_mode","data-kind":if crate::is_service_product(p){"Service"}else if crate::is_variant_product(p){"Variants"}else{"Each"},if crate::is_service_product(p){"Service"}else if crate::is_variant_product(p){"Variants"}else{"Each"}}} small {"{p.sku.as_deref().unwrap_or_default()}"}}
                                span {class:"inventory_balance",strong {if crate::is_service_product(p){"Service"}else{{crate::format_qty(p.stock)}" pcs"}} small {class:if p.stock<=0.0{"inventory_empty_stock"}else if p.stock<=p.low_stock{"inventory_low_stock"}else{"inventory_in_stock"},if crate::is_service_product(p){"No stock tracking"}else if p.stock<=0.0{"Out of stock"}else if p.stock<=p.low_stock{"Low stock"}else{"In stock"}}}
                            }
                        }
                        if count()<rows.len(){button {onclick:move |_|count+=50,"Load more"}}
                    }
                }
                section {class:"inventory_detail",
                    if let Some(p)=current.clone() {
                        div {class:"inventory_detail_toolbar",button {onclick:move |_|receiving.set(true),crate::icons::ActionLabel { label:"View Movements" }}}
                        header {class:"inventory_detail_heading",h3 {"{p.name}"}}
                        p {class:"inventory_current_stock","Current stock: " {crate::format_qty(p.stock)} " pcs · Cost: " {crate::format_ks(p.cost)}}
                        if crate::is_service_product(&p) {p {"No stock tracking"}}
                        else {for detail_key in [format!("{}-{}",p.id,revision())] {
                            ReceiveDialog {key:"{detail_key}",product:p.clone(),suppliers:suppliers.clone(),location_names:locations.iter().map(|l|l.name.clone()).collect(),db_form:db_form.clone(),on_close:move |_|selected.set(None),on_saved:move |_|{data.restart();revision+=1;},on_error:move |e|error.set(e)}
                        }
                        }
                    } else {div {class:"inventory_select_empty",strong {"Select a product"}}}
                }
            }
        }
        if receiving() {if let Some(product)=current {div {class:"modal_backdrop",section {class:"customer_dialog movement_dialog",role:"dialog",aria_modal:"true",aria_label:"Stock movements",
            header {class:"movement_dialog_header",div {h2 {"Stock Movements"} p {"{product.name}"}} button {title:"Close",aria_label:"Close stock movements",onclick:move |_|receiving.set(false),"×"}}
            MovementList {product_id:product.id,db_form:db_form.clone(),on_changed:move |_|{data.restart();revision+=1;}}
        }}}}
        if !error().is_empty(){MessageBox {title:"Inventory Error".to_string(),message:error(),on_close:move |_|error.set(String::new())}}
    }
}

fn movement_label(kind: &str) -> &str {
    match kind { "in"|"stock_in"=>"Stock In", "out"|"stock_out"=>"Stock Out", "adjustment"=>"Adjustment", "sale"=>"Sale", "refund"=>"Refund", _=>kind }
}

fn movement_time(value: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(value,"%Y-%m-%d %H:%M:%S%.f")
        .map(|date|date.format("%d %b %Y · %H:%M").to_string()).unwrap_or_else(|_|value.to_string())
}

#[component]
fn MovementList(product_id: i32, db_form: DbForm, on_changed: EventHandler<()>) -> Element {
    let mut count = use_signal(|| 30usize);
    let source_form = db_form.clone();
    let session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let mut selected = use_signal(|| None::<i32>);
    let mut reason = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut message = use_signal(String::new);
    let mut data = use_resource(move || {
        let source = db_form.clone();
        let limit = count() as i64 + 1;
        async move {
            async {
                inventory::movements(&connect(&source.database_config()?).await?, product_id, limit).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    rsx! {section {class:"inventory_movements",
        header {class:"movement_toolbar",span {"Latest first"} button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:busy(),onclick:move |_|data.restart(),"Refresh"}}
        if !message().is_empty(){p {role:"status","{message}"}}
        if let Some(id)=selected() {
            section {class:"inventory_reversal_confirmation",
                h3 {"Reverse Stock In #{id}?"}
                p {"Stock reversal only. Cost and supplier payments are unchanged."}
                label {"Reason" input {value:"{reason}",disabled:busy(),oninput:move |e|reason.set(e.value())}}
                button {disabled:busy(),onclick:move |_|selected.set(None),crate::icons::ActionLabel { label:"Cancel" }}
                button {disabled:busy() || reason().trim().is_empty(),onclick:move |_|{
                    let Some(actor)=session.read().clone() else{return;};
                    let source=source_form.clone();let why=reason();busy.set(true);message.set(String::new());
                    spawn(async move {
                        let result=async {let pool=connect(&source.database_config()?).await?;pos_core::inventory_reversal::reverse_stock_in(&pool,product_id,id,&why,&actor).await}.await;
                        busy.set(false);
                        match result {Ok(())=>{selected.set(None);message.set("Stock In reversed".into());data.restart();on_changed.call(());},Err(e)=>message.set(format!("{e:#}"))}
                    });
                },if busy(){"Reversing..."}else{"Confirm Reverse"}}
            }
        }
        match data.read().as_ref() {
            None=>rsx!{p {role:"status","Loading..."}},
            Some(Err(e))=>rsx!{p {role:"alert","{e}"}},
            Some(Ok(rows))=>rsx!{
                if rows.is_empty(){p {"No stock movements."}}
                div {class:"movement_column_labels",span {"Date / Location"} span {"Movement / Details"} span {"Change / Balance"} span {"Action"}}
                for row in rows.iter().take(count()) {article {key:"{row.id}",class:"inventory_movement movement_record",
                    div {class:"movement_when",strong {"{movement_time(&row.created_at)}"} small {"{row.location}"} small {"#{row.id} · {row.created_by}"}}
                    div {class:"movement_description",span {class:"movement_type","data-kind":"{row.kind}","{movement_label(&row.kind)}"} p {"{row.reason}"} if let Some(id)=row.variant_id {small {"{row.variant_name} (#{id})"}} if !row.reference.is_empty(){small {"{row.reference}"}}
                        if !row.notes.is_empty() || !row.batches.is_empty(){details {summary {"Details"} if !row.notes.is_empty(){p {"{row.notes}"}} if !row.batches.is_empty(){p {"{row.batches}"}}}}
                    }
                    div {class:"movement_quantity","data-positive":row.new_stock>row.old_stock,strong {if row.new_stock>row.old_stock {"+"} "{crate::format_qty(row.new_stock-row.old_stock)}"} small {{format!("{} → {}",crate::format_qty(row.old_stock),crate::format_qty(row.new_stock))}}}
                    div {class:"movement_action",
                    if row.notes.contains("[REVERSED]"){strong {"Reversed"}} else if matches!(row.kind.as_str(),"in"|"stock_in") && !row.reference.starts_with("REV-") && !row.reference.ends_with("-REV") && session.read().as_ref().is_some_and(|s|s.allows(pos_core::auth::Permission::Manage)) {
                        button {disabled:busy(),onclick:{let id=row.id;move |_|{selected.set(Some(id));reason.set(String::new());message.set(String::new());}},"Reverse"}
                    }
                    }
                }}
                if rows.len()>count() && count()<10000 {button {onclick:move |_|count.set((count()+30).min(10000)),"Load more"}}
            }
        }
    }}
}

#[component]
fn ReceiveDialog(
    product: Product,
    location_names: Vec<String>,
    suppliers: Vec<pos_core::suppliers::Supplier>,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut mode = use_signal(|| "Stock In".to_string());
    let session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let mut qty = use_signal(String::new);
    let mut cost = use_signal(|| product.cost.to_string());
    let mut variant = use_signal(String::new);
    let mut location = use_signal(|| {
        location_names
            .first()
            .cloned()
            .unwrap_or_default()
    });
    let stock_source = db_form.clone();
    let product_id = product.id;
    let names_source = db_form.clone();
    let stock_locations = use_resource(move || {
        let source = names_source.clone();
        let variant_id = variant().parse::<i32>().ok();
        async move {
            async { inventory::stock_location_names(&connect(&source.database_config()?).await?, product_id, variant_id).await }.await.map_err(|e|format!("{e:#}"))
        }
    });
    let mut location_names = location_names;
    if mode() != "Stock In" {
        if let Some(Ok(names)) = stock_locations.read().as_ref() {
            for name in names {
                if !location_names.contains(name) { location_names.push(name.clone()); }
            }
        }
    }
    let location_balance = use_resource(move || {
        let source = stock_source.clone();
        let place = location();
        let variant_id = variant().parse::<i32>().ok();
        async move {
            async { inventory::location_stock(&connect(&source.database_config()?).await?, product_id, variant_id, &place).await }.await.map_err(|e|format!("{e:#}"))
        }
    });
    let mut batch = use_signal(String::new);
    let batch_source = db_form.clone();
    let batch_number = use_resource(move || {
        let source = batch_source.clone();
        async move {
            async {
                let pool = connect(&source.database_config()?).await?;
                pos_core::numbers::batch(&mut *pool.acquire().await?).await
            }.await.map_err(|e| format!("{e:#}"))
        }
    });
    use_effect(move || {
        if let Some(Ok(number)) = batch_number.read().as_ref() {
            batch.set(number.clone());
        }
    });
    let mut supplier = use_signal(String::new);
    let mut expiry = use_signal(String::new);
    let mut no_expire = use_signal(|| true);
    let mut reason = use_signal(|| "Stock received".to_string());
    let mut reference = use_signal(String::new);
    let mut actor = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let selected_variant = product.variants.iter().find(|v| Some(v.variant_id)==variant().parse().ok());
    let current_stock = if crate::is_variant_product(&product) { selected_variant.map(|v|v.stock) } else { Some(product.stock) };
    let variant_costs = product.variants.clone();
    rsx! {div {class:"inventory_inline_editor",
        div {class:"inventory_action_tabs",role:"tablist",for label in ["Stock In","Stock Out","Adjustment"] {button {role:"tab",aria_selected:mode()==label,class:if mode()==label{"active"}else{""},disabled:busy(),onclick:move |_|{mode.set(label.into());qty.set(String::new());reason.set(String::new());},"{label}"}}}
        section {class:"inventory_action_form",h3 {"{mode}"}
        div {class:if mode()=="Stock In" {"customer_form inventory_stock_in_fields"}else{"customer_form"},
            if crate::is_variant_product(&product){label {class:"inventory_variant_field","Variant" select {disabled:busy(),value:"{variant}",onchange:move |e|{let value=e.value();if let Some(v)=variant_costs.iter().find(|v|Some(v.variant_id)==value.parse().ok()){cost.set(v.cost.to_string());}variant.set(value);},option {value:"","Select variant"} for v in &product.variants {option {value:"{v.variant_id}","{v.size.as_deref().unwrap_or_default()} {v.color.as_deref().unwrap_or_default()} · {v.sku.as_deref().unwrap_or_default()} · Stock: " {crate::format_qty(v.stock)}}}}}
            }
            if mode()=="Stock In" {
                div {class:"inventory_receive_balance",
                    span {if crate::is_variant_product(&product) {"Current variant stock (all locations)"}else{"Current stock (all locations)"} strong {if let Some(stock)=current_stock {{crate::format_qty(stock)}}else{"Select variant"}}}
                    span {"After Stock In" strong {if let Some(stock)=current_stock {if let Ok(quantity)=qty().parse::<i32>() {if quantity>0 {{crate::format_qty(stock+f64::from(quantity))}}else{"-"}}else{"-"}}else{"-"}}}
                }
            }
            label {"Location" select {disabled:busy(),value:"{location}",onchange:move |e|location.set(e.value()),option {value:"",disabled:true,"Select location"} for name in &location_names {option {value:"{name}","{name}"}}}}
            if mode()!="Stock In" {p {match location_balance.read().as_ref(){Some(Ok(value))=>format!("Current location stock: {}",crate::format_qty(*value)),Some(Err(e))=>e.clone(),None=>"Loading location stock...".into()}}}
            label {if mode()=="Adjustment"{"New quantity at selected location"}else{"Quantity"} input {r#type:"number",min:if mode()=="Adjustment"{"0"}else{"1"},step:"1",disabled:busy(),value:"{qty}",oninput:move |e|qty.set(e.value())}}
            if mode()=="Stock In" {
            label {"Unit cost" input {r#type:"number",min:"0",step:"any",disabled:busy(),value:"{cost}",oninput:move |e|cost.set(e.value())}}
            label {"Batch No." input {disabled:busy()||!batch_number.finished(),value:"{batch}",oninput:move |e|batch.set(e.value())}}
            if let Some(Err(e))=batch_number.read().as_ref(){p {class:"inventory_notes_field",role:"alert","{e}"}}
            label {"Expiry date" input {r#type:"date",disabled:busy()||no_expire(),value:if no_expire(){String::new()}else{expiry()},oninput:move |e|expiry.set(e.value())}}
            label {class:"inventory_no_expire",input {r#type:"checkbox",disabled:busy(),checked:no_expire(),onchange:move |e|no_expire.set(e.checked())} "No Expire"}
            }
            if mode()=="Stock In" {
                label {"Supplier" select {disabled:busy(),value:"{supplier}",onchange:move |e|supplier.set(e.value()),option {value:"","No supplier"} for s in suppliers.iter().filter(|s|s.status.eq_ignore_ascii_case("active")) {option {value:"{s.id}","{s.name}"}}}}
            } else {
                label {"Reason" input {disabled:busy(),value:"{reason}",oninput:move |e|reason.set(e.value())}}
                label {"Reference" input {disabled:busy(),value:"{reference}",oninput:move |e|reference.set(e.value())}}
            }
            if mode()=="Stock In" {label {"Received by" input {disabled:busy(),value:"{actor}",oninput:move |e|actor.set(e.value())}}} else {label {"Recorded by" span {"{session.read().as_ref().map(|s|s.username()).unwrap_or_default()}"}}}
            label {class:"inventory_notes_field","Notes" textarea {disabled:busy(),value:"{notes}",oninput:move |e|notes.set(e.value())}}
        }
        div {class:"customers_actions inventory_save_actions",button {disabled:busy(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel { label:"Cancel" }}
            button {class:"customer_primary",disabled:busy(),onclick:move |_|{
                if busy(){return;}
                let Some(operator)=session.read().clone() else {on_error.call("Sign in again".into());return;};
                let expected_location_stock = location_balance.read().as_ref().and_then(|r|r.as_ref().ok()).copied();
                if mode()!="Stock In" && (!location_balance.finished() || expected_location_stock.is_none()) {on_error.call("Wait for location stock to load".into());return;}
                if mode()=="Stock In" && batch().is_empty() {on_error.call("Wait for the batch number. Check the database numbering migration if it cannot load.".into());return;}
                if !location_names.contains(&location()) {on_error.call("Select an available location. Add locations on the Locations page if needed.".into());return;}
                if mode()=="Stock In" && !no_expire() && expiry().is_empty() {on_error.call("Enter an expiry date or select No Expire.".into());return;}
                let (Ok(quantity),Ok(unit_cost))=(qty().parse::<i32>(),if mode()=="Stock In" {cost().parse::<f64>()}else{Ok(product.cost)}) else {on_error.call("Enter valid quantity and cost".into());return;};
                if crate::is_variant_product(&product) && variant().is_empty() {on_error.call("Select a variant.".into());return;}
                let receipt=inventory::Receipt {product_id:product.id,variant_id:variant().parse().ok(),supplier_id:if mode()=="Stock In"{supplier().parse().ok()}else{None},quantity,unit_cost,location:location(),batch:batch(),expiry:if no_expire(){String::new()}else{expiry()},reason:if mode()=="Stock In"{"Stock received".into()}else{reason()},reference:if mode()=="Stock In"{String::new()}else{reference()},received_by:actor(),notes:notes(),expected_stock:product.stock,expected_location_stock};
                let operation=mode();
                if operation=="Stock In" {if let Err(e)=receipt.validate(){on_error.call(e.to_string());return;}}
                busy.set(true);let source=db_form.clone();spawn(async move {let result=async {let pool=connect(&source.database_config()?).await?;if operation=="Stock In" {inventory::receive(&pool,&receipt).await}else{inventory::change(&pool,&receipt,operation=="Adjustment",&operator).await}}.await;busy.set(false);match result {Ok(())=>on_saved.call(()),Err(e)=>on_error.call(format!("{e:#}"))}});
            },if busy(){"Saving..."}else{"Save {mode}"}}
        }
    }}}
}
