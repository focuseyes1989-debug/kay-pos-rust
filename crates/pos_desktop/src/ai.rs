use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{
    ai::{self, Request, Scope},
    auth::Session,
};

#[component]
pub fn AiPage(db_form: DbForm, actor: Session) -> Element {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut from = use_signal(|| today.clone());
    let mut to = use_signal(|| today.clone());
    let mut tab = use_signal(|| 0usize);
    let mut question = use_signal(String::new);
    let mut request = use_signal(|| None::<Request>);
    let mut result = use_signal(|| None::<Result<ai::Answer, String>>);
    let mut busy = use_signal(|| false);
    let mut revision = use_signal(|| 0usize);
    let mut run = {
        let db = db_form.clone();
        let actor = actor.clone();
        move |next: Request| {
            revision += 1;
            let run_revision = revision();
            request.set(Some(next.clone()));
            result.set(None);
            busy.set(true);
            let db = db.clone();
            let actor = actor.clone();
            spawn(async move {
                let answer = async {
                    let pool = pos_core::connect(&db.database_config()?).await?;
                    ai::ask(&pool, &actor, &next).await
                }
                .await
                .map_err(|e: anyhow::Error| format!("အချက်အလက်ရယူရာတွင် အမှားဖြစ်နေပါသည် - {e:#}"));
                if revision() == run_revision {
                    result.set(Some(answer));
                    busy.set(false);
                }
            });
        }
    };
    let mut rerun = run.clone();
    rsx! {section {class:"customers_page phase6_page ai_page",
        header {class:"customers_header", h2 {crate::icons::ActionLabel {label:"AI"}}
            button {hidden:true,"data-page-refresh":"true",disabled:busy() || request().is_none(),onclick:move |_|if let Some(current)=request(){rerun(current)},"Refresh"}
        }
        div {class:"receipt_tabs reports_tabs ai_tabs",role:"tablist",aria_label:"AI views",
            for (index, name) in ["AI Chat", "Analytics", "Dashboard Assistant", "Product Assistant", "Summary / Digest"].iter().enumerate() {
                button {role:"tab",aria_selected:tab()==index,class:if tab()==index {"active"}else{""},onclick:move |_|{revision+=1;busy.set(false);tab.set(index);question.set(String::new());request.set(None);result.set(None);},"{name}"}
            }
        }
        div {class:"phase6_filters",
            label {"From" input {r#type:"date",value:"{from}",oninput:move |e|from.set(e.value())}}
            label {"To" input {r#type:"date",value:"{to}",oninput:move |e|to.set(e.value())}}
            if tab()==0 || tab()==3 {
                label {if tab()==0 {"Request"}else{"Product"}
                    input {value:"{question}",placeholder:if tab()==0 {"sales trend / dashboard / digest / product name"}else{"Name, SKU or barcode"},oninput:move |e|question.set(e.value())}
                }
            }
            button {class:"customer_primary",disabled:busy(),onclick:move |_|{
                let r=if tab()==0 {ai::chat(&question(),&from(),&to()).map_err(|e|e.to_string())}else{
                    Ok(Request {scope:match tab(){1=>Scope::Analytics,2=>Scope::Dashboard,3=>Scope::Products,_=>Scope::Digest},from:from(),to:to(),product:question()})
                };
                match r {Ok(next)=>run(next),Err(error)=>{request.set(None);result.set(Some(Err(error)));}}
            },if tab()==0 {"Send"}else{"Generate"}}
        }
        if busy() {p {role:"status",class:"ai_loading","အချက်အလက်များ ရယူနေပါသည်..."}}
        else if let Some(response)=result() {
            match response {
                Err(e)=>rsx! {p {role:"alert","{e}"}},
                Ok(answer)=>rsx! {
                    h3 {"{answer.title}"}
                    p {"{answer.from} / {answer.to} · {answer.generated_at}"}
                    div {class:"customers_table_scroll",table {class:"customers_table",
                        thead {tr {for heading in &answer.headers {th {"{heading}"}}}}
                        tbody {for row in &answer.rows {tr {for cell in row {td {"{cell}"}}}}}
                    }}
                    if answer.rows.is_empty() {p {"ကိုက်ညီသည့်မှတ်တမ်း မရှိပါ။"}}
                    p {"ဒေတာရင်းမြစ် - {answer.sources}"}
                    for note in &answer.notes {p {"{note}"}}
                },
            }
        } else {
            section {class:"ai_welcome panel",
                strong {if tab()==0 {"မေးလိုသည့်အကြောင်းအရာကို ရိုက်ထည့်ပါ"}else{"ရက်အပိုင်းအခြားရွေးပြီး အဖြေထုတ်ပါ"}}
                small {if tab()==0 {"ဥပမာ - ဒီနေ့ အရောင်း၊ dashboard၊ အနှစ်ချုပ် သို့မဟုတ် product <အမည် / SKU / barcode>"}else{"အဖြေများကို သတ်မှတ်ထားသော PostgreSQL database မှ တိုက်ရိုက်ရယူပါသည်။"}}
            }
        }
    }}
}
