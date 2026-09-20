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
            header {class:"customers_header customers_touch_header",h2 {"Inventory"} div {class:"customers_actions",button {disabled:!data.finished(),onclick:move |_|{data.restart();revision+=1;},"Refresh"}}}
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
                        div {class:"inventory_detail_toolbar",button {onclick:move |_|receiving.set(true),"View Movements"}}
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
        if receiving() {if let Some(product)=current {div {class:"modal_backdrop",section {class:"customer_dialog inventory_receive",role:"dialog",aria_modal:"true",aria_label:"Stock movements",button {onclick:move |_|receiving.set(false),"Close"} h2 {"{product.name}"} MovementList {product_id:product.id,db_form:db_form.clone()}}}}}
        if !error().is_empty(){MessageBox {title:"Inventory Error".to_string(),message:error(),on_close:move |_|error.set(String::new())}}
    }
}

#[component]
fn MovementList(product_id: i32, db_form: DbForm) -> Element {
    let data = use_resource(move || {
        let source = db_form.clone();
        async move {
            async {
                inventory::movements(&connect(&source.database_config()?).await?, product_id).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let mut count = use_signal(|| 30usize);
    rsx! {section {class:"inventory_movements",h3 {"Stock movements"}
        match data.read().as_ref() {
            None=>rsx!{p {role:"status","Loading..."}},
            Some(Err(e))=>rsx!{p {role:"alert","{e}"}},
            Some(Ok(rows))=>rsx!{
                if rows.is_empty(){p {"No stock movements."}}
                for row in rows.iter().take(count()) {article {class:"inventory_movement",strong {"{row.kind} · " {crate::format_qty(row.quantity)}} small {"{row.created_at} · {row.location}"} div {{format!("{} → {}",crate::format_qty(row.old_stock),crate::format_qty(row.new_stock))}} p {"{row.reason}"} small {"{row.reference} · {row.created_by}"} if let Some(id)=row.variant_id {small {"Variant #{id}"}}}}
                if rows.len()>count(){button {onclick:move |_|count+=30,"Load more"}}
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
    let mut qty = use_signal(String::new);
    let mut cost = use_signal(|| product.cost.to_string());
    let mut variant = use_signal(String::new);
    let mut location = use_signal(|| {
        location_names
            .first()
            .cloned()
            .unwrap_or_default()
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
            label {if mode()=="Adjustment"{"New total quantity"}else{"Quantity"} input {r#type:"number",min:if mode()=="Adjustment"{"0"}else{"1"},step:"1",disabled:busy(),value:"{qty}",oninput:move |e|qty.set(e.value())}}
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
            label {if mode()=="Stock In"{"Received by"}else{"Recorded by"} input {disabled:busy(),value:"{actor}",oninput:move |e|actor.set(e.value())}}
            label {class:"inventory_notes_field","Notes" textarea {disabled:busy(),value:"{notes}",oninput:move |e|notes.set(e.value())}}
        }
        div {class:"customers_actions inventory_save_actions",button {disabled:busy(),onclick:move |_|on_close.call(()),"Cancel"}
            button {class:"customer_primary",disabled:busy(),onclick:move |_|{
                if busy(){return;}
                if mode()=="Stock In" && batch().is_empty() {on_error.call("Wait for the batch number. Check the database numbering migration if it cannot load.".into());return;}
                if !location_names.contains(&location()) {on_error.call("Select an available location. Add locations on the Locations page if needed.".into());return;}
                if mode()=="Stock In" && !no_expire() && expiry().is_empty() {on_error.call("Enter an expiry date or select No Expire.".into());return;}
                let (Ok(quantity),Ok(unit_cost))=(qty().parse::<i32>(),if mode()=="Stock In" {cost().parse::<f64>()}else{Ok(product.cost)}) else {on_error.call("Enter valid quantity and cost".into());return;};
                if crate::is_variant_product(&product) && variant().is_empty() {on_error.call("Select a variant.".into());return;}
                let receipt=inventory::Receipt {product_id:product.id,variant_id:variant().parse().ok(),supplier_id:if mode()=="Stock In"{supplier().parse().ok()}else{None},quantity,unit_cost,location:location(),batch:batch(),expiry:if no_expire(){String::new()}else{expiry()},reason:if mode()=="Stock In"{"Stock received".into()}else{reason()},reference:if mode()=="Stock In"{String::new()}else{reference()},received_by:actor(),notes:notes(),expected_stock:product.stock};
                let operation=mode();
                if operation=="Stock In" {if let Err(e)=receipt.validate(){on_error.call(e.to_string());return;}}
                busy.set(true);let source=db_form.clone();spawn(async move {let result=async {let pool=connect(&source.database_config()?).await?;if operation=="Stock In" {inventory::receive(&pool,&receipt).await}else{inventory::change(&pool,&receipt,operation=="Adjustment").await}}.await;busy.set(false);match result {Ok(())=>on_saved.call(()),Err(e)=>on_error.call(format!("{e:#}"))}});
            },if busy(){"Saving..."}else{"Save {mode}"}}
        }
    }}}
}
