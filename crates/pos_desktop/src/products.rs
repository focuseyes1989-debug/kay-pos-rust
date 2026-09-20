use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{catalog, connect, db, Category, Product};

fn blank() -> Product {
    Product {
        id: 0,
        name: String::new(),
        category_id: None,
        category_name: None,
        sku: None,
        barcode: None,
        price: 0.0,
        cost: 0.0,
        stock: 0.0,
        low_stock: 0.0,
        sold_by: Some("Each".into()),
        image_filename: None,
        image_data_url: None,
        variants: vec![],
        price_tiers: vec![],
    }
}
fn kind(p: &Product) -> &str {
    if crate::is_service_product(p) {
        "Service"
    } else if crate::is_variant_product(p) {
        "Variants"
    } else {
        p.sold_by.as_deref().unwrap_or("Each")
    }
}

#[component]
pub fn ProductsPage(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let form = source.clone();
        async move {
            let result = async {
                let pool = connect(&form.database_config()?).await?;
                tokio::try_join!(
                    db::search_product_metadata(&pool, "", 0),
                    db::list_categories(&pool)
                )
            }
            .await;
            result.map_err(|e| format!("{e:#}"))
        }
    });
    let mut query = use_signal(String::new);
    let mut category = use_signal(String::new);
    let mut mode = use_signal(String::new);
    let mut count = use_signal(|| 50usize);
    let mut editor = use_signal(|| None::<Product>);
    let mut deletion = use_signal(|| None::<Product>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let loading = !data.finished();
    let state = data.read();
    let (all, categories) = match state.as_ref() {
        Some(Ok(v)) => v.clone(),
        _ => (vec![], vec![]),
    };
    let search = query().to_lowercase();
    let mut rows = all
        .into_iter()
        .filter(|p| {
            (category().is_empty()
                || p.category_id.map(|id| id.to_string()).as_deref() == Some(category().as_str()))
                && (mode().is_empty() || kind(p) == mode())
                && format!(
                    "{} {} {}",
                    p.name,
                    p.sku.as_deref().unwrap_or(""),
                    p.barcode.as_deref().unwrap_or("")
                )
                .to_lowercase()
                .contains(&search)
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    rsx! {
        section { class:"customers_page products_touch",
            header { class:"customers_header customers_touch_header", h2 { "Products" }
                div { class:"customers_actions",
                    button { disabled: loading, onclick:move |_|data.restart(),"Refresh" }
                    button { class:"customer_primary",onclick:move |_|editor.set(Some(blank())),"+ Add Item" }
                }
            }
            section { class:"products_items",
                header { class:"products_items_header", h3 { "Items" } small { "{rows.len().min(count())} loaded · {rows.len()} items" } }
                div { class:"products_filters",
                    label { "Search" input { placeholder:"Product, SKU or barcode",value:"{query}",oninput:move |e| { query.set(e.value());count.set(50); } } }
                    label { "Category" select { value:"{category}",onchange:move |e|{category.set(e.value());count.set(50);},option { value:"","All categories" } for c in &categories { option { value:"{c.id}","{c.name}" } } } }
                    label { "Product type" select { value:"{mode}",onchange:move |e|{mode.set(e.value());count.set(50);},option { value:"","All types" } for item in ["Each","Service","Variants","Wholesale"] { option { "{item}" } } } }
                    button { onclick:move |_|{query.set(String::new());category.set(String::new());mode.set(String::new());count.set(50);},"Reset" }
                }
                div { class:"products_manage_list", aria_busy: loading,
                    if loading {
                        div { class: "product_loading", role: "status", aria_live: "polite",
                            span { class: "loading_spinner", aria_hidden: "true" }
                            strong { "Loading..." }
                            small { "Product cards are loading" }
                        }
                    } else {
                    if let Some(Err(message))=state.as_ref() { p { role:"alert","{message}" } }
                    if matches!(state.as_ref(),Some(Ok(_))) && rows.is_empty() { p { "No products found." } }
                    for p in rows.iter().take(count()) {
                        article { class:"product_manage_row",key:"{p.id}",
                            div { class:"product_manage_image", crate::CatalogImage { key:"{p.id}", id:p.id, name:p.name.clone() } }
                            div { class:"product_manage_identity", div { strong { "{p.name}" } span { class:"product_mode", "data-kind":kind(p), "{kind(p)}" } } small { "{p.category_name.as_deref().unwrap_or_default()} · {p.sku.as_deref().or(p.barcode.as_deref()).unwrap_or_default()}" } }
                            div { class:"product_manage_price", strong { "{crate::format_ks(p.price)}" } small { if crate::is_service_product(p) { "No stock tracking" } else { "Stock: " {crate::format_qty(if crate::is_variant_product(p) { p.variants.iter().map(|v|v.stock).sum() } else {p.stock})} } } }
                            div { class:"customers_actions",
                                button { onclick:{let p=p.clone();move |_|editor.set(Some(p.clone()))},"Edit" }
                                button { class:"product_delete",onclick:{let p=p.clone();move |_|deletion.set(Some(p.clone()))},"Delete" }
                            }
                        }
                    }
                    if count()<rows.len() { button { onclick:move |_|count.set(count()+50),"Load more" } }
                    }
                }
            }
        }
        if let Some(product)=editor() { ProductEditor { product,categories,db_form:db_form.clone(),on_close:move |_|editor.set(None),on_saved:move |_|{editor.set(None);data.restart();},on_error:move |message|error.set(message) } }
        if let Some(p)=deletion() {
            div { class:"modal_backdrop",section { class:"customer_dialog",role:"dialog",aria_modal:"true",aria_label:"Delete product",h2 { "Delete product?" } p { "{p.name}" }
                div { class:"customers_actions",button { disabled:busy(),onclick:move |_|deletion.set(None),"Cancel" }
                    button { disabled:busy(),onclick:move |_|{if busy(){return;} let source=db_form.clone();busy.set(true);spawn(async move { let result=async {catalog::delete(&connect(&source.database_config()?).await?,p.id).await}.await;busy.set(false);match result {Ok(())=>{deletion.set(None);data.restart();},Err(e)=>error.set(format!("{e:#}"))} });},"Delete" }
                }
            } }
        }
        if !error().is_empty() { MessageBox {title:"Product Error".to_string(),message:error(),on_close:move |_|error.set(String::new())} }
    }
}

#[component]
fn ProductEditor(
    product: Product,
    categories: Vec<Category>,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut form = use_signal(|| product.clone());
    let mut price = use_signal(|| product.price.to_string());
    let mut low = use_signal(|| product.low_stock.to_string());
    let mut saving = use_signal(|| false);
    let mut pending_image = use_signal(|| None::<catalog::ProductImage>);
    let mut choosing_image = use_signal(|| false);
    let category_id = form.read().category_id;
    let missing_category = category_id.filter(|id| !categories.iter().any(|c| c.id == *id));
    let product_type = form
        .read()
        .sold_by
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Each".into());
    let original_type = product
        .sold_by
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Each".into());
    rsx! { div {class:"modal_backdrop",section {class:"customer_dialog product_editor_dialog",role:"dialog",aria_modal:"true",aria_label:"Product",
        header {class:"product_editor_header",
            h2 { if product.id==0 {"Add Item"} else {"Edit Item"} }
            button {disabled:saving(),aria_label:"Close",onclick:move |_|on_close.call(()),"×"}
        }
        div {class:if product_type.eq_ignore_ascii_case("Variants") {"product_editor_body product_editor_variants"} else {"product_editor_body"},
        div {class:"product_editor_main",
        section {class:"product_editor_section",h3 {"Product information"}
        div {class:"customer_form",
            label {"Product name *" input {value:"{form().name}",disabled:saving(),onmounted:move |event|async move {let _=event.set_focus(true).await;},oninput:move |e|form.write().name=e.value()} }
            label {"Product type" select {value:"{product_type}",disabled:saving(),onchange:move |e|form.write().sold_by=Some(e.value()),
                for mode in ["Each","Service","Variants"] {option {value:"{mode}",selected:product_type.eq_ignore_ascii_case(mode),"{mode}"}}
                if original_type.eq_ignore_ascii_case("Wholesale") {option {value:"{original_type}",selected:product_type==original_type,"Wholesale"}}
            } }
            label {"Category" select {disabled:saving(),value:category_id.map(|id|id.to_string()).unwrap_or_default(),onchange:move |e|form.write().category_id=e.value().parse().ok(),
                option {value:"",selected:category_id.is_none(),"No category"}
                if let Some(id)=missing_category {option {value:"{id}",selected:true,{product.category_name.clone().unwrap_or_else(||format!("Category #{id}"))}}}
                for c in categories.iter() {option {key:"{c.id}",value:"{c.id}",selected:category_id==Some(c.id),"{c.name}"}}
            } }
        } }
        if !product_type.eq_ignore_ascii_case("Variants") {
        section {class:"product_editor_section",h3 {"Sales & inventory"}
        div {class:"customer_form",
            if !product_type.eq_ignore_ascii_case("Service") {
            label {"Price" input {r#type:"number",min:"0",step:"0.01",value:"{price}",disabled:saving(),oninput:move |e|price.set(e.value())} }
            label {"Stock" input {readonly:true,value:crate::format_qty(product.stock),title:"Managed through inventory"} }
            label {"Low stock" input {r#type:"number",min:"0",value:"{low}",disabled:saving(),oninput:move |e|low.set(e.value())} }
            }
            label {"SKU" input {placeholder:"Auto",value:form().sku.unwrap_or_default(),disabled:saving(),oninput:move |e|form.write().sku=Some(e.value())} }
            label {"Barcode" input {value:form().barcode.unwrap_or_default(),disabled:saving(),oninput:move |e|form.write().barcode=Some(e.value())} }
        } }
        }
        if product_type.eq_ignore_ascii_case("Each") || product_type.eq_ignore_ascii_case("Wholesale") {
            section {class:"product_editor_section", h3 {"Wholesale"}
                for (index,tier) in form().price_tiers.iter().enumerate() {
                    div {class:"customer_form wholesale_tier_row",
                        label {"Min Qty" input {r#type:"number",min:"1",value:"{tier.min_qty}",disabled:saving(),oninput:move |e|form.write().price_tiers[index].min_qty=e.value().parse().unwrap_or(0)} }
                        label {"Unit Qty" input {r#type:"number",min:"1",value:"{tier.unit_multiplier}",disabled:saving(),oninput:move |e|form.write().price_tiers[index].unit_multiplier=e.value().parse().unwrap_or(0)} }
                        label {"Unit" input {value:tier.unit_label.clone().unwrap_or_default(),disabled:saving(),oninput:move |e|form.write().price_tiers[index].unit_label=Some(e.value())} }
                        label {"Unit Price" input {r#type:"number",min:"0.01",step:"0.01",value:"{tier.unit_price}",disabled:saving(),oninput:move |e|form.write().price_tiers[index].unit_price=e.value().parse().unwrap_or(f64::NAN)} }
                        button {disabled:saving(),onclick:move |_|{form.write().price_tiers.remove(index);},"Remove tier"}
                    }
                }
                button {disabled:saving(),onclick:move |_|form.write().price_tiers.push(pos_core::ProductPriceTier {id:0,product_id:product.id,min_qty:1,unit_multiplier:1,unit_label:None,unit_price:0.0}),"+ Add tier"}
            }
        }
        }
        aside {class:"product_editor_side",
            section {class:"product_editor_section",h3 {"Product image"}
                div {class:"product_editor_preview product_image_preview",if let Some(url)=&form().image_data_url {img {src:"{url}",alt:"{form().name}"}} else if form().id>0 {crate::CatalogImage {key:"{form().id}",id:form().id,name:form().name.clone()}} else {span {class:"product_image_empty","Add a product image"}}}
                div {class:"product_image_label","Product image"}
                div {class:"product_image_picker",
                button {class:"product_image_choose",disabled:saving() || choosing_image(),onclick:move |_|{
                    choosing_image.set(true);
                    spawn(async move {
                        if let Some(file)=rfd::AsyncFileDialog::new().add_filter("Product image", &["png","jpg","jpeg","gif","webp"]).pick_file().await {
                            let result=async {
                                let metadata=tokio::fs::metadata(file.path()).await?;
                                anyhow::ensure!(metadata.len() <= 10 * 1024 * 1024,"Image must be 10 MB or smaller");
                                let data=tokio::fs::read(file.path()).await?;
                                catalog::ProductImage::from_bytes(file.file_name(),data)
                            }.await;
                            match result {
                                Ok(image)=>{let mut p=form.write();p.image_data_url=Some(image.data_url());p.image_filename=Some(file.file_name());drop(p);pending_image.set(Some(image));}
                                Err(e)=>on_error.call(format!("{e:#}")),
                            }
                        }
                        choosing_image.set(false);
                    });
                },if choosing_image(){"Choosing..."}else{"Choose File"}}
                span {class:"product_image_filename",title:form().image_filename.unwrap_or_default(),{form().image_filename.unwrap_or_else(||"No file chosen".into())}}
                }
            }
        }
        if product_type.eq_ignore_ascii_case("Variants") {
            section {class:"product_editor_section product_editor_variant_section",h3 {"Variants"}
                for (index,variant) in form().variants.iter().enumerate() {
                    div {class:"variant_editor_row",
                        div {class:"customer_form variant_primary_row variant_compact_row",
                            for (key,label,value) in [("size","Size",variant.size.clone()),("color","Color",variant.color.clone()),("sku","SKU",variant.sku.clone()),("barcode","Barcode",variant.barcode.clone())] {
                                label {"{label}" input {value:value.unwrap_or_default(),disabled:saving(),oninput:move |e|{let mut p=form.write();let v=&mut p.variants[index];match key {"size"=>v.size=Some(e.value()),"color"=>v.color=Some(e.value()),"sku"=>v.sku=Some(e.value()),_=>v.barcode=Some(e.value())}}} }
                            }
                            label {"Price" input {r#type:"number",min:"0",step:"any",value:"{variant.price}",disabled:saving(),oninput:move |e|form.write().variants[index].price=e.value().parse().unwrap_or(f64::NAN)} }
                            label {"Stock Alert" input {r#type:"number",min:"0",step:"1",value:"{variant.low_stock}",disabled:saving(),oninput:move |e|form.write().variants[index].low_stock=e.value().parse().unwrap_or(f64::NAN)} }
                            label {"Wholesale min qty" input {r#type:"number",min:"0",value:"{variant.wholesale_min_qty}",disabled:saving(),oninput:move |e|form.write().variants[index].wholesale_min_qty=e.value().parse().unwrap_or(-1)} }
                            label {"Wholesale price" input {r#type:"number",min:"0",step:"any",value:"{variant.wholesale_price}",disabled:saving(),oninput:move |e|form.write().variants[index].wholesale_price=e.value().parse().unwrap_or(f64::NAN)} }
                        }
                        button {disabled:saving() || variant.stock!=0.0,onclick:move |_|{form.write().variants.remove(index);},"Remove variant"}
                    }
                }
                button {disabled:saving(),onclick:move |_|{let sku=next_variant_sku(&form());form.write().variants.push(pos_core::ProductVariant {variant_id:0,product_id:product.id,size:None,color:None,sku:Some(sku),barcode:None,price:0.0,cost:0.0,stock:0.0,low_stock:0.0,wholesale_min_qty:0,wholesale_price:0.0});},"+ Add variant"}
            }
        }
        }
        div {class:"customers_actions product_editor_footer",button {disabled:saving(),onclick:move |_|on_close.call(()),"Cancel"}
            button {class:"customer_primary",disabled:saving() || choosing_image(),onclick:move |_|{
                if saving(){return;} let mut p=form();
                if !crate::is_variant_product(&p) && !p.sold_by.as_deref().unwrap_or("Each").eq_ignore_ascii_case("Service") {let (Ok(price),Ok(low))=(price().parse::<f64>(),low().parse::<f64>()) else {on_error.call("Enter valid numeric values.".into());return;};p.price=price;p.low_stock=low;}
                let image=pending_image();let source=db_form.clone();saving.set(true);spawn(async move {let result=async {catalog::save_with_image(&connect(&source.database_config()?).await?,&p,image.as_ref()).await}.await;saving.set(false);match result {Ok(())=>on_saved.call(()),Err(e)=>on_error.call(format!("{e:#}"))}});
            },if saving(){"Saving..."}else{"Save Item"}}
        }
    } } }
}

fn next_variant_sku(product: &Product) -> String {
    let prefix = if product.id == 0 {
        "VAR-NEW".to_string()
    } else {
        format!("P{:05}", product.id)
    };
    (1..)
        .map(|n| format!("{prefix}-{n:02}"))
        .find(|sku| !product.variants.iter().any(|v| v.sku.as_ref() == Some(sku)))
        .unwrap()
}
