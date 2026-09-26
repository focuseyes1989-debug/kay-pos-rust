use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{
    auth::{Permission, Session},
    connect,
    service_orders::{self as jobs, Action, Job, Prompt},
};

fn action_button_label(action: Action) -> &'static str {
    match action {
        Action::Start => "Start",
        Action::Complete => "Complete",
        Action::Cancel => "Cancel",
        Action::Delete => "Delete",
        Action::Collect => "Mark as Collected",
    }
}

fn time(value: Option<chrono::NaiveDateTime>) -> String {
    value
        .map(|v| v.format("%d %b %Y %H:%M").to_string())
        .unwrap_or_else(|| "-".into())
}
fn urgency(job: &Job) -> &'static str {
    if jobs::ready(&job.status) || matches!(job.status.as_str(), "delivered" | "cancelled") {
        return "";
    }
    match job
        .expected_at
        .map(|d| (d - chrono::Local::now().naive_local()).num_minutes())
    {
        Some(n) if n < 0 => "overdue",
        Some(n) if n <= 1440 => "soon",
        Some(n) if n <= 4320 => "upcoming",
        _ => "",
    }
}
#[component]
pub fn ServiceOrdersPage(db_form: DbForm) -> Element {
    let session = use_context::<Signal<Option<Session>>>();
    let mut tab = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut applied = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut limit = use_signal(|| 100i64);
    let mut visible_rows = use_signal(Vec::<Job>::new);
    let mut loaded = use_signal(|| false);
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        let q = applied();
        let s = status();
        let n = limit();
        async move {
            async {
                let pool = connect(&source.database_config()?).await?;
                let rows = jobs::list(&pool, &q, &s, n + 1).await?;
                // Keep the displayed snapshot stable during polling and transient errors.
                if *visible_rows.peek() != rows {
                    visible_rows.set(rows);
                }
                if !*loaded.peek() {
                    loaded.set(true);
                }
                Ok::<(), anyhow::Error>(())
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let mut selected = use_signal(|| None::<i32>);
    let mut editor = use_signal(|| None::<(Job, bool)>);
    let mut confirmation = use_signal(|| None::<(Job, Action)>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut revision = use_signal(|| 0u64);
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if data.finished() && !busy() && !tab() && editor().is_none() && confirmation().is_none() {
                data.restart();
            }
        }
    });
    let rows = visible_rows();
    let current = rows
        .iter()
        .take(limit() as usize)
        .find(|j| Some(j.id) == selected())
        .cloned();
    let manage = session
        .read()
        .as_ref()
        .is_some_and(|s| s.allows(Permission::Manage));
    rsx! {
        style { {include_str!("../assets/service-orders.css")} }
        section {class:if tab(){"service_page service_prompts_page"}else{"service_page service_jobs_page"},
            header {class:"service_heading",
                div {class:"receipt_tabs",
                    button {class:if !tab(){"active"}else{""},onclick:move |_|tab.set(false),"Jobs"}
                    button {class:if tab(){"active"}else{""},onclick:move |_|tab.set(true),"Design Prompts"}
                }
            }
            if tab() {PromptLibrary {db_form:db_form.clone(),job:current.clone()}}
            else {
                div {class:"service_filters",
                    input {aria_label:"Search jobs",placeholder:"Job name, details, notes or number",value:query(),oninput:move |e|query.set(e.value()),onkeydown:move |e|if e.key()==Key::Enter{applied.set(query());limit.set(100);}}
                    select {aria_label:"Job status",value:status(),onchange:move |e|{status.set(e.value());limit.set(100);},
                        for (value,label) in [("","All statuses"),("pending","Pending"),("in_progress","In Progress"),("ready_for_pickup","Ready for Pickup"),("delivered","Delivered"),("cancelled","Cancelled")] {option {value,"{label}"}}
                    }
                    button {onclick:move |_|{applied.set(query());limit.set(100);},"Search"}
                    button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:busy(),onclick:move |_|{data.restart();revision+=1;},"Refresh"}
                }
                div {class:"service_job_messages",
                if !notice().is_empty(){p {role:"status","{notice}"}}
                if let Some(Err(e))=data.read().as_ref(){p {role:"alert","{e}"}}
                if !loaded() && !data.finished(){p {role:"status","Loading jobs..."}}
                }
                div {class:"service_jobs_columns",
                div {class:"service_jobs_list",
                div {class:"service_table_scroll",
                    table {class:"service_table",
                        thead {tr {for label in ["Date / Time","Job Name","Details","Appointment","Status","Working By","Started At","Work Completed By","Completed At","Delivered By","Delivered At"] {th {"{label}"}}}}
                        tbody {for job in rows.iter().take(limit() as usize) {
                            tr {key:"{job.id}",class:if Some(job.id)==selected(){"selected"}else{""},
                                tabindex:0,aria_selected:Some(job.id)==selected(),
                                onkeydown:{let id=job.id;move |e:KeyboardEvent|if e.key()==Key::Enter||e.key()==Key::Character(" ".into()){e.prevent_default();selected.set(Some(id));}},
                                onclick:{let id=job.id;move |_|selected.set(Some(id))},
                                td {{time(Some(job.received_at))}}
                                td {span {class:"service_job_name","{job.job_title}"} small {"{job.order_no}"}}
                                td {class:"service_details_cell","{job.complaint}"}
                                td {"data-urgency":urgency(job),{time(job.expected_at)}}
                                td {span {class:"service_status","data-status":jobs::status_label(&job.status),"{jobs::status_label(&job.status)}"}}
                                td {"{job.started_by}"} td {{time(job.started_at)}} td {"{job.completed_by}"} td {{time(job.completed_at)}} td {"{job.delivered_by}"} td {{time(job.delivered_at)}}
                            }
                        }}
                    }
                    if loaded()&&rows.is_empty(){p {class:"service_empty","No jobs found"}}
                }
                if rows.len()>limit() as usize && limit()<10000 {button {onclick:move |_|limit+=100,"Load more"}}
                }
                section {class:"service_job_detail",aria_label:"Job detail",
                div {class:"service_detail_top_actions",role:"toolbar",aria_label:"Job editing actions",
                    button {class:"customer_primary",disabled:busy(),onclick:{let source=db_form.clone();move |_|{
                        let Some(actor)=session.read().clone() else{return;};
                        let source=source.clone();busy.set(true);
                        spawn(async move{let result=async{jobs::reserve(&connect(&source.database_config()?).await?,&actor).await}.await;busy.set(false);match result{Ok(j)=>editor.set(Some((j,true))),Err(e)=>error.set(format!("{e:#}"))}});
                    }},"New"}
                    button {disabled:busy()||current.as_ref().is_none_or(|j|matches!(j.status.as_str(),"delivered"|"cancelled")),onclick:{let j=current.clone();move |_|if let Some(j)=j.clone(){editor.set(Some((j,false)));}},crate::icons::ActionLabel { label:"Edit" }}
                    button {class:"customer_primary",disabled:busy()||current.as_ref().is_none_or(|j|!Action::Complete.allowed(&j.status)),
                        onclick:{let j=current.clone();move |_|if let Some(j)=j.clone(){confirmation.set(Some((j,Action::Complete)));}},"Complete"}
                }
                div {class:"service_job_detail_body",
                if let Some(job)=current.clone() {
                        div {class:"service_detail_title",h3 {"{job.job_title}"} span {"{job.order_no}"}}
                        p {span {class:"service_status","data-status":jobs::status_label(&job.status),"{jobs::status_label(&job.status)}"}}
                        div {class:"service_detail_copy",
                            div {h4 {"Customer"} p {"{job.customer_name}"} p {"{job.customer_phone}"}}
                            div {h4 {"Details"} p {"{job.complaint}"}}
                            div {h4 {"Notes"} p {"{job.internal_notes}"}}
                        }
                        details {summary {"Status history"} JobHistory {key:"{job.id}",db_form:db_form.clone(),id:job.id,updated_at:job.updated_at,revision:revision()}}
                } else {
                    p {class:"service_empty","No job selected"}
                }
                }
                div {class:"service_job_actions service_job_toolbar",role:"toolbar",aria_label:"Job actions",
                    for action in [Action::Start,Action::Collect,Action::Delete,Action::Cancel] {
                        button {class:if matches!(action,Action::Cancel|Action::Delete){"danger"}else{"customer_primary"},
                            disabled:busy()||(action==Action::Delete&&!manage)||current.as_ref().is_none_or(|j|!action.allowed(&j.status)),
                            title:if action==Action::Delete&&!manage{"Manager permission required"}else{""},
                            onclick:{let j=current.clone();move |_|if let Some(j)=j.clone(){confirmation.set(Some((j,action)));}},
                            "{action_button_label(action)}"
                        }
                    }
                }
                }
                }
            }
        }
        if let Some((job,new))=editor(){
            JobEditor {job,new,db_form:db_form.clone(),on_close:move |_|editor.set(None),on_saved:move |id|{editor.set(None);selected.set(Some(id));data.restart();revision+=1;notice.set("Job saved".into());}}
        }
        if let Some((job,action))=confirmation(){
            JobConfirmation {job,action,db_form:db_form.clone(),on_close:move |_|confirmation.set(None),on_saved:move |_|{confirmation.set(None);data.restart();revision+=1;notice.set("Job updated".into());}}
        }
        if !error().is_empty(){MessageBox {title:"Service Order",message:error(),on_close:move |_|error.set(String::new())}}
    }
}

