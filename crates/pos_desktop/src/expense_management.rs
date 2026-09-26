use crate::{DbForm, icons::ActionLabel};
use dioxus::prelude::*;
use pos_core::{auth::Session,expense_management::{self as core,Action,Budget,Command,Settings}};
use chrono::Datelike;
use std::path::PathBuf;
use anyhow::{ensure,Result};

fn journal(db:&DbForm)->Result<PathBuf>{
    let root=std::env::var_os("LOCALAPPDATA").ok_or_else(||anyhow::anyhow!("LOCALAPPDATA is unavailable"))?;
    Ok(PathBuf::from(root).join("KAY POS Rust/management-pending").join(format!("{}.json",pos_core::auth::fingerprint(db.database_config()?.database_url.as_bytes()))))
}
pub fn pending(db:&DbForm)->Result<Option<(String,Command)>> {
    let p=journal(db)?;if !p.exists(){return Ok(None)}
    Ok(Some(serde_json::from_slice(&std::fs::read(p)?)?))
}
fn persist(db:&DbForm,actor:&Session,c:&Command)->Result<()> {
    let bytes=serde_json::to_vec(&(actor.username(),c))?;
    if let Some((user,old))=pending(db)? {ensure!(user==actor.username()&&serde_json::to_vec(&old)?==serde_json::to_vec(c)?,"Resolve the previous management request first");return Ok(())}
    let p=journal(db)?;std::fs::create_dir_all(p.parent().unwrap())?;
    let mut temp=tempfile::NamedTempFile::new_in(p.parent().unwrap())?;
    use std::io::Write;temp.write_all(&bytes)?;temp.as_file().sync_all()?;temp.persist_noclobber(p)?;Ok(())
}
fn clear(db:&DbForm,actor:&Session,c:&Command)->Result<()> {
    if let Some((user,old))=pending(db)? {ensure!(user==actor.username()&&serde_json::to_vec(&old)?==serde_json::to_vec(c)?,"Pending request changed");std::fs::remove_file(journal(db)?)?;}Ok(())
}
pub fn command(action:Action)->Command{Command{request_id:pos_core::auth::new_request_id(),action}}

#[component]
pub fn Operation(db_form:DbForm,actor:Session,command:Command,on_done:EventHandler<()>) -> Element {
    let mut busy=use_signal(||false);let mut message=use_signal(String::new);
    let mut sent=use_signal(||pending(&db_form).ok().flatten().is_some());
    rsx!{div{class:"modal_backdrop",section{class:"customer_dialog phase4_dialog",role:"dialog",aria_modal:"true",aria_label:"Save changes",
        h2{if sent(){"Pending changes"}else{"Save changes?"}}
        p{role:"status","{message}"}
        div{class:"customers_actions",
            button{disabled:busy(),onclick:{let db=db_form.clone();let actor=actor.clone();let cmd=command.clone();move |_|{
                if !sent(){on_done.call(());return} busy.set(true);let db=db.clone();let actor=actor.clone();let cmd=cmd.clone();
                spawn(async move{let result=async{let pool=pos_core::connect(&db.database_config()?).await?;let saved=core::resolve(&pool,&actor,&cmd).await?;clear(&db,&actor,&cmd)?;Ok::<_,anyhow::Error>(saved)}.await;
                    busy.set(false);match result{Ok(_)=>on_done.call(()),Err(e)=>message.set(format!("{e:#}"))}});
            }},if sent(){"Check and close"}else{"Cancel"}},
            button{class:"customer_primary",disabled:busy(),onclick:move |_|{
                if busy(){return} if let Err(e)=persist(&db_form,&actor,&command){message.set(format!("{e:#}"));return}
                sent.set(true);busy.set(true);let db=db_form.clone();let actor=actor.clone();let cmd=command.clone();
                spawn(async move{let result=async{let pool=pos_core::connect(&db.database_config()?).await?;core::execute(&pool,&actor,&cmd).await?;clear(&db,&actor,&cmd)?;Ok::<_,anyhow::Error>(())}.await;
                    busy.set(false);match result{Ok(())=>on_done.call(()),Err(e)=>message.set(format!("{e:#}"))}});
            },if busy(){"Saving..."}else if sent(){"Retry original changes"}else{ActionLabel{label:"Save"}}}
        }
    }}}
}

