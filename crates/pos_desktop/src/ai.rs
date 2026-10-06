use crate::{DbForm, WorkspaceView};
use dioxus::prelude::*;
use pos_core::{
    ai::{self, Request, Scope},
    auth::Session,
};

const AI_CSS: &str = include_str!("../assets/ai.css");
const CHAT_TOPICS: &[(&str,&str,&str)] = &[
    ("sales","အရောင်း","ဒီနေ့ အရောင်း ပြပါ"),
    ("category","Category ရောင်းရငွေ","ဒီနေ့ Service ရောင်းရငွေ ပြပါ"),
    ("receipts","Receipt များ","ဒီနေ့ receipt ပြပါ"),
    ("category-receipts","Category ပါသည့် Receipt","ဒီနေ့ ရေခဲမုန့် (category) ရောင်းထားတဲ့ Receipt ပြပေးပါ"),
    ("amount","ငွေပမာဏအလိုက် Receipt","ဒီနေ့ 10000 ကျပ် ရောင်းရတဲ့ receipt ပြပါ"),
    ("products","ကုန်ပစ္စည်း","products"),
    ("inventory","ကုန်လက်ကျန်","inventory"),
    ("low-stock","လက်ကျန်နည်းသည့် ပစ္စည်း","low stock"),
    ("expenses","အသုံးစရိတ်","ဒီလ အသုံးစရိတ် ပြပါ"),
    ("customers","ဖောက်သည်","customers"),
    ("credit","ဖောက်သည်အကြွေး","debt"),
    ("employees","ဝန်ထမ်း","employees"),
    ("attendance","ဝန်ထမ်းတက်ရောက်မှု","attendance today"),
    ("summary","လုပ်ငန်းအနှစ်ချုပ်","digest"),
    ("help","အသုံးပြုနည်း","help"),
];
fn slash_topics(text:&str)->Vec<(&'static str,&'static str,&'static str)> {
    let Some(search)=text.trim_start().strip_prefix('/') else{return vec![]};
    let search=search.trim().to_lowercase();
    CHAT_TOPICS.iter().copied().filter(|(key,label,_)|key.contains(&search)||label.contains(&search)).collect()
}

#[derive(Clone, PartialEq)]
struct ChatEntry {
    plan: pos_core::assistant::Plan,
    choices: Vec<String>,
    prompt: String,
    scope: Scope,
    subject: String,
    receipt_category: String,
    receipt_payment: String,
    model_text: Option<String>,
    response: Option<Result<ai::Answer, String>>,
}