#[component]
fn JobHistory(db_form: DbForm, id: i32, updated_at: chrono::NaiveDateTime, revision: u64) -> Element {
    let data = use_resource(use_reactive(
        (&db_form, &id, &updated_at, &revision),
        move |(source, id, _, _)| async move {
            async { jobs::history_list(&connect(&source.database_config()?).await?, id).await }
                .await
                .map_err(|e| format!("{e:#}"))
        },
    ));
    rsx! {match data.read().as_ref(){
        Some(Ok(rows))=>rsx!{for row in rows {p {class:"service_history","{time(Some(row.changed_at))} · {row.changed_by} · {jobs::status_label(&row.to_status)}" span {"{row.note}"}}}},
        Some(Err(e))=>rsx!{p {role:"alert","{e}"}},
        None=>rsx!{p {"Loading..."}}
    }}
}
#[component]
fn JobEditor(
    job: Job,
    new: bool,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<i32>,
) -> Element {
    let session = use_context::<Signal<Option<Session>>>();
    let mut form = use_signal(|| job.clone());
    let mut received = use_signal(|| job.received_at.format("%Y-%m-%dT%H:%M").to_string());
    let mut appointment = use_signal(|| {
        job.expected_at
            .map(|v| v.format("%Y-%m-%dT%H:%M").to_string())
            .unwrap_or_else(|| {
                chrono::Local::now()
                    .naive_local()
                    .format("%Y-%m-%dT%H:%M")
                    .to_string()
            })
    });
    let mut scheduled = use_signal(|| job.expected_at.is_some());
    let mut saving = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! {div {class:"modal_backdrop",section {class:"service_dialog",role:"dialog",aria_modal:"true",aria_label:if new{"New Job"}else{"Edit Job"},
        h2 {if new{"New Job"}else{"Edit Job"}} small {"{job.order_no}"}
        div {class:"service_form",
            label {"Received" input {r#type:"datetime-local",disabled:saving()||!new,value:received(),oninput:move |e|received.set(e.value())}}
            label {"Job Name" input {autofocus:true,disabled:saving(),value:form().job_title,oninput:move |e|form.write().job_title=e.value()}}
            label {"Customer name" input {disabled:saving(),value:form().customer_name,oninput:move |e|form.write().customer_name=e.value()}}
            label {"Phone" input {r#type:"tel",disabled:saving(),value:form().customer_phone,oninput:move |e|form.write().customer_phone=e.value()}}
            label {"Details" textarea {rows:4,disabled:saving(),value:form().complaint,oninput:move |e|form.write().complaint=e.value()}}
            label {class:"service_check",input {r#type:"checkbox",checked:scheduled(),disabled:saving(),onchange:move |e|scheduled.set(e.checked())} "Set appointment"}
            input {aria_label:"Appointment",r#type:"datetime-local",disabled:saving()||!scheduled(),value:appointment(),oninput:move |e|appointment.set(e.value())}
            label {"Notes" textarea {rows:3,disabled:saving(),value:form().internal_notes,oninput:move |e|form.write().internal_notes=e.value()}}
        }
        if !error().is_empty(){p {role:"alert","{error}"}}
        div {class:"service_dialog_actions",
            button {disabled:saving(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel { label:"Cancel" }}
            button {class:"customer_primary",disabled:saving(),onclick:move |_|{
                if saving(){return;}
                let Some(actor)=session.read().clone() else{return;};
                let mut j=form();
                let parse=|v:&str|chrono::NaiveDateTime::parse_from_str(v,"%Y-%m-%dT%H:%M");
                if new {match parse(&received()){Ok(v)=>j.received_at=v,Err(_)=>{error.set("Enter a valid received date and time".into());return;}}}
                j.expected_at=if scheduled(){match parse(&appointment()){Ok(v)=>Some(v),Err(_)=>{error.set("Enter a valid appointment".into());return;}}}else{None};
                saving.set(true);let source=db_form.clone();spawn(async move{
                    let result=async{jobs::save(&connect(&source.database_config()?).await?,&actor,&j,new).await}.await;
                    saving.set(false);match result{Ok(())=>on_saved.call(j.id),Err(e)=>error.set(format!("{e:#}"))}
                });
            },if saving(){"Saving..."}else{"Save Job"}}
        }
    }}}
}
#[component]
fn JobConfirmation(
    job: Job,
    action: Action,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let session = use_context::<Signal<Option<Session>>>();
    let mut note = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! {div {class:"modal_backdrop",section {class:"service_dialog",role:"dialog",aria_modal:"true",aria_label:action.label(),
        h2 {"{action.label()}"} h3 {"{job.job_title}"} p {"{job.order_no}"}
        if action==Action::Collect {h4 {"Job notes"} p {class:"service_preserve","{job.internal_notes}"}}
        if action==Action::Delete {p {"Permanently delete this cancelled job and its history?"}}
        label {class:"service_form","Action note" textarea {value:note(),disabled:busy(),oninput:move |e|note.set(e.value())}}
        if !error().is_empty(){p {role:"alert","{error}"}}
        div {class:"service_dialog_actions",button {disabled:busy(),onclick:move |_|on_close.call(()),"Back"}
            button {class:if action==Action::Delete{"danger"}else{"customer_primary"},disabled:busy(),onclick:move |_|{
                if busy(){return;}let Some(actor)=session.read().clone() else{return;};
                let source=db_form.clone();let j=job.clone();let n=note();busy.set(true);
                spawn(async move{let result=async{jobs::act(&connect(&source.database_config()?).await?,&actor,&j,action,&n).await}.await;busy.set(false);match result{Ok(())=>on_saved.call(()),Err(e)=>error.set(format!("{e:#}"))}});
            }, {if busy() { "Saving..." } else { action_button_label(action) }}}
        }
    }}}
}

#[component]
fn PromptLibrary(db_form: DbForm, job: Option<Job>) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        async move {
            async { jobs::prompts(&connect(&source.database_config()?).await?).await }
                .await
                .map_err(|e| format!("{e:#}"))
        }
    });
    let mut query = use_signal(String::new);
    let mut category = use_signal(String::new);
    let mut inactive = use_signal(|| false);
    let mut selected = use_signal(|| None::<Prompt>);
    let mut editor = use_signal(|| None::<Prompt>);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let rows = data
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let categories = rows
        .iter()
        .map(|p| p.category.clone())
        .filter(|c| !c.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    let filter = query().to_lowercase();
    let mut prompt_page = use_signal(|| 0usize);
    use_effect(move || { let _ = (query(), category(), inactive()); prompt_page.set(0); });
    let filtered: Vec<_> = rows.iter().filter(|p| (inactive() || p.active == 1)
        && (category().is_empty() || p.category == category())
        && format!("{} {} {}",p.title,p.category,p.prompt_text).to_lowercase().contains(&filter)).collect();
    let pages=filtered.len().div_ceil(12).max(1);
    let page=prompt_page().min(pages-1);
    rsx! {
        div {class:"service_filters",
            input {aria_label:"Search prompts",placeholder:"Search prompts",value:query(),oninput:move |e|query.set(e.value())}
            select {aria_label:"Prompt category",value:category(),onchange:move |e|category.set(e.value()),option {value:"","All categories"} for c in categories {option {value:c.clone(),"{c}"}}}
            label {class:"service_check",input {r#type:"checkbox",checked:inactive(),onchange:move |e|inactive.set(e.checked())} "Include inactive"}
            button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:loading(),onclick:move |_|{data.restart();selected.set(None);},"Refresh"}
        }
        div {class:"service_prompt_messages",
        if let Some(Err(e))=data.read().as_ref(){p {role:"alert","{e}"}}
        if !notice().is_empty(){p {role:"status","{notice}"}}
        if !data.finished()||loading(){p {role:"status","Loading..."}}
        }
        div {class:"service_prompt_columns",
            div {class:"service_prompt_gallery",
            div {class:"service_prompt_list",
                for prompt in filtered.iter().skip(page*12).take(12) {
                    button {key:"{prompt.id}",aria_pressed:selected().as_ref().is_some_and(|s|s.id==prompt.id),class:if selected().as_ref().is_some_and(|s|s.id==prompt.id){"service_prompt_item selected"}else{"service_prompt_item"},disabled:loading(),onclick:{let source=db_form.clone();let id=prompt.id;move |_|{
                        let source=source.clone();loading.set(true);spawn(async move{let result=async{jobs::prompt(&connect(&source.database_config()?).await?,id).await}.await;loading.set(false);match result{Ok(p)=>selected.set(Some(p)),Err(e)=>error.set(format!("{e:#}"))}});
                    }},
                        div {class:"service_prompt_thumbnail",
                            PromptThumbnail { key:"{prompt.id}-{prompt.updated_at:?}",db_form:db_form.clone(),id:prompt.id,title:prompt.title.clone() }
                        }
                        div {class:"service_prompt_caption",strong {"{prompt.title}"} small {"{prompt.category}"} if prompt.active==0{small {class:"service_prompt_inactive","Inactive"}}}
                    }
                }
                if data.finished()&&filtered.is_empty(){p {class:"service_empty","No design prompts"}}
            }
            div {class:"service_prompt_pagination",
                button {disabled:page==0,onclick:move |_|prompt_page.set(page.saturating_sub(1)),"Previous"}
                span {"{page+1} / {pages}"}
                button {disabled:page+1>=pages,onclick:move |_|prompt_page.set(page+1),"Next"}
            }
            }
            section {class:"service_prompt_detail",
                div {class:"service_detail_top_actions",role:"toolbar",aria_label:"Prompt actions",
                    button {class:"customer_primary",onclick:move |_|editor.set(Some(Prompt{active:1,..Default::default()})),"New Prompt"}
                }
                div {class:"service_prompt_detail_body",
                if let Some(prompt)=selected(){
                    h3 {"{prompt.title}"} small {"{prompt.category}"}
                    if prompt.image_data.starts_with("data:image/") {img {class:"service_prompt_image",src:prompt.image_data.clone(),alt:prompt.image_name.clone()}}
                    p {class:"service_preserve","{prompt.prompt_text}"}
                }else{p {class:"service_empty","Select a design prompt"}}
                }
                if let Some(prompt)=selected(){
                    div {class:"service_job_actions service_prompt_footer",
                        button {onclick:{let p=prompt.clone();move |_|editor.set(Some(p.clone()))},crate::icons::ActionLabel { label:"Edit" }}
                        button {onclick:{let text=prompt.prompt_text.clone();move |_|{let text=text.clone();spawn(async move{match copy_text(text).await{Ok(())=>notice.set("Prompt copied".into()),Err(e)=>error.set(e)}});}},"Copy Prompt"}
                        if let Some(j)=job.clone(){
                            button {class:"customer_primary",title:"Copy prompt with selected job details",onclick:{let text=jobs::render_prompt(&prompt.prompt_text,&j);move |_|{let text=text.clone();spawn(async move{match copy_text(text).await{Ok(())=>notice.set("Job prompt copied".into()),Err(e)=>error.set(e)}});}},"Copy for Selected Job"}
                        }
                    }
                }
            }
        }
        if let Some(prompt)=editor(){PromptEditor {prompt,db_form:db_form.clone(),on_close:move |_|editor.set(None),on_saved:move |_|{editor.set(None);selected.set(None);data.restart();notice.set("Prompt saved".into());}}}
        if !error().is_empty(){MessageBox {title:"Design Prompts",message:error(),on_close:move |_|error.set(String::new())}}
    }
}
fn thumbnail(data: &str) -> anyhow::Result<String> {
    use base64::Engine;
    let (_, encoded)=data.split_once(',').ok_or_else(||anyhow::anyhow!("Invalid image"))?;
    anyhow::ensure!(encoded.len()<=5_000_000,"Oversized image");
    let bytes=base64::engine::general_purpose::STANDARD.decode(encoded)?;
    let mut reader=image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(12000); limits.max_image_height=Some(12000);
    limits.max_alloc=Some(64*1024*1024); reader.limits(limits);
    let small=reader.decode()?.thumbnail(320,240);
    let mut output=std::io::Cursor::new(Vec::new());
    small.write_to(&mut output,image::ImageFormat::Png)?;
    Ok(format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(output.into_inner())))
}

#[cfg(test)]
mod thumbnail_tests {
    use super::*;
    #[test]
    fn thumbnail_is_bounded_and_rejects_invalid_data() {
        use base64::Engine;
        let source=image::DynamicImage::new_rgb8(800,600);
        let mut bytes=std::io::Cursor::new(Vec::new());
        source.write_to(&mut bytes,image::ImageFormat::Png).unwrap();
        let input=format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes.into_inner()));
        let output=thumbnail(&input).unwrap();
        let image=image::load_from_memory(&base64::engine::general_purpose::STANDARD.decode(output.split_once(',').unwrap().1).unwrap()).unwrap();
        assert_eq!((image.width(),image.height()),(320,240));
        assert!(thumbnail("invalid").is_err());
        assert!(thumbnail("data:image/png;base64,YmFk").is_err());
    }
}

#[component]
fn PromptThumbnail(db_form:DbForm,id:i32,title:String)->Element {
    let data=use_resource(move || {let form=db_form.clone();async move {
        static SLOTS:std::sync::OnceLock<tokio::sync::Semaphore>=std::sync::OnceLock::new();
        let result=async {
            let _permit=SLOTS.get_or_init(||tokio::sync::Semaphore::new(3)).acquire().await?;
            let source=jobs::prompt_image(&connect(&form.database_config()?).await?,id).await?;
            if source.is_empty(){return Ok::<_,anyhow::Error>(None);}
            let url=tokio::task::spawn_blocking(move ||thumbnail(&source)).await??;
            Ok(Some(url))
        }.await;
        result.map_err(|_|"Image unavailable")
    }});
    rsx!{match data.read().as_ref(){
        Some(Ok(Some(url)))=>rsx!{img {src:url.clone(),alt:title,loading:"lazy",decoding:"async"}},
        Some(Ok(None))=>rsx!{span {"No image"}},
        Some(Err(_))=>rsx!{span {"Image unavailable"}},
        None=>rsx!{span {"Loading image..."}}
    }}
}

async fn copy_text(text: String) -> Result<(), String> {
    let encoded = serde_json::to_string(&text).map_err(|e| e.to_string())?;
    document::eval(&format!("await navigator.clipboard.writeText({encoded});"))
        .await
        .map_err(|e| format!("Could not copy prompt: {e}"))?;
    Ok(())
}
#[component]
fn PromptEditor(
    prompt: Prompt,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let session = use_context::<Signal<Option<Session>>>();
    let mut form = use_signal(|| prompt.clone());
    let mut busy = use_signal(|| false);
    let mut image_busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! {div {class:"modal_backdrop",section {class:"service_dialog service_prompt_editor",role:"dialog",aria_modal:"true",aria_label:"Design Prompt",
        h2 {"Design Prompt"}
        div {class:"service_prompt_editor_columns",
        div {class:"service_form",
            label {"Title" input {autofocus:true,disabled:busy(),value:form().title,oninput:move |e|form.write().title=e.value()}}
            label {"Category" input {disabled:busy(),value:form().category,oninput:move |e|form.write().category=e.value()}}
            label {"Sort order" input {r#type:"number",min:0,disabled:busy(),value:form().sort_order,oninput:move |e|if let Ok(n)=e.value().parse(){form.write().sort_order=n;}}}
            label {"Prompt" textarea {rows:8,disabled:busy(),value:form().prompt_text,oninput:move |e|form.write().prompt_text=e.value()}}
            label {class:"service_check",input {r#type:"checkbox",disabled:busy(),checked:form().active==1,onchange:move |e|form.write().active=if e.checked(){1}else{0}} "Active"}
        }
        section {class:"service_prompt_photo",aria_label:"Sample photo",
            h3 {"Sample image"}
            div {class:"service_prompt_photo_preview",
                if !form().image_data.is_empty(){img {src:form().image_data,alt:"Sample image"}}
                else {span {"No image"}}
            }
            div {class:"service_job_actions",
                button {disabled:busy()||image_busy(),onclick:move |_|{image_busy.set(true);spawn(async move{
                    if let Some(file)=rfd::AsyncFileDialog::new().add_filter("Images",&["png","jpg","jpeg","webp"]).pick_file().await{
                        let result=async{
                            use base64::Engine;
                            anyhow::ensure!(tokio::fs::metadata(file.path()).await?.len()<=3_500_000,"Choose an image under 3.5 MB");
                            let bytes=tokio::fs::read(file.path()).await?;
                            let mime=if bytes.starts_with(b"\x89PNG"){ "image/png" }else if bytes.starts_with(&[255,216,255]){"image/jpeg"}else{"image/webp"};
                            let mut p=form();p.image_name=file.file_name();p.image_data=format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes));
                            // Validate the image independently while the text fields are still being edited.
                            let mut check=p.clone();check.title="Image".into();check.prompt_text="Image".into();jobs::validate_prompt(&check)?;
                            Ok::<_,anyhow::Error>(p)
                        }.await;
                        match result{Ok(p)=>form.set(p),Err(e)=>error.set(format!("{e:#}"))}
                    }image_busy.set(false);
                });},"Upload Sample Image"}
                button {disabled:busy()||image_busy()||form().image_data.is_empty(),onclick:move |_|{form.write().image_data.clear();form.write().image_name.clear();},"Remove Image"}
            }
        }
        }
        if !error().is_empty(){p {role:"alert","{error}"}}
        div {class:"service_dialog_actions",button {disabled:busy()||image_busy(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel { label:"Cancel" }}
            button {class:"customer_primary",disabled:busy()||image_busy(),onclick:move |_|{
                if busy(){return;}let Some(actor)=session.read().clone() else{return;};let p=form();
                if let Err(e)=jobs::validate_prompt(&p){error.set(e.to_string());return;}
                let source=db_form.clone();busy.set(true);spawn(async move{let result=async{jobs::save_prompt(&connect(&source.database_config()?).await?,&actor,&p).await}.await;busy.set(false);match result{Ok(())=>on_saved.call(()),Err(e)=>error.set(format!("{e:#}"))}});
            },if busy(){"Saving..."}else{crate::icons::ActionLabel { label:"Save Prompt" }}}
        }
    }}}
}