#[component]
pub fn BudgetPanel(db_form:DbForm,actor:Session,alerts:bool)->Element {
    let mut month=use_signal(||chrono::Local::now().format("%Y-%m").to_string());
    let mut refresh=use_signal(||0u64);let source=db_form.clone();let who=actor.clone();
    let data=use_resource(move||{let source=source.clone();let who=who.clone();let month=month();let _=refresh();async move{
        async{let d=pos_core::discounts::date(&format!("{month}-01"))?;let pool=pos_core::connect(&source.database_config()?).await?;core::load(&pool,&who,d.year(),d.month() as i32).await}.await.map_err(|e:anyhow::Error|format!("{e:#}"))
    }});
    let mut edit=use_signal(||None::<(Option<Budget>,Budget)>);let mut settings=use_signal(||None::<(Settings,Settings)>);
    let mut op=use_signal(||pending(&db_form).ok().flatten().map(|(_,c)|c));
    let mut error=use_signal(||pending(&db_form).err().map(|e|e.to_string()).unwrap_or_default());let mut checking=use_signal(||false);
    rsx!{section{class:"phase4_panel",
        button{hidden:true,"data-page-refresh":"true",onclick:move |_|refresh+=1}
        div{class:"phase4_toolbar",
            label{"Month" input{r#type:"month",value:"{month}",oninput:move|e|month.set(e.value())}}
            if alerts{button{disabled:checking(),onclick:{let db=db_form.clone();let actor=actor.clone();move |_|{let db=db.clone();let actor=actor.clone();checking.set(true);spawn(async move{let result=async{core::check_alerts(&pos_core::connect(&db.database_config()?).await?,&actor,true).await}.await;checking.set(false);match result{Ok(_)=>refresh+=1,Err(e)=>error.set(format!("{e:#}"))}});}},ActionLabel{label:"Check alerts"}}}
        }
        if !error().is_empty(){p{role:"alert","{error}"}}
        match data.read().as_ref(){
            Some(Err(e))=>rsx!{p{role:"alert","{e}"}},None=>rsx!{p{"Loading..."}},
            Some(Ok((rows,config,history)))=>rsx!{
                if alerts {
                    div{class:"phase4_toolbar",span{if config.enable_notifications==1 {"Enabled"}else{"Disabled"}} span{"Threshold {config.warning_threshold}%"} span{"{config.check_frequency}"}
                        button{onclick:{let c=config.clone();move |_|settings.set(Some((c.clone(),c.clone())))},ActionLabel{label:"Edit"}}
                    }
                    div{class:"phase4_table",table{thead{tr{th{"Date"}th{"Category"}th{"Message"}th{"Status"}th{}}}tbody{
                        for a in history {
                            tr { key:"{a.id}",
                                td {"{a.created_at}"} td {"{a.category}"} td {"{a.message}"}
                                td { if a.is_read==1 {"Read"} else {"Unread"} }
                                td { if a.is_read==0 {
                                    button { onclick: {let i=a.id;move |_|op.set(Some(command(Action::ReadAlert{i})))}, ActionLabel {label:"Mark read"} }
                                } }
                            }
                        }
                    }}}
                    if history.is_empty(){p{"No alerts for this month."}}
                } else {
                    div{class:"phase4_table",table{thead{tr{th{"Category"}th{"Budget"}th{"Actual"}th{"Remaining"}th{"Usage"}th{"Status"}th{}}}tbody{
                        for r in rows {tr{key:"{r.category}",td{"{r.category}"}td{"{r.budget_amount} {crate::regional::current().symbol()}"}td{"{r.actual}"}td{"{r.budget_amount-r.actual}"}
                            td{if r.budget_amount>0.into(){progress{max:"100",value:"{(r.actual/r.budget_amount*pos_core::Money::from(100)).min(100.into())}"}span{"{(r.actual/r.budget_amount*pos_core::Money::from(100)).round_dp(1)}%"}}}td{{core::alert_level(r.budget_amount,r.actual,config.warning_threshold).unwrap_or(if r.budget_amount.is_zero(){"No budget"}else{"Within budget"})}}
                            td{button{onclick:{let r=r.clone();move |_|{if let Ok(d)=pos_core::discounts::date(&format!("{}-01",month())){let b=Budget{category:r.category.clone(),month:d.month() as i32,year:d.year(),budget_amount:r.budget_amount,notes:r.notes.clone()};edit.set(Some((r.exists.then_some(b.clone()),b)));}}},ActionLabel{label:"Edit"}}}
                        }}
                    }}}
                }
            }
        }
    }
    if let Some((old,b))=edit(){BudgetEditor{old,b,onsave:move|c|{edit.set(None);op.set(Some(c));},onclose:move |_|edit.set(None)}}
    if let Some((old,s))=settings(){SettingsEditor{old,s,onsave:move|c|{settings.set(None);op.set(Some(c));},onclose:move |_|settings.set(None)}}
    if let Some(c)=op(){Operation{db_form:source_for_operation(&db_form),actor,command:c,on_done:move |_|{op.set(None);refresh+=1;}}}
    }
}
fn source_for_operation(db:&DbForm)->DbForm{db.clone()}

#[component]
fn BudgetEditor(old:Option<Budget>,b:Budget,onsave:EventHandler<Command>,onclose:EventHandler<()>)->Element{
    let mut amount=use_signal(||b.budget_amount.to_string());let mut notes=use_signal(||b.notes.clone());let mut error=use_signal(String::new);
    rsx!{div{class:"modal_backdrop",section{class:"customer_dialog phase4_dialog",h2{"{b.category} / {b.year}-{b.month:02}"}
        label{"Budget" input{r#type:"number",min:"0",step:"0.01",value:"{amount}",oninput:move|e|amount.set(e.value())}}
        label{"Notes" textarea{value:"{notes}",oninput:move|e|notes.set(e.value())}}
        p{role:"alert","{error}"}div{class:"customers_actions",button{onclick:move |_|onclose.call(()),ActionLabel{label:"Cancel"}}
            button{class:"customer_primary",onclick:move |_|{match amount().parse(){Ok(value)=>{let mut new=b.clone();new.budget_amount=value;new.notes=notes();onsave.call(command(Action::Budget{old:old.clone(),new}));},Err(_)=>error.set("Enter a valid amount".into())}},ActionLabel{label:"Save budget"}}
        }
    }}}
}
#[component]
fn SettingsEditor(old:Settings,s:Settings,onsave:EventHandler<Command>,onclose:EventHandler<()>)->Element{
    let mut form=use_signal(||s);rsx!{div{class:"modal_backdrop",section{class:"customer_dialog phase4_dialog",h2{"Expense alerts"}
        label{input{r#type:"checkbox",checked:form().enable_notifications==1,onchange:move|e|form.write().enable_notifications=i32::from(e.checked())}"Enabled"}
        label{"Warning threshold (%)" input{r#type:"number",min:"50",max:"100",value:"{form().warning_threshold}",oninput:move|e|{if let Ok(n)=e.value().parse(){form.write().warning_threshold=n}}}}
        label{"Frequency" select{value:"{form().check_frequency}",onchange:move|e|form.write().check_frequency=e.value(),for f in ["daily","weekly","monthly"]{option{value:f,"{f}"}}}}
        div{class:"customers_actions",button{onclick:move |_|onclose.call(()),ActionLabel{label:"Cancel"}}button{class:"customer_primary",onclick:move |_|onsave.call(command(Action::Settings{old:old.clone(),new:form()})),ActionLabel{label:"Save settings"}}}
    }}}
}

#[component]
pub fn Attachments(db_form:DbForm,actor:Session,expense_id:i32,onclose:EventHandler<()>)->Element{
    let mut refresh=use_signal(||0u64);let source=db_form.clone();let who=actor.clone();
    let data=use_resource(move||{let db=source.clone();let who=who.clone();let _=refresh();async move{async{core::attachments(&pos_core::connect(&db.database_config()?).await?,&who,expense_id).await}.await.map_err(|e:anyhow::Error|format!("{e:#}"))}});
    let mut op=use_signal(||pending(&db_form).ok().flatten().map(|(_,c)|c));let mut error=use_signal(String::new);let mut busy=use_signal(||false);
    rsx!{div{class:"modal_backdrop",section{class:"customer_dialog phase4_dialog",role:"dialog",aria_modal:"true",aria_label:"Expense attachments",
        header{class:"phase4_toolbar",h2{"Attachments"}button{disabled:busy(),onclick:move |_|onclose.call(()),ActionLabel{label:"Close"}}}
        p{role:"status","{error}"}
        button{class:"customer_primary",disabled:busy(),onclick:move |_|{busy.set(true);spawn(async move{
            if let Some(file)=rfd::AsyncFileDialog::new().add_filter("PDF / Images",&["pdf","png","jpg","jpeg"]).pick_file().await {
                let result=async{let meta=tokio::fs::metadata(file.path()).await?;ensure!(meta.len()<=10485760,"Maximum file size is 10 MB");let content=tokio::fs::read(file.path()).await?;let filename=file.file_name();core::file_type(&filename,&content)?;Ok::<_,anyhow::Error>(command(Action::Attach{expense_id,filename,content}))}.await;
                match result{Ok(c)=>op.set(Some(c)),Err(e)=>error.set(format!("{e:#}"))}
            }busy.set(false);
        });},ActionLabel{label:"Upload"}}
        match data.read().as_ref(){None=>rsx!{p{"Loading..."}},Some(Err(e))=>rsx!{p{role:"alert","{e}"}},Some(Ok(rows))=>rsx!{
            for a in rows{article{class:"phase4_attachment",key:"{a.id}",div{strong{"{a.filename}"}small{"{a.file_size} bytes / {a.uploaded_by}"}if !a.stored{small{"Main POS local file"}}}
                if a.stored{button{disabled:busy(),onclick:{let db=db_form.clone();let actor=actor.clone();let id=a.id;move |_|{let db=db.clone();let actor=actor.clone();busy.set(true);spawn(async move{let result=async{let pool=pos_core::connect(&db.database_config()?).await?;let(name,bytes)=core::download(&pool,&actor,id).await?;if let Some(f)=rfd::AsyncFileDialog::new().set_file_name(&name).save_file().await{tokio::fs::write(f.path(),bytes).await?;}Ok::<_,anyhow::Error>(())}.await;busy.set(false);if let Err(e)=result{error.set(format!("{e:#}"));}});}},ActionLabel{label:"Download"}}
                    button{disabled:busy(),onclick:{let i=a.id;move |_|op.set(Some(command(Action::RemoveAttachment{i})))},ActionLabel{label:"Delete"}}
                }
            }}if rows.is_empty(){p{"No attachments."}}
        }}
    }}
    if let Some(c)=op(){Operation{db_form,actor,command:c,on_done:move |_|{op.set(None);refresh+=1;}}}
    }
}