#[component]
pub fn AiPage(db_form: DbForm, actor: Session, on_navigate: EventHandler<WorkspaceView>) -> Element {
    let mut question = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut history = use_signal(Vec::<ChatEntry>::new);
    let mut busy = use_signal(|| false);
    let mut revision = use_signal(|| 0usize);
    let mut topic_index=use_signal(||0usize);
    let mut topics_dismissed=use_signal(||false);
    let topics=slash_topics(&question());
    let topics_open=!busy() && !topics_dismissed() && question().trim_start().starts_with('/');
    let selected_topic=topic_index().min(topics.len().saturating_sub(1));
    use_effect(move || {
        let _=topic_index();let _=question();
        document::eval("requestAnimationFrame(() => {const list=document.getElementById('ai-slash-topics');const item=list?.querySelector('[aria-selected=true]');if(list&&item){const a=list.getBoundingClientRect(),b=item.getBoundingClientRect();if(b.top<a.top)list.scrollTop+=b.top-a.top;else if(b.bottom>a.bottom)list.scrollTop+=b.bottom-a.bottom;}});");
    });
    use_effect(move || {
        let _ = history.read().len();
        let _ = busy();
        document::eval("requestAnimationFrame(() => {const body=document.querySelector('.ai_chat_body');if(body)body.scrollTop=body.scrollHeight;})");
    });
    let mut run = {
        let db = db_form.clone();
        let actor = actor.clone();
        move |plan: pos_core::assistant::Plan| {
            if busy() {
                return;
            }
            let prompt = question();
            let next=plan.request.clone();
            question.set(String::new());
                let mut entries = history.write();
                if entries.len() >= 20 {
                    entries.remove(0);
                }
                entries.push(ChatEntry {
                    plan:plan.clone(),
                    choices:vec![],
                    prompt: prompt.clone(),
                    scope: next.scope,
                    subject: next.product.clone(),
                    receipt_category: next.receipt_category.clone(),
                    receipt_payment: next.receipt_payment.clone(),
                    model_text: None,
                    response: None,
                });
                drop(entries);

            notice.set(String::new());
            revision += 1;
            let run_revision = revision();
            busy.set(true);
            let db = db.clone();
            let actor = actor.clone();
            spawn(async move {
                let response = async {
                    let pool = pos_core::connect(&db.database_config()?).await?;
                    let choices=pos_core::assistant::candidates(&pool,&actor,&next).await?;
                    if !choices.is_empty(){return Ok::<_,anyhow::Error>((None,choices));}
                    Ok((Some(pos_core::assistant::answer(&pool,&actor,&plan).await?),vec![]))
                }
                .await
                .map_err(|e: anyhow::Error| format!("အချက်အလက်ရယူရာတွင် အမှားဖြစ်နေပါသည် - {e:#}"));
                if revision() == run_revision {
                    let answer = response
                        .as_ref()
                        .map(|(answer, _)| answer.clone())
                        .map_err(|e| e.clone());

                        if let Some(entry) = history.write().last_mut() {
                            match answer {
                                Ok(Some(value))=>entry.response=Some(Ok(value)),
                                Ok(None)=>{entry.choices=response.as_ref().unwrap().1.clone();},
                                Err(error)=>entry.response=Some(Err(error)),
                            }
                        }

                    busy.set(false);
                }
            });
        }
    };
    let choice_runner=run.clone();
    let mut submit = move || {
        if busy() {return;}
        if question().trim_start().starts_with('/') {notice.set("စာရင်းထဲမှ အကြောင်းအရာတစ်ခု ရွေးပြီး မေးခွန်းကို ပြင်ရေးပါ။".into());return;}
        if question().trim().is_empty() {notice.set("မေးခွန်းကို ရိုက်ထည့်ပါ".into());return;}
        let previous=history().into_iter().rev().find(|entry|matches!(entry.response,Some(Ok(_)))).map(|entry|entry.plan.request);
        match pos_core::assistant::plan(&question(),previous.as_ref(),chrono::Local::now().date_naive()) {Ok(next)=>run(next),Err(error)=>notice.set(error.to_string())}
    };
    let mut submit_enter = submit.clone();
    rsx! {section {class:"customers_page phase6_page ai_page ai_workspace ai_codex",
        style { "{AI_CSS}" }
        if !notice().is_empty() {p {class:"ai_error",role:"alert","{notice}"}}
        div {class:"phase6_filters ai_composer",
            if topics_open {
                div {class:"ai_slash_menu",
                    div {class:"ai_slash_heading",strong {"ဘာအကြောင်း မေးချင်ပါသလဲ?"} small {"↑ ↓ ရွေးရန် · Enter ထည့်ရန် · Esc ပိတ်ရန်"}}
                    div {id:"ai-slash-topics",role:"listbox",aria_label:"မေးနိုင်သော အကြောင်းအရာများ",
                        for (index,(key,label,prompt)) in topics.iter().copied().enumerate() {
                            button {id:format!("ai-topic-{index}"),r#type:"button",role:"option",aria_selected:index==selected_topic,
                                class:if index==selected_topic{"ai_slash_option selected"}else{"ai_slash_option"},
                                onclick:move |_|{question.set(prompt.into());topics_dismissed.set(true);notice.set(String::new());document::eval("document.querySelector('.ai_prompt_control textarea')?.focus();");},
                                span {"{label}"} small {"/{key}"}
                            }
                        }
                        if topics.is_empty(){p {"ကိုက်ညီသည့်အကြောင်းအရာ မရှိပါ။ / တစ်ခုတည်း ရိုက်ပြီး ပြန်ရွေးပါ။"}}
                    }
                }
            }
            label {class:"ai_prompt_control",span {class:"ai_sr_only","Message"}
                textarea {aria_label:"Message KAY Assistant",aria_expanded:topics_open,aria_controls:"ai-slash-topics",aria_activedescendant:if topics_open&&!topics.is_empty(){format!("ai-topic-{selected_topic}")}else{String::new()},rows:2,maxlength:1000,disabled:busy(),value:"{question}",placeholder:"မေးခွန်း ရိုက်ပါ၊ သို့မဟုတ် / နှိပ်ပြီး အကြောင်းအရာ ရွေးပါ",oninput:move |e|{question.set(e.value());topic_index.set(0);topics_dismissed.set(false);},
                    onkeydown:move |event|{
                        if topics_open {
                            match event.key() {
                                Key::Escape=>{event.prevent_default();topics_dismissed.set(true);return;},
                                Key::ArrowDown=>{event.prevent_default();topic_index.set((selected_topic+1).min(topics.len().saturating_sub(1)));return;},
                                Key::ArrowUp=>{event.prevent_default();topic_index.set(selected_topic.saturating_sub(1));return;},
                                Key::Enter=>{event.prevent_default();if let Some((_,_,prompt))=topics.get(selected_topic){question.set((*prompt).into());topics_dismissed.set(true);notice.set(String::new());}return;},
                                _=>{},
                            }
                        }
                        if event.key()==Key::Enter && !event.modifiers().contains(Modifiers::SHIFT) {event.prevent_default();submit_enter();}
                    }
                }
            }
            div {class:"ai_composer_toolbar",
                button {class:"ai_topic_toggle",r#type:"button",aria_label:"Choose question topic",title:"မေးမည့်အကြောင်းအရာ ရွေးပါ",disabled:busy(),onclick:move |_|{question.set("/".into());topic_index.set(0);topics_dismissed.set(false);document::eval("document.querySelector('.ai_prompt_control textarea')?.focus();");},crate::icons::Icon {name:"add"}}
                button {class:"ai_new_chat",r#type:"button",disabled:busy(),onclick:move |_|{history.clear();question.set(String::new());notice.set(String::new());topics_dismissed.set(true);document::eval("document.querySelector('.ai_prompt_control textarea')?.focus();");},"New chat"}
                button {class:"ai_send",aria_label:"Send message",title:"Send · Enter",disabled:busy()||question().trim().is_empty()||question().trim_start().starts_with('/'),onclick:move |_|submit(),if busy() {span {class:"loading_spinner"}}else{svg {view_box:"0 0 24 24",width:"20",height:"20",fill:"none","aria-hidden":"true",path {d:"M12 19V5M5 12l7-7 7 7",stroke:"currentColor",stroke_width:"2",stroke_linecap:"round",stroke_linejoin:"round"}}}}
            }
        }
        div {class:"ai_chat_body",
        if !history().is_empty() {
            div { class:"ai_conversation", aria_label:"Conversation",
                for (index,entry) in history().iter().enumerate() {
                    {let followup_entry=entry.clone();
                    rsx! {article { key:"{index}", class:"ai_turn",
                        div { class:"ai_user_message", small {"You"} p {"{entry.prompt}"} }
                        div { class:"ai_assistant_message",
                            strong {"KAY Assistant"}
                            if let Some(text)=&entry.model_text {p {class:"ai_model_text","{text}"}}
                            if !entry.choices.is_empty() {
                                p {"အမည်အတိအကျ မသေချာပါ။ ဘယ်အမည်ကို ဆိုလိုတာလဲ ရွေးပေးပါ။"}
                                small {"အနီးစပ်ဆုံး အမည်များ (ပထမအမည် 1,000 အတွင်းမှ ရှာထားသည်)"}
                                div {class:"ai_followups",for name in &entry.choices {
                                    {let choice=name.clone();let plan=entry.plan.clone();let mut choose_run=choice_runner.clone();rsx!{button {disabled:busy(),onclick:move |_|{question.set(choice.clone());choose_run(pos_core::assistant::choose(plan.clone(),&choice));},"{name}"}}}
                                }}
                            } else {match &entry.response {
                                None=>rsx! {div {class:"ai_loading",role:"status",span {class:"loading_spinner"} "အချက်အလက်များ ရယူနေပါသည်…"}},
                                Some(Err(error))=>rsx! {p {class:"ai_error",role:"alert","{error}"}},
                                Some(Ok(answer))=>rsx! {
                                    if entry.model_text.is_some() {details {class:"ai_evidence",summary {"အသုံးပြုထားသော POS အချက်အလက်"} AnswerPanel {answer:answer.clone(),request:entry.plan.request.clone(),db_form:db_form.clone(),actor:actor.clone()}}}
                                    else {AnswerPanel {answer:answer.clone(),request:entry.plan.request.clone(),db_form:db_form.clone(),actor:actor.clone()}}
                                    if let Some(view)=scope_view(entry.scope) {
                                        button {class:"ai_text_action",onclick:move |_|on_navigate.call(view), "သက်ဆိုင်ရာ page ဖွင့်ရန် →"}
                                    }
                                    div {class:"ai_followups",
                                        for (label,prompt) in [("ဒီနေ့","today"),("မနေ့က","yesterday"),("ဒီလ","this month")] {
                                            if matches!(entry.scope,Scope::Analytics|Scope::CategorySales|Scope::Dashboard|Scope::Digest|Scope::Expenses|Scope::Receipts|Scope::Attendance) {
                                                {let entry=followup_entry.clone();rsx!{button {disabled:busy(),onclick:move |_|question.set(followup_prompt(&entry,prompt)),"{label}"}}}
                                            }
                                        }
                                    }
                                },
                            }}
                        }
                    }}}
                }
            }
        } else if busy() {
            div {class:"ai_loading",role:"status",span {class:"loading_spinner"} "အချက်အလက်များ ရယူနေပါသည်…"}
        } else {
            section {class:"ai_welcome panel",
                div {class:"ai_welcome_icon",crate::icons::Icon {name:"analytics"}}
                h3 {"ဒီနေ့ ဘာကို သိချင်ပါသလဲ?"}
                p {"ပစ္စည်းအမည်ကို တိုက်ရိုက်ရိုက်ပါ။ အရောင်း၊ ကုန်လက်ကျန်၊ အသုံးစရိတ်နဲ့ ရက်စွဲကို မေးခွန်းထဲမှာ ထည့်မေးနိုင်ပါတယ်။"}

                    div {class:"ai_suggestions",
                        for (label,prompt) in [("ဒီနေ့ အရောင်း","ဒီနေ့ အရောင်း"),("ဒီလ အရောင်း","ဒီလ အရောင်း"),("လုပ်ငန်းအခြေအနေ","dashboard"),("လုပ်ငန်းအနှစ်ချုပ်","digest"),("အသုံးစရိတ်","expenses this month"),("ဖောက်သည်အကြွေး","debt"),("ကုန်လက်ကျန်နည်း","low stock"),("ဝန်ထမ်းတက်ရောက်မှု","attendance today"),("အသုံးပြုနည်း","help"),("ပစ္စည်းစာရင်း","products"),("ကုန်လက်ကျန်","inventory")] {
                            button {r#type:"button",onclick:move |_|question.set(prompt.into()),"{label}"}
                        }
                    }

            }
        }

        }
    }}
}

fn followup_prompt(entry:&ChatEntry,period:&str)->String {
    if entry.scope==Scope::CategorySales {format!("category sales {} {period}",entry.subject)}
    else if entry.scope==Scope::Receipts {
        let category=if entry.receipt_category.is_empty(){String::new()}else{format!("{} (category)",entry.receipt_category)};
        let amount=entry.subject.strip_prefix("amount:").map(|value|format!("{value} ကျပ်")).unwrap_or_default();
        format!("{category} receipts {amount} {} {period}",entry.receipt_payment)
    } else if entry.scope==Scope::Expenses {format!("expenses {} {period}",entry.subject)}
    else {format!("{} {period}",ai::scope_prompt(entry.scope))}
}

fn scope_view(scope: Scope) -> Option<WorkspaceView> {
    Some(match scope {
        Scope::Analytics | Scope::CategorySales => WorkspaceView::SaleSummary,
        Scope::Expenses => WorkspaceView::Expenses,
        Scope::Customers | Scope::Credit => WorkspaceView::Customers,
        Scope::Products => WorkspaceView::Products,
        Scope::Inventory => WorkspaceView::Inventory,
        Scope::Employees | Scope::Attendance => WorkspaceView::Employees,
        Scope::Receipts => WorkspaceView::Receipts,
        Scope::Dashboard | Scope::Digest => WorkspaceView::Dashboard,
        _ => return None,
    })
}

#[component]
fn AnswerPanel(answer: ai::Answer, request:Request, db_form: DbForm, actor: Session) -> Element {
    let mut entity_detail=use_signal(||None::<ai::Answer>);
    let mut detail=use_signal(||None::<pos_core::ReceiptDetail>);
    let mut detail_error=use_signal(String::new);
    let mut detail_busy=use_signal(||false);
    let open_entity={let db=db_form.clone();let actor=actor.clone();let request=request.clone();move |row:Vec<String>| {
        if detail_busy(){return;}
        entity_detail.set(None);detail_error.set(String::new());detail_busy.set(true);
        let db=db.clone();let actor=actor.clone();let request=request.clone();
        spawn(async move {
            let result=async {let pool=pos_core::connect(&db.database_config()?).await?;pos_core::assistant::detail(&pool,&actor,&request,&row).await}.await;
            match result {Ok(value)=>entity_detail.set(Some(value)),Err(error)=>detail_error.set(format!("Detail မရပါ: {error:#}"))}
            detail_busy.set(false);
        });
    }};
    let open_receipt=move |sale_id:i32| {
        if detail_busy(){return;}
        detail.set(None);detail_error.set(String::new());detail_busy.set(true);
        let db=db_form.clone();let actor=actor.clone();
        spawn(async move {
            let result=async {let pool=pos_core::connect(&db.database_config()?).await?;ai::receipt_detail(&pool,&actor,sale_id).await}.await;
            match result {Ok(value)=>detail.set(Some(value)),Err(error)=>detail_error.set(format!("Receipt detail မရပါ: {error:#}"))}
            detail_busy.set(false);
        });
    };
    let export_answer = answer.clone();
    let mut export_notice = use_signal(String::new);
    let value_column = match answer.sources.as_str() {"comparison" | "sales, sale_items" | "sales, sale_items, products, categories" | "expenses" => Some(2), _ => None};
    let bars: Vec<(String,f64,String)> = value_column.map(|column| answer.rows.iter().take(12).filter_map(|row| {
        let text=row.get(column)?;
        let value=text.parse::<f64>().ok()?;
        if !value.is_finite() || value<0.0 {return None;}
        Some((row.first()?.clone(),value,text.clone()))
    }).collect()).unwrap_or_default();
    let max_value=bars.iter().map(|(_,value,_)|*value).fold(0.0f64,f64::max);
    rsx! { section {class:"ai_answer",
        header {class:"ai_answer_header", div {
            h3 {"{answer.title}"}
            if matches!(answer.sources.as_str(),"products"|"products, product_variants"|"customers"|"employees") {small {"လက်ရှိအချက်အလက်"}}
            else if answer.sources!="KAY POS Rust curated guidance" {small {"{answer.from} – {answer.to}"}}
        } }
        if !answer.narrative.is_empty() {p {class:"ai_response_intro","{answer.narrative}"}}
        if matches!(answer.sources.as_str(),"sales, sale_items"|"sales, sale_items, products, categories"|"expenses"|"comparison"|"sales") {
            details {class:"ai_calculation",summary {"တွက်ချက်ပုံနှင့် Refund သတ်မှတ်ချက်"}
                for note in answer.notes.iter().filter(|note|note.contains("refund")||note.contains("Refund")||note.contains("discount")||note.contains("ကွာခြားချက်")||note.contains("ရှာဖွေ")) {p {"{note}"}}
                if request.scope==Scope::Expenses {p {"အသုံးစရိတ်သည် ရွေးထားသောကာလအတွင်း မှတ်တမ်းတင်ထားသည့် amount စုစုပေါင်း ဖြစ်ပါသည်။"}}
            }
        }
        if answer.sources!="KAY POS Rust curated guidance" {
            div {class:"ai_answer_scope",small {"အသုံးပြုထားသော သတ်မှတ်ချက်များ"}
                p {"{answer.from} – {answer.to}"}
                if !request.product.is_empty(){p {"ရှာဖွေမှု: {request.product}"}}
                if !request.receipt_category.is_empty(){p {"Category: {request.receipt_category}"}}
                if !request.receipt_payment.is_empty(){p {"Payment: {request.receipt_payment}"}}
                if let Some(total)=&answer.total_amount {strong {"စုစုပေါင်း: {total}"}}
            }
        }
        if answer.sources=="products" && answer.rows.len()==1 {
            if let Some(row)=answer.rows.first() {
                p {class:"ai_response_intro",strong {"{row.first().cloned().unwrap_or_default()}"} " ကို ရှာတွေ့ပါတယ်။"}
                dl {class:"ai_product_facts",
                    for (heading,value) in answer.headers.iter().zip(row.iter()).skip(1) {div {dt {"{heading}"} dd {"{value}"}}}
                }
                {let row=row.clone();let mut open=open_entity.clone();rsx!{button {class:"ai_text_action",disabled:detail_busy(),onclick:move |_|open(row.clone()),"အသေးစိတ် ကြည့်ရန် →"}}}
            }
        } else if answer.sources=="KAY POS Rust curated guidance" {
            for row in &answer.rows {div {class:"ai_guidance_text",for (index,cell) in row.iter().enumerate(){if index==0 {strong {"{cell}"}}else{p {"{cell}"}}}}}
        } else {
            if answer.narrative.is_empty() && !answer.rows.is_empty() {p {class:"ai_response_intro","ကိုက်ညီသည့်မှတ်တမ်း {answer.rows.len()} ခု တွေ့ပါတယ်။"}}
        }
        if max_value>0.0 {
            div {class:"ai_chart",aria_label:"Result chart",
                strong {"{answer.headers[value_column.unwrap()]} · first 12 rows"}
                for (label,value,text) in &bars {
                    div {class:"ai_chart_row",span {"{label}"} div {class:"ai_chart_track",div {class:"ai_chart_bar",style:format!("width:{}%",value/max_value*100.0)}} span {"{text}"}}
                }
            }
        }
        if !answer.rows.is_empty() && !(answer.sources=="products" && answer.rows.len()==1) && answer.sources!="KAY POS Rust curated guidance" {
        div {class:"customers_table_scroll ai_answer_table",tabindex:0,aria_label:answer.title.clone(),
            table {class:"customers_table",
                thead {tr {for heading in &answer.headers {th {scope:"col","{heading}"}}}}
                tbody {for (index,row) in answer.rows.iter().enumerate() {tr {for (column,cell) in row.iter().enumerate() {td {
                    if column==0 && answer.receipt_ids.get(index).is_some() {
                        {let id=answer.receipt_ids[index];let mut open=open_receipt.clone();rsx!{button {class:"ai_text_action",disabled:detail_busy(),onclick:move |_|open(id),"{cell}"}}}
                    } else if column==0 && answer.sources!="comparison" && matches!(request.scope,Scope::Products|Scope::Inventory|Scope::Expenses|Scope::Customers|Scope::Credit|Scope::Employees) {
                        {let row=row.clone();let mut open=open_entity.clone();rsx!{button {class:"ai_text_action",disabled:detail_busy(),onclick:move |_|open(row.clone()),"{cell}"}}}
                    } else {"{cell}"}
                }}}}}
            }
        }
        }
        if answer.rows.is_empty() && answer.narrative.is_empty() {p {class:"ai_empty","ကိုက်ညီသည့်မှတ်တမ်း မရှိပါ။"}}
        if detail_busy(){p {role:"status","အသေးစိတ် ရယူနေပါတယ်…"}}
        if !detail_error().is_empty(){p {class:"ai_error",role:"alert","{detail_error}"}}
        if detail.read().is_some(){
            dialog {
                class:"ai_receipt_dialog",aria_label:"Receipt detail",
                onmounted:move |_|{document::eval("document.querySelector('.ai_receipt_dialog:not([open])')?.showModal();");},
                oncancel:move |event|{event.prevent_default();detail.set(None);},
                crate::ReceiptDetailPanel {detail:detail(),read_only:true,on_close:move |_|detail.set(None),on_print:move |_|{},on_refund:move |_|{}}
            }
        }
        if let Some(value)=entity_detail() {
            dialog {class:"ai_receipt_dialog ai_entity_dialog",aria_label:"အသေးစိတ်",
                onmounted:move |_|{document::eval("document.querySelector('.ai_entity_dialog:not([open])')?.showModal();");},
                oncancel:move |event|{event.prevent_default();entity_detail.set(None);},
                header {class:"receipt_detail_head",h3 {"{value.title}"} button {aria_label:"Close",onclick:move |_|entity_detail.set(None),crate::icons::Icon {name:"close"}}}
                div {class:"ai_entity_content",
                    p {class:"ai_entity_intro","{value.narrative}"}
                    if value.rows.is_empty(){p {"မှတ်တမ်း မရှိတော့ပါ။ Data ပြန်စစ်ပေးပါ။"}}
                    for row in &value.rows {dl {class:"ai_product_facts",for (heading,cell) in value.headers.iter().zip(row) {div {dt {"{heading}"} dd {if cell.trim().is_empty(){"—"}else{"{cell}"}}}}}}
                }
            }
        }
        button {class:"ai_text_action",onclick:move |_|{
            let answer = export_answer.clone();
            spawn(async move {
                if let Some(file) = rfd::AsyncFileDialog::new().add_filter("CSV", &["csv"]).set_file_name("kay-pos-ai.csv").save_file().await {
                    match file.write(ai::answer_csv(&answer).as_bytes()).await {
                        Ok(()) => export_notice.set("CSV သိမ်းပြီးပါပြီ".into()),
                        Err(error) => export_notice.set(format!("CSV သိမ်းမရပါ: {error}")),
                    }
                }
            });
        }, "Export CSV"}
        if !export_notice().is_empty() {p {role:"status","{export_notice}"}}
        details {class:"ai_sources",summary {"ဒေတာရင်းမြစ်နှင့် မှတ်ချက်များ"}
            p {"{answer.sources}"}
            small {"Updated {answer.generated_at}"}
            for note in &answer.notes {p {"{note}"}}
        }
    }}
}
