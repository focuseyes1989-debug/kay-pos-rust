use crate::{format_qty, DbForm};
use dioxus::prelude::*;
use pos_core::{connect, db, inventory, locations, stock_transfer::Transfer};

#[component]
pub fn StockTransferDialog(
    db_form: DbForm,
    actor: pos_core::auth::Session,
    #[props(default)] initial_stock:Option<locations::Stock>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<String>,
) -> Element {
    let source = db_form.clone();
    let data = use_resource(move || {
        let source = source.clone();
        async move {
            async {
                let pool = connect(&source.database_config()?).await?;
                tokio::try_join!(
                    db::search_product_metadata(&pool, "", 0),
                    locations::list_names(&pool)
                )
            }
            .await
            .map_err(|e: anyhow::Error| format!("{e:#}"))
        }
    });
    let mut search = use_signal(String::new);
    let mut product_id = use_signal(|| initial_stock.as_ref().map(|r|r.id).unwrap_or(0));
    let mut variant_id = use_signal(|| initial_stock.as_ref().and_then(|r|r.variant_id));
    let mut from = use_signal(||initial_stock.as_ref().map(|r|r.location.clone()).unwrap_or_default());
    let mut to = use_signal(String::new);
    let mut quantity = use_signal(||initial_stock.as_ref().filter(|r|r.quantity>0.0 && r.quantity.fract()==0.0 && r.quantity<=i32::MAX as f64).map(|r|format_qty(r.quantity)).unwrap_or_else(||"1".into()));
    let mut reason = use_signal(String::new);
    let mut notes = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let date = use_signal(|| chrono::Local::now().format("%Y-%m-%d %H:%M").to_string());
    let reference =
        use_signal(|| format!("TRF-{}", chrono::Local::now().format("%Y%m%d%H%M%S%6f")));
    let source = db_form.clone();
    let mut stock = use_resource(move || {
        let source = source.clone();
        let id = product_id();
        let variant = variant_id();
        let location = from();
        async move {
            if id == 0 {
                return Ok((vec![], 0.0));
            }
            async {
                let pool = connect(&source.database_config()?).await?;
                tokio::try_join!(
                    inventory::stock_location_names(&pool, id, variant),
                    inventory::location_stock(&pool, id, variant, &location)
                )
            }
            .await
            .map_err(|e: anyhow::Error| format!("{e:#}"))
        }
    });
    let source = db_form.clone();
    let image = use_resource(move || {
        let source = source.clone();
        let id = product_id();
        async move {
            if id == 0 {
                return None;
            }
            let pool = connect(&source.database_config().ok()?).await.ok()?;
            db::product_image(&pool, id).await.ok().flatten()
        }
    });
    let (products, destinations) = data
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let product = products.iter().find(|p| p.id == product_id()).cloned();
    let (sources, available) = stock
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let term = search().to_lowercase();
    let filtered = products.iter().filter(|p| {
        !matches!(
            p.sold_by
                .as_deref()
                .unwrap_or("Each")
                .to_lowercase()
                .as_str(),
            "service" | "restaurant"
        ) && (p.id == product_id()
            || format!(
                "{} {} {}",
                p.name,
                p.sku.as_deref().unwrap_or(""),
                p.barcode.as_deref().unwrap_or("")
            )
            .to_lowercase()
            .contains(&term))
    });
    rsx! {div {class:"modal_backdrop",section {class:"stock_transfer_dialog",role:"dialog",aria_modal:"true",aria_label:"Stock transfer",
     header {div {class:"stock_transfer_heading",h2 {"Stock Transfer"} p {"Move stock between locations"}} small {class:"stock_transfer_reference","{reference}"} button {aria_label:"Close stock transfer",disabled:busy(),onclick:move |_|on_close.call(()),crate::icons::Icon {name:"close"}}}
     if let Some(Err(message))=data.read().as_ref() {p {class:"stock_transfer_error",role:"alert","{message}"}}
     if let Some(Err(message))=stock.read().as_ref() {p {class:"stock_transfer_error",role:"alert","{message}"}}
     if !error().is_empty(){p {class:"stock_transfer_error",role:"alert","{error}"}}
     div {class:"stock_transfer_columns",
      div {class:"stock_transfer_form",
       section {class:"stock_transfer_section",h3 {"1 · Select product"}
       label {span {"Search"} input {r#type:"search",placeholder:"Product name, barcode or SKU",disabled:busy(),value:"{search}",oninput:move |e|search.set(e.value())}}
       label {span {"Product"} select {disabled:busy()||!data.finished(),value:"{product_id}",onchange:move |e|{product_id.set(e.value().parse().unwrap_or(0));variant_id.set(None);from.set(String::new());error.set(String::new());},option {value:"0","Select product..."} for p in filtered {option {value:"{p.id}","{p.name}"}}}}
       if let Some(p)=&product {if p.sold_by.as_deref().is_some_and(|m|m.eq_ignore_ascii_case("variants")) {
        label {span {"Variant"} select {disabled:busy(),value:variant_id().map(|id|id.to_string()).unwrap_or_default(),onchange:move |e|{variant_id.set(e.value().parse().ok());from.set(String::new());},option {value:"","Select variant..."} for v in &p.variants {option {value:"{v.variant_id}",{format!("{} / {} / {}",v.size.as_deref().unwrap_or(""),v.color.as_deref().unwrap_or(""),v.sku.as_deref().unwrap_or(""))}}}}}
       }}
       }
       section {class:"stock_transfer_section",h3 {"2 · Transfer route"}
       div {class:"stock_transfer_route",
       label {span {"From"} select {disabled:busy()||!stock.finished(),value:"{from}",onchange:move |e|{from.set(e.value());error.set(String::new());},option {value:"","Select From location..."} for name in sources {option {value:"{name}","{name}"}}}}
       label {span {"To"} select {disabled:busy(),value:"{to}",onchange:move |e|{to.set(e.value());error.set(String::new());},option {value:"","Select To location..."} for location in destinations {option {value:"{location.name}","{location.name}"}}}}
       }
       div {class:"stock_transfer_balance",
       div {class:"stock_transfer_available",div {span {"Available at source"} strong {if stock.finished(){"{format_qty(available)}"}else{"Loading..."}}} button {class:"stock_transfer_refresh",disabled:busy()||!stock.finished(),onclick:move |_|stock.restart(),title:"Refresh stock",aria_label:"Refresh available stock",crate::icons::Icon {name:"refresh"}}}
       label {span {"Quantity"} input {r#type:"number",min:"1",step:"1",disabled:busy(),value:"{quantity}",oninput:move |e|quantity.set(e.value())}}
       }
       }
       section {class:"stock_transfer_section",h3 {"3 · Transfer details"}
       div {class:"stock_transfer_details_grid",
       label {span {"Reason"} input {disabled:busy(),placeholder:"Reason for transfer",value:"{reason}",oninput:move |e|reason.set(e.value())}}
       label {span {"Date"} input {value:"{date}",readonly:true}}
       }
    label {span {"Notes (optional)"} textarea {disabled:busy(),placeholder:"Additional notes",value:"{notes}",oninput:move |e|notes.set(e.value())}}
       }
      }
      aside {class:"stock_transfer_product",
       strong {"Product information"}
       if let Some(Some(url))=image.read().as_ref(){img {src:"{url}",alt:"Product image"}}else{div {class:"stock_transfer_no_image","No image available"}}
       if let Some(p)=product {
        h3 {"{p.name}"}
        dl {
         div {dt {"SKU"} dd {{p.sku.as_deref().unwrap_or("—")}}}
         div {dt {"Barcode"} dd {{p.barcode.as_deref().unwrap_or("—")}}}
         div {dt {"Type"} dd {{p.sold_by.as_deref().unwrap_or("Each")}}}
        }
        div {class:"stock_transfer_product_stats",div {span {"Total stock"} strong {"{format_qty(p.stock)}"}} div {span {"Unit cost"} strong {"{crate::format_ks(p.cost)}"}}}
       }else{p {"Select a product to view its details."}}
      }
     }
     footer {small {"Total stock stays unchanged."} div {class:"stock_transfer_actions",button {disabled:busy(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel {label:"Cancel"}}
      button {class:"customer_primary",disabled:busy()||!stock.finished()||!data.finished(),onclick:move |_|{
       if busy(){return;}
       let request=Transfer {product_id:product_id(),variant_id:variant_id(),from:from(),to:to(),quantity:quantity().parse().unwrap_or(0),reason:reason(),notes:notes(),reference:reference(),expected_available:available};
       if let Err(e)=request.validate(){error.set(e.to_string());return;}
       if !stock.read().as_ref().is_some_and(|r|r.is_ok()){error.set("Refresh available stock first".into());return;}
       busy.set(true);error.set(String::new());let source=db_form.clone();let actor=actor.clone();spawn(async move {let result=async {let pool=connect(&source.database_config()?).await?;pos_core::stock_transfer::save(&pool,&actor,&request).await}.await;busy.set(false);match result {Ok(())=>on_saved.call(request.reference),Err(e)=>{error.set(format!("{e:#}"));stock.restart();}}});
      },if busy(){"Transferring..."}else{crate::icons::Icon {name:"undo"} "Transfer Stock"}}
     }
     }
    }}}
}
