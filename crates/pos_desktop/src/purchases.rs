use crate::{
    icons::{ActionLabel, Icon},
    purchase_pending::{self, Pending},
    DbForm,
};
use chrono::Datelike;
use dioxus::prelude::*;
use pos_core::{
    auth::Session,
    purchases::{self, Action, Command, Draft, Line, Payment, Product, Supplier},
};

#[derive(Clone)]
struct Editor {
    id: Option<i32>,
    revision: i32,
    draft: Draft,
}

fn dispatch(
    source: DbForm,
    actor: Session,
    command: Command,
    mut pending: Signal<Option<Pending>>,
    mut busy: Signal<bool>,
    mut notice: Signal<String>,
    mut refresh: Signal<u64>,
) {
    if busy() {
        return;
    }
    let value = Pending {
        username: actor.username().into(),
        command,
    };
    if let Err(e) = purchase_pending::save(&source, &value) {
        notice.set(format!("{e:#}"));
        return;
    }
    pending.set(Some(value.clone()));
    busy.set(true);
    notice.set(String::new());
    spawn(async move {
        let result = async {
            let pool = pos_core::connect(&source.database_config()?).await?;
            let id = purchases::execute(&pool, &actor, &value.command).await?;
            purchase_pending::clear(&source, &value)?;
            Ok::<_, anyhow::Error>(id)
        }
        .await;
        match result {
            Ok(id) => {
                pending.set(None);
                notice.set(format!("Saved. Record #{id}"));
                refresh += 1;
            }
            Err(e) => notice.set(format!("{e:#}")),
        }
        busy.set(false);
    });
}

