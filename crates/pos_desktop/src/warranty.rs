use crate::{format_qty, DbForm};
use dioxus::prelude::*;
use pos_core::{auth::Session, warranty as api};

#[component]
pub fn WarrantyPage(db_form: DbForm, actor: Session) -> Element {
    let mut query = use_signal(String::new);
    let applied = crate::live_search::use_debounced_search(query);
    let mut invoice = use_signal(String::new);
    let mut loaded_invoice = use_signal(String::new);
    let mut item = use_signal(|| 0i32);
    let mut serial = use_signal(String::new);
    let mut months = use_signal(|| "12".to_string());
    let mut selected = use_signal(String::new);
    let mut selected_claim = use_signal(String::new);
    let mut issue = use_signal(String::new);
    let mut note = use_signal(String::new);
    let mut next_status = use_signal(|| "in_progress".to_string());
    let mut notice = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut revision = use_signal(|| 0u64);
    let source = db_form.clone();
    let reader = actor.clone();
    let data = use_resource(move || {
        let form = source.clone();
        let actor = reader.clone();
        let q = applied();
        let inv = loaded_invoice();
        let unit = selected();
        let claim = selected_claim();
        let _ = revision();
        async move {
            async {
                let pool = pos_core::connect(&form.database_config()?).await?;
                let units = api::search(&pool, &actor, &q).await?;
                let lines = if inv.is_empty() {
                    Vec::new()
                } else {
                    api::receipt_lines(&pool, &actor, &inv).await?
                };
                let claims = if unit.is_empty() {
                    Vec::new()
                } else {
                    api::claims(&pool, &actor, &unit).await?
                };
                let events = if claim.is_empty() {
                    Vec::new()
                } else {
                    api::events(&pool, &actor, &claim).await?
                };
                Ok::<_, anyhow::Error>((units, lines, claims, events))
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let register_form = db_form.clone();
    let register_actor = actor.clone();
    let claim_form = db_form.clone();
    let claim_actor = actor.clone();
    let document_form=db_form.clone();
    let document_actor=actor.clone();
    let update_form = db_form.clone();
    let update_actor = actor.clone();
    rsx! {
        style { {include_str!("../assets/warranty.css")} }
        section {class:"customers_page warranty_page",
            header {class:"warranty_heading",h2 {"Serial Numbers & Warranty"} p {"Register serial numbers against a completed receipt with a customer. Warranty starts on the sale date."}}
            div {class:"warranty_layout",
                section {class:"warranty_card settings_organized",
                    h3 {"Register sold unit"}
                    div {class:"warranty_receipt_search",input {aria_label:"Receipt number",placeholder:"Receipt number e.g. INV000001",value:"{invoice}",oninput:move |e|invoice.set(e.value())}
                        button {disabled:busy()||invoice().trim().is_empty(),onclick:move |_|{item.set(0);loaded_invoice.set(invoice());notice.set(String::new());},"Find receipt"}}
                    if let Some(Ok((_,lines,_,_)))=data.read().as_ref() {
                        if !loaded_invoice().is_empty() && lines.is_empty(){p {"No eligible receipt found. Use a completed receipt with a customer."}}
                        for line in lines {
                            button {class:if item()==line.id {"warranty_line selected"}else{"warranty_line"},onclick:{let id=line.id;move |_|item.set(id)},
                                strong {"{line.product}"} small {"{line.customer} · {line.registered} / {format_qty(line.quantity)} units registered"}}
                        }
                    }
                    label {"Serial number" input {value:"{serial}",maxlength:120,oninput:move |e|serial.set(e.value())}}
                    label {"Warranty (months)" input {r#type:"number",min:1,max:120,value:"{months}",oninput:move |e|months.set(e.value())}}
                    button {class:"settings_save",disabled:busy()||item()==0||serial().trim().is_empty(),onclick:move |_|{
                        let form=register_form.clone();let actor=register_actor.clone();let id=item();let number=serial();let term=months();busy.set(true);
                        spawn(async move {let result=async {let pool=pos_core::connect(&form.database_config()?).await?;api::register(&pool,&actor,id,&number,term.parse()?).await}.await;
                            match result {Ok(id)=>{selected.set(id);serial.set(String::new());notice.set("Serial registered. Warranty linked to the original receipt and customer.".into());revision+=1;},Err(e)=>notice.set(format!("{e:#}"))} busy.set(false);});
                    },"Register serial"}
                }
                section {class:"warranty_card",
                    h3 {"Serial & warranty lookup"}
                    input {r#type:"search",aria_label:"Search serials",placeholder:"Serial, receipt, customer or product",value:"{query}",oninput:move |e|{query.set(e.value());selected.set(String::new());selected_claim.set(String::new());}}
                    if !data.finished(){p {role:"status","Loading…"}}
                    if let Some(Err(e))=data.read().as_ref(){p {role:"alert","{e}"}}
                    if let Some(Ok((units,_,_,_)))=data.read().as_ref(){
                        small {"Showing up to 200 units. Search to narrow results."}
                        div {class:"warranty_units",
                            for unit in units {
                                button {class:if selected()==unit.id {"warranty_unit selected"}else{"warranty_unit"},onclick:{let id=unit.id.clone();move |_|{selected.set(id.clone());selected_claim.set(String::new());issue.set(String::new());note.set(String::new());}},
                                    strong {"{unit.serial}"} span {"{unit.product}"} small {"{unit.invoice} · {unit.customer}"}
                                    small {"{unit.starts} → {unit.ends} · " if api::coverage(unit,chrono::Local::now().date_naive()) {"Covered"}else if unit.sale_status!="completed" {"Receipt refunded / unavailable"}else{"Outside warranty"}}
                                }
                            }
                            if units.is_empty(){p {"No registered serial numbers found."}}
                        }
                    }
                }
            }
            if !selected().is_empty(){
                section {class:"warranty_card settings_organized",
                    div {class:"warranty_document_actions",
                        strong {"A4 Warranty Form"}
                        for (pdf,label) in [(true,"Export PDF"),(false,"Print A4")] {
                            button {disabled:busy(),onclick:{let form=document_form.clone();let actor=document_actor.clone();move |_|{
                                let form=form.clone();let actor=actor.clone();let unit=selected();busy.set(true);
                                spawn(async move {let result=crate::warranty_form::output(form,actor,unit,pdf).await;notice.set(match result {Ok(s)=>s,Err(e)=>format!("{e:#}")});busy.set(false);});
                            }},"{label}"}
                        }
                    }
                    h3 {"Warranty claims"}
                    p {"Claims outside the warranty period or on refunded receipts are recorded as rejected. Open claims do not change stock, payments or sales."}
                    label {"Issue / fault description" textarea {value:"{issue}",maxlength:2000,oninput:move |e|issue.set(e.value())}}
                    button {disabled:busy()||issue().trim().is_empty(),onclick:move |_|{
                        let form=claim_form.clone();let actor=claim_actor.clone();let unit=selected();let text=issue();busy.set(true);
                        spawn(async move {let result=async {let pool=pos_core::connect(&form.database_config()?).await?;api::open_claim(&pool,&actor,&unit,&text).await}.await;
                            match result {Ok(id)=>{selected_claim.set(id);issue.set(String::new());notice.set("Claim recorded. Check coverage and status below.".into());revision+=1;},Err(e)=>notice.set(format!("{e:#}"))}busy.set(false);});
                    },"Record claim"}
                    if let Some(Ok((_,_,claims,events)))=data.read().as_ref(){
                        for claim in claims {
                            button {class:if selected_claim()==claim.id {"warranty_claim selected"}else{"warranty_claim"},onclick:{let id=claim.id.clone();move |_|{selected_claim.set(id.clone());note.set(String::new());}},
                                strong {"{claim.status}"} span {"{claim.issue}"} small {"{claim.created_at} · Coverage at receipt: " if claim.covered==1 {"Covered"}else{"Not covered"}}
                            }
                        }
                        if claims.is_empty(){p {"No claims for this serial."}}
                        if !selected_claim().is_empty(){
                            h3 {"Claim history"}
                            for event in events {div {class:"warranty_event",strong {"{event.status} · {event.actor}"} small {"{event.created_at}"} p {"{event.note}"}}}
                            if actor.allows(pos_core::auth::Permission::Manage) && claims.iter().any(|c|c.id==selected_claim() && matches!(c.status.as_str(),"open"|"in_progress")) {
                                label {"Next status" select {value:"{next_status}",onchange:move |e|next_status.set(e.value()),option {value:"in_progress","In progress"} option {value:"resolved","Resolved"} option {value:"rejected","Rejected"}}}
                                label {"Service / resolution note" textarea {value:"{note}",maxlength:2000,oninput:move |e|note.set(e.value())}}
                                button {class:"settings_save",disabled:busy()||note().trim().is_empty(),onclick:move |_|{
                                    let form=update_form.clone();let actor=update_actor.clone();let id=selected_claim();let state=next_status();let text=note();busy.set(true);
                                    spawn(async move {let result=async {let pool=pos_core::connect(&form.database_config()?).await?;api::update_claim(&pool,&actor,&id,&state,&text).await}.await;
                                        match result {Ok(())=>{note.set(String::new());notice.set("Claim status saved.".into());revision+=1;},Err(e)=>notice.set(format!("{e:#}"))}busy.set(false);});
                                },"Save claim status"}
                            }
                        }
                    }
                }
            }
            if !notice().is_empty(){p {role:"status","{notice}"}}
        }
    }
}
