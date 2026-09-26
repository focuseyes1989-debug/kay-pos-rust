use crate::{icons::ActionLabel, DbForm};
use dioxus::prelude::*;
use pos_core::auth::Session;
#[component]
pub fn History(
    db_form: DbForm,
    actor: Session,
    customer_id: i32,
    name: String,
    onclose: EventHandler<()>,
) -> Element {
    let mut data = use_resource(move || {
        let db = db_form.clone();
        let actor = actor.clone();
        async move {
            async {
                pos_core::loyalty::history(
                    &pos_core::connect(&db.database_config()?).await?,
                    &actor,
                    customer_id,
                )
                .await
            }
            .await
            .map_err(|e: anyhow::Error| format!("{e:#}"))
        }
    });
    let mut page = use_signal(|| 0usize);
    let mut query = use_signal(String::new);
    rsx! {div{class:"modal_backdrop",section{class:"customer_dialog loyalty_dialog",role:"dialog",aria_modal:"true",aria_label:"Customer points history",
        header{class:"customers_header",h2{"{name} / Points history"}button{onclick:move |_|onclose.call(()),ActionLabel{label:"Close"}}}
        label{"Search" input{r#type:"search",value:"{query}",placeholder:"Type or receipt",oninput:move|e|{query.set(e.value());page.set(0);}}}
        match data.read().as_ref(){None=>rsx!{p{"Loading..."}},Some(Err(e))=>rsx!{p{role:"alert","{e}"}button{onclick:move |_|data.restart(),"Retry"}},Some(Ok((balance,all)))=>{
            let q=query().to_lowercase();let rows=all.iter().filter(|r|format!("{} {}",r.kind,r.reference).to_lowercase().contains(&q)).collect::<Vec<_>>();let pages=rows.len().div_ceil(25).max(1);let current=page().min(pages-1);
            rsx!{p{strong{"Current points: {balance}"}}
                div{class:"loyalty_table",table{thead{tr{th{"Date"}th{"Type"}th{"Points"}th{"Reference"}th{"Expiry"}}}tbody{
                    for r in rows.iter().skip(current*25).take(25){tr{key:"{r.id}",td{"{r.date}"}td{"{r.kind}"}td{"{r.points}"}td{"{r.reference}"}td{"{r.expiry}"}}}
                }}}
                if rows.is_empty(){p{"No points history."}}
                footer{class:"customers_pagination",button{disabled:current==0,onclick:move |_|page.set(current.saturating_sub(1)),"Previous"}span{"Page {current+1} of {pages}"}button{disabled:current+1>=pages,onclick:move |_|page.set(current+1),"Next"}}
            }
        }}
    }}}
}