#[component]
pub fn PurchasesPage(db_form: DbForm, actor: Session) -> Element {
    let today = chrono::Local::now().date_naive();
    let mut from = use_signal(|| today.with_day(1).unwrap().to_string());
    let mut to = use_signal(|| today.to_string());
    let mut dates = use_signal(|| (from(), to()));
    let mut refresh = use_signal(|| 0u64);
    let mut tab = use_signal(|| 0usize);
    let mut query = use_signal(String::new);
    let mut supplier = use_signal(|| 0i32);
    let mut status = use_signal(String::new);
    let mut page = use_signal(|| 0usize);
    let mut editor = use_signal(|| None::<Editor>);
    let mut operation = use_signal(|| None::<Action>);
    let mut details = use_signal(|| None::<i32>);
    let mut busy = use_signal(|| false);
    let mut exporting = use_signal(|| false);
    let initial = use_hook(|| match purchase_pending::load(&db_form) {
        Ok(p) => (p, String::new()),
        Err(e) => (None, format!("Purchase journal error: {e:#}")),
    });
    let journal_error = initial.1.clone();
    let mut pending = use_signal(|| initial.0);
    let mut notice = use_signal(|| initial.1);
    let source = db_form.clone();
    let user = actor.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        let user = user.clone();
        let (a, b) = dates();
        let _ = refresh();
        async move {
            async {
                let pool = pos_core::connect(&source.database_config()?).await?;
                purchases::load(&pool, &user, &a, &b).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let loading = !data.finished();
    let locked = busy() || pending.read().is_some() || !journal_error.is_empty();
    let snapshot = data.read();
    let loaded = if loading {
        None
    } else {
        snapshot.as_ref().and_then(|r| r.as_ref().ok())
    };
    let source = db_form.clone();
    let user = actor.clone();
    let on_action = Callback::new(move |action: Action| {
        if locked {
            return;
        }
        editor.set(None);
        operation.set(None);
        dispatch(
            source.clone(),
            user.clone(),
            Command {
                request_id: pos_core::auth::new_request_id(),
                action,
            },
            pending,
            busy,
            notice,
            refresh,
        );
    });
    let search = query().to_lowercase();
    let draft_rows: Vec<_> = loaded
        .into_iter()
        .flat_map(|d| d.drafts.iter())
        .filter(|r| {
            (supplier() == 0 || r.supplier_id == supplier())
                && (status().is_empty() || r.status == status())
                && format!("{} {}", r.number, r.supplier)
                    .to_lowercase()
                    .contains(&search)
        })
        .cloned()
        .collect();
    let order_rows: Vec<_> = loaded
        .into_iter()
        .flat_map(|d| d.orders.iter())
        .filter(|r| {
            (supplier() == 0 || r.supplier_id == Some(supplier()))
                && (status().is_empty() || r.payment_status == status())
                && format!("{} {}", r.number, r.supplier)
                    .to_lowercase()
                    .contains(&search)
        })
        .cloned()
        .collect();
    let count = if tab() == 0 {
        draft_rows.len()
    } else {
        order_rows.len()
    };
    let pages = count.div_ceil(25).max(1);
    let current = page().min(pages - 1);
    let currency = crate::regional::current().symbol().to_string();
    let snapshot_at = loaded.map(|d| d.loaded_at.clone()).unwrap_or_default();
    rsx! {section {class:"customers_page purchases_page",
        button {hidden:true,"data-page-refresh":"true",disabled:loading||locked,onclick:move |_|{refresh+=1;data.restart();},"Refresh"}
        header {class:"reports_header",h2 {ActionLabel{label:"Purchases"}}
            div {class:"customers_actions",
                button {disabled:loading||locked||exporting()||tab()!=1,onclick:{let orders=order_rows.clone();let source=db_form.clone();let actor=actor.clone();let currency=currency.clone();let snapshot_at=snapshot_at.clone();move |_|{
                    let orders=orders.clone();let source=source.clone();let actor=actor.clone();let currency=currency.clone();let snapshot_at=snapshot_at.clone();let (from,to)=dates();exporting.set(true);
                    spawn(async move{if let Some(file)=rfd::AsyncFileDialog::new().add_filter("Excel",&["xlsx"]).set_file_name("Purchase-History.xlsx").save_file().await{
                        let result=async{let pool=pos_core::connect(&source.database_config()?).await?;let mut connection=pool.acquire().await?;actor.authorize(&mut connection,pos_core::auth::Permission::Manage).await?;
                            use pos_core::reports::{Cell,Report,Table};
                            let mut rows=Vec::new();for r in orders {rows.push(vec![Cell::Text(r.number),r.supplier.into(),r.order_date.into(),Cell::Money(r.total.parse()?),Cell::Money(r.paid.parse()?),r.status.into(),r.payment_status.into()]);}
                            crate::reports::export(&Report{from,to,as_of:snapshot_at,warnings:vec!["Received orders by receipt date. Paid amounts reflect the current linked supplier ledger.".into()],tables:vec![Table{title:"Purchase History".into(),headers:["Order","Supplier","Date","Total","Paid","Status","Payment status"].iter().map(|s|s.to_string()).collect(),rows}]},&currency,file.path())
                        }.await;notice.set(match result{Ok(())=>"Export saved".into(),Err(e)=>format!("Export failed: {e:#}")});
                    }exporting.set(false);});
                }},ActionLabel{label:"Export Excel"}}
                button {class:"customer_primary",disabled:loading||locked||loaded.is_none(),onclick:move |_|editor.set(Some(Editor{id:None,revision:0,draft:Draft{supplier_id:supplier(),order_date:today.to_string(),discount:"0".into(),tax:"0".into(),notes:String::new(),lines:vec![]}})),ActionLabel{label:"Add purchase order"}}
                button {disabled:loading||locked||supplier()==0,onclick:move |_|operation.set(Some(Action::Pay(Payment{supplier_id:supplier(),po_id:None,amount:String::new(),date:today.to_string(),method:"Cash".into(),reference:String::new(),notes:String::new()}))),ActionLabel{label:"Supplier payment"}}
            }
        }
        div {class:"purchase_filters",
            label {"Search" input {r#type:"search",value:query(),placeholder:"Order or supplier",oninput:move |e|{query.set(e.value());page.set(0);}}}
            label {"Supplier" select {value:supplier().to_string(),onchange:move |e|{supplier.set(e.value().parse().unwrap_or(0));page.set(0);},option{value:"0","All suppliers"} if let Some(d)=loaded {for s in &d.suppliers {option{value:s.id.to_string(),"{s.name}"}}}}}
            if tab()!=2 {label {"Status" select {value:status(),onchange:move |e|{status.set(e.value());page.set(0);},option{value:"","All statuses"} for label in if tab()==0{vec!["pending","received","cancelled"]}else{vec!["Unpaid","Partial","Paid"]}{option{value:label,"{label}"}}}}}
            label {"From" input {r#type:"date",value:from(),oninput:move |e|from.set(e.value())}}
            label {"To" input {r#type:"date",value:to(),oninput:move |e|to.set(e.value())}}
            button {disabled:loading,onclick:move |_|{match pos_core::sale_summary::dates(&from(),&to()){Ok(_)=>{dates.set((from(),to()));page.set(0);refresh+=1;},Err(_)=>notice.set("Enter a valid date range".into())}},"Apply"}
        }
        div {class:"receipt_tabs reports_tabs",role:"tablist",for (i,label) in ["Purchase Orders","Purchase History","Supplier Ledger"].iter().enumerate(){button{role:"tab",aria_selected:tab()==i,class:if tab()==i{"active"}else{""},onclick:move |_|{tab.set(i);status.set(String::new());page.set(0);},"{label}"}}}
        if !notice().is_empty(){p{role:"status","{notice}"}}
        if loading {p{role:"status","Loading purchases..."}}
        else if let Some(Err(error))=snapshot.as_ref(){p{role:"alert","{error}"}}
        else if tab()==2 {
            if let Some(s)=loaded.and_then(|d|d.suppliers.iter().find(|s|s.id==supplier())) {
                div {key:"{supplier()}-{refresh()}-{dates().0}-{dates().1}",h3 {"{s.name}"} p {"Current ledger balance: {amount(&s.balance)}"}
                    Ledger {db_form:db_form.clone(),actor:actor.clone(),supplier_id:supplier(),from:dates().0,to:dates().1}
                }
            }else{p{"Select a supplier."}}
        }else{
            div {class:"reports_table_scroll",table{class:"customers_table",
                thead {tr {for h in ["Order","Supplier","Date","Total","Status","Actions"]{th{"{h}"}}}}
                tbody {
                    if tab()==0 {for r in draft_rows.iter().skip(current*25).take(25){tr{key:"draft-{r.id}",td{"{r.number}"}td{"{r.supplier}"}td{"{r.order_date}"}td{class:"reports_money","{amount(&r.total)}"}td{"{r.status}"}
                        td{div{class:"purchase_actions",
                            button {disabled:locked||r.status!="pending",onclick:{let r=r.clone();move |_|match serde_json::from_str(&r.body){Ok(draft)=>editor.set(Some(Editor{id:Some(r.id),revision:r.revision,draft})),Err(e)=>notice.set(format!("Draft cannot be read: {e}"))}},ActionLabel{label:"Edit"}}
                            button {disabled:locked||r.status!="pending",onclick:{let r=r.clone();move |_|operation.set(Some(Action::Receive{id:r.id,revision:r.revision,date:today.to_string()}))},ActionLabel{label:"Receive Order"}}
                            button {disabled:locked||r.status!="pending",onclick:{let r=r.clone();move |_|operation.set(Some(Action::Cancel{id:r.id,revision:r.revision,reason:String::new()}))},ActionLabel{label:"Cancel"}}
                        }}
                    }}}else{for r in order_rows.iter().skip(current*25).take(25){tr{key:"order-{r.id}",td{"{r.number}"}td{"{r.supplier}"}td{"{r.order_date}"}td{class:"reports_money","{amount(&r.total)}"}td{"{r.status} / {r.payment_status}" small {"Paid: {amount(&r.paid)}"} small {"Remaining: {amount(&purchases::remaining(&r.total,&r.paid).unwrap_or_default())}"}}
                        td{div{class:"purchase_actions",
                            button {onclick:{let id=r.id;move |_|details.set(Some(id))},ActionLabel{label:"View details"}}
                            button {disabled:locked||r.status!="completed"||r.supplier_id.is_none(),onclick:{let r=r.clone();move |_|operation.set(Some(Action::Pay(Payment{supplier_id:r.supplier_id.unwrap_or(0),po_id:Some(r.id),amount:purchases::remaining(&r.total,&r.paid).unwrap_or_default(),date:today.to_string(),method:"Cash".into(),reference:r.number.clone(),notes:String::new()})))},ActionLabel{label:"Supplier payment"}}
                        }}
                    }}}
                }
            }}
            if count==0 {p{"No orders in this period."}}
            footer{class:"reports_pagination",span{"{count} orders"}button{title:"Previous page",aria_label:"Previous page",disabled:current==0,onclick:move |_|page.set(current.saturating_sub(1)),Icon{name:"arrow_circle_left"}}span{"{current+1} / {pages}"}button{title:"Next page",aria_label:"Next page",disabled:current+1>=pages,onclick:move |_|page.set(current+1),Icon{name:"arrow_circle_right"}}}
        }
    }
    if let (Some(e),Some(d))=(editor(),loaded) {DraftEditor{id:e.id,revision:e.revision,draft:e.draft,suppliers:d.suppliers.clone(),products:d.products.clone(),locations:d.locations.clone(),on_close:move |_|editor.set(None),on_action}}
    if let Some(action)=operation(){OperationDialog{action,on_close:move |_|operation.set(None),on_action}}
    if let Some(id)=details(){OrderDetails{db_form:db_form.clone(),actor:actor.clone(),id,on_close:move |_|details.set(None)}}
    if let Some(value)=pending(){div{class:"modal_backdrop",section{class:"customer_dialog purchase_recovery",role:"dialog",aria_modal:"true",aria_label:"Unresolved purchase request",
        h2{"Unresolved purchase request"}p{"Operator: {value.username}"}p{"{value.command.request_id}"}
        if !notice().is_empty(){p{role:"alert","{notice}"}}
        div{class:"customers_actions",
            button{class:"customer_primary",disabled:busy()||value.username!=actor.username(),onclick:{let source=db_form.clone();let user=actor.clone();let command=value.command.clone();move |_|dispatch(source.clone(),user.clone(),command.clone(),pending,busy,notice,refresh)},"Retry same request"}
            button{disabled:busy()||value.username!=actor.username(),onclick:{let source=db_form.clone();let user=actor.clone();let value=value.clone();move |_|{
                let source=source.clone();let user=user.clone();let value=value.clone();busy.set(true);
                spawn(async move{let result=async{let pool=pos_core::connect(&source.database_config()?).await?;let saved=purchases::cancel_unsaved(&pool,&user,&value.command).await?;purchase_pending::clear(&source,&value)?;Ok::<_,anyhow::Error>(saved)}.await;busy.set(false);match result{Ok(saved)=>{pending.set(None);refresh+=1;notice.set(if let Some(id)=saved{format!("Already saved as record #{id}")}else{"Unsaved request cancelled".into()});},Err(e)=>notice.set(format!("{e:#}"))}});
            }},"Check and cancel unsaved"}
        }
    }}}
    }
}

fn amount(s: &str) -> String {
    s.parse::<f64>()
        .map(crate::format_ks)
        .unwrap_or_else(|_| "Unknown".into())
}

#[component]
fn DraftEditor(
    id: Option<i32>,
    revision: i32,
    draft: Draft,
    suppliers: Vec<Supplier>,
    products: Vec<Product>,
    locations: Vec<String>,
    on_close: EventHandler<()>,
    on_action: EventHandler<Action>,
) -> Element {
    let mut form = use_signal(|| draft);
    let mut choice = use_signal(String::new);
    let mut error = use_signal(String::new);
    let total = purchases::totals(&form())
        .ok()
        .map(|(_, n)| amount(&n.to_string()))
        .unwrap_or_else(|| "Unknown".into());
    rsx! {div{class:"modal_backdrop",section{class:"customer_dialog purchase_dialog",role:"dialog",aria_modal:"true",aria_label:"Purchase order",
        header{class:"reports_header",h2{if id.is_some(){"Edit purchase order"}else{"New purchase order"}}button{title:"Close",aria_label:"Close",onclick:move |_|on_close.call(()),Icon{name:"close"}}}
        div{class:"purchase_form",
            label{"Supplier" select{disabled:id.is_some(),value:form().supplier_id.to_string(),onchange:move |e|form.write().supplier_id=e.value().parse().unwrap_or(0),option{value:"0","Select supplier"}for s in suppliers.iter().filter(|s|s.active||s.id==form().supplier_id){option{value:s.id.to_string(),"{s.name}"}}}}
            label{"Order date" input{r#type:"date",value:form().order_date,oninput:move |e|form.write().order_date=e.value()}}
            label{"Discount amount" input{r#type:"number",min:"0",step:"0.01",value:form().discount,oninput:move |e|form.write().discount=e.value()}}
            label{"Tax %" input{r#type:"number",min:"0",max:"100",step:"0.01",value:form().tax,oninput:move |e|form.write().tax=e.value()}}
        }
        div{class:"purchase_add",
            label{"Product / Variant" select{value:choice(),onchange:move |e|choice.set(e.value()),option{value:"","Select item"}for (i,p) in products.iter().enumerate(){option{value:i.to_string(),"{p.label}"}}}}
            button{disabled:choice().parse::<usize>().ok().and_then(|i|products.get(i)).is_none(),onclick:{let products=products.clone();let locations=locations.clone();move |_|{
                if let Some(p)=choice().parse::<usize>().ok().and_then(|i|products.get(i)){form.write().lines.push(Line{product_id:p.product_id,variant_id:p.variant_id,quantity:1,unit_price:p.cost.parse::<f64>().map(|v|format!("{v:.2}")).unwrap_or_default(),location:locations.first().cloned().unwrap_or_default(),batch:String::new(),expiry:String::new()});choice.set(String::new());}
            }},ActionLabel{label:"Add item"}}
        }
        div{class:"purchase_lines",for (i,line) in form().lines.iter().enumerate(){div{class:"purchase_line",key:"{i}",
            strong{class:"purchase_line_name",{products.iter().find(|p|p.product_id==line.product_id&&p.variant_id==line.variant_id).map(|p|p.label.clone()).unwrap_or_else(||format!("Product #{} (unavailable)",line.product_id))}}
            label{"Quantity (base units)" input{r#type:"number",min:"1",max:"1000000",step:"1",value:line.quantity.to_string(),oninput:move |e|form.write().lines[i].quantity=e.value().parse().unwrap_or(0)}}
            label{"Unit price" input{r#type:"number",min:"0",step:"0.01",value:line.unit_price.clone(),oninput:move |e|form.write().lines[i].unit_price=e.value()}}
            label{"Location" select{value:line.location.clone(),onchange:move |e|form.write().lines[i].location=e.value(),option{value:"","Select location"}for place in &locations{option{value:place.clone(),"{place}"}}}}
            label{"Batch" input{value:line.batch.clone(),oninput:move |e|form.write().lines[i].batch=e.value()}}
            label{"Expiry" input{r#type:"date",value:line.expiry.clone(),oninput:move |e|form.write().lines[i].expiry=e.value()}}
            button{title:"Remove item",aria_label:"Remove item",onclick:move |_|{form.write().lines.remove(i);},Icon{name:"delete"}}
        }}}
        label{"Notes" textarea{value:form().notes,oninput:move |e|form.write().notes=e.value()}}
        strong{"Total: {total}"}
        if !error().is_empty(){p{role:"alert","{error}"}}
        footer{class:"customers_actions",button{onclick:move |_|on_close.call(()),ActionLabel{label:"Cancel"}}button{class:"customer_primary",onclick:move |_|{let draft=form();match purchases::totals(&draft){Ok(_)=>on_action.call(Action::Save{id,revision,draft}),Err(e)=>error.set(e.to_string())}},ActionLabel{label:"Save order"}}}
    }}}
}

#[component]
fn OperationDialog(
    action: Action,
    on_close: EventHandler<()>,
    on_action: EventHandler<Action>,
) -> Element {
    let mut value = use_signal(|| action);
    let mut error = use_signal(String::new);
    rsx! {div{class:"modal_backdrop",section{class:"customer_dialog purchase_operation",role:"dialog",aria_modal:"true",aria_label:"Confirm purchase operation",
        match value(){
            Action::Receive{..}=>rsx!{h2{"Receive entire order"}label{"Receipt date" input{r#type:"date",value:if let Action::Receive{date,..}=value(){date}else{String::new()},oninput:move |e|if let Action::Receive{date,..}=&mut *value.write(){*date=e.value();}}}},
            Action::Cancel{..}=>rsx!{h2{"Cancel purchase order"}label{"Reason" textarea{value:if let Action::Cancel{reason,..}=value(){reason}else{String::new()},oninput:move |e|if let Action::Cancel{reason,..}=&mut *value.write(){*reason=e.value();}}}},
            Action::Pay(p)=>rsx!{h2{"Supplier payment"}p{{p.po_id.map(|id|format!("Purchase #{id}")).unwrap_or_else(||"Unallocated payment / advance".into())}}
                div{class:"purchase_form",for (key,label,kind,text) in [("amount","Amount","number",p.amount),("date","Payment date","date",p.date),("reference","Reference","text",p.reference)]{label{"{label}" input{r#type:kind,step:"0.01",value:text,oninput:move |e|if let Action::Pay(p)=&mut *value.write(){match key{"amount"=>p.amount=e.value(),"date"=>p.date=e.value(),_=>p.reference=e.value()}}}}}
                label{"Method" select{value:p.method,onchange:move |e|if let Action::Pay(p)=&mut *value.write(){p.method=e.value();},for method in ["Cash","Bank Transfer","Mobile Payment"]{option{value:method,"{method}"}}}}}
                label{"Notes" textarea{value:p.notes,oninput:move |e|if let Action::Pay(p)=&mut *value.write(){p.notes=e.value();}}}
            },
            _=>rsx!{}
        }
        if !error().is_empty(){p{role:"alert","{error}"}}
        footer{class:"customers_actions",button{onclick:move |_|on_close.call(()),ActionLabel{label:"Cancel"}}button{class:"customer_primary",onclick:move |_|{
            let action=value();let valid=match &action{Action::Pay(p)=>purchases::money(&p.amount).map(|n|!n.is_zero()&&chrono::NaiveDate::parse_from_str(&p.date,"%Y-%m-%d").is_ok()),Action::Cancel{reason,..}=>Ok(!reason.trim().is_empty()),Action::Receive{date,..}=>Ok(chrono::NaiveDate::parse_from_str(date,"%Y-%m-%d").is_ok()),_=>Ok(false)};
            if matches!(valid,Ok(true)){on_action.call(action);}else{error.set("Enter valid payment, date or reason".into());}
        },ActionLabel{label:"Confirm"}}}
    }}}
}

#[component]
fn OrderDetails(db_form: DbForm, actor: Session, id: i32, on_close: EventHandler<()>) -> Element {
    let rows = use_resource(move || {
        let source = db_form.clone();
        let actor = actor.clone();
        async move {
            async {
                let pool = pos_core::connect(&source.database_config()?).await?;
                purchases::order_items(&pool, &actor, id).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    rsx! {div{class:"modal_backdrop",section{class:"customer_dialog purchase_dialog",role:"dialog",aria_modal:"true",aria_label:"Purchase details",header{class:"reports_header",h2{"Purchase #{id}"}button{onclick:move |_|on_close.call(()),ActionLabel{label:"Close"}}}
        if let Some(Ok(items))=rows.read().as_ref(){div{class:"reports_table_scroll",table{class:"customers_table",thead{tr{for h in ["Product","Variant","Quantity","Unit price","Line total","Location","Batch","Expiry"]{th{"{h}"}}}}tbody{for item in items{tr{td{"{item.name}"}td{"{item.variant}"}td{"{item.quantity}"}td{"{amount(&item.unit_price)}"}td{"{amount(&item.total)}"}td{"{item.location}"}td{"{item.batch}"}td{"{item.expiry}"}}}}}}}else if let Some(Err(e))=rows.read().as_ref(){p{role:"alert","{e}"}}else{p{"Loading..."}}
    }}}
}
#[component]
fn Ledger(db_form: DbForm, actor: Session, supplier_id: i32, from: String, to: String) -> Element {
    let rows = use_resource(move || {
        let source = db_form.clone();
        let actor = actor.clone();
        let a = from.clone();
        let b = to.clone();
        async move {
            async {
                let pool = pos_core::connect(&source.database_config()?).await?;
                purchases::ledger(&pool, &actor, supplier_id, &a, &b).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    rsx! {if let Some(Ok(entries))=rows.read().as_ref(){div{class:"reports_table_scroll",table{class:"customers_table",thead{tr{for h in ["Date","Reference","Order","Type","Amount","Notes"]{th{"{h}"}}}}tbody{for row in entries{tr{td{"{row.date}"}td{"{row.reference}"}td{"{row.order_number}"}td{"{row.kind}"}td{"{amount(&row.amount)}"}td{"{row.notes}"}}}}}}if entries.is_empty(){p{"No ledger entries in this period."}}}else if let Some(Err(e))=rows.read().as_ref(){p{role:"alert","{e}"}}else{p{"Loading ledger..."}}}
}
