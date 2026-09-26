use crate::{icons::ActionLabel, DbForm};
use dioxus::prelude::*;
use pos_core::{
    auth::Session,
    category_groups::{self as groups, Group},
};

#[component]
pub fn ActivityPage(db_form: DbForm, actor: Session) -> Element {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut from = use_signal(|| today.clone());
    let mut to = use_signal(|| today.clone());
    let mut query = use_signal(String::new);
    let mut applied = use_signal(|| (today.clone(), today.clone(), String::new()));
    let mut page = use_signal(|| 0i64);
    let mut data = use_resource(move || {
        let db = db_form.clone();
        let actor = actor.clone();
        let (f, t, q) = applied();
        let p = page();
        async move {
            async {
                pos_core::activity::list(
                    &pos_core::connect(&db.database_config()?).await?,
                    &actor,
                    &f,
                    &t,
                    &q,
                    p,
                )
                .await
            }
            .await
            .map_err(|e: anyhow::Error| format!("{e:#}"))
        }
    });
    rsx! {section{class:"customers_page phase6_page",
        header{class:"customers_header",h2{ActionLabel{label:"Activity Log"}}button{hidden:true,"data-page-refresh":"true",disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}}
        div{class:"phase6_filters",
            label{"From" input{r#type:"date",value:"{from}",oninput:move|e|from.set(e.value())}}
            label{"To" input{r#type:"date",value:"{to}",oninput:move|e|to.set(e.value())}}
            label{"Search" input{r#type:"search",placeholder:"User, action or details",value:"{query}",oninput:move|e|query.set(e.value())}}
            button{disabled:!data.finished(),onclick:move |_|{page.set(0);applied.set((from(),to(),query()));data.restart();},"Apply"}
        }
        match data.read().as_ref(){None=>rsx!{p{"Loading..."}},Some(Err(e))=>rsx!{p{role:"alert","{e}"}},Some(Ok(rows))=>rsx!{
            div{class:"customers_table_scroll",table{class:"customers_table",thead{tr{th{"Date"}th{"User"}th{"Action"}th{"Details"}th{"IP address"}}}tbody{
                for r in rows.iter().take(50){tr{key:"{r.id}",td{"{r.created_at}"}td{"{r.username}"}td{"{r.action}"}td{class:"phase6_details","{r.details}"}td{"{r.ip_address}"}}}
            }}}
            if rows.is_empty(){p{"No activity found."}}
            footer{class:"customers_actions",button{disabled:page()==0,onclick:move |_|page-=1,"Previous"}span{"Page {page()+1}"}button{disabled:rows.len()<=50,onclick:move |_|page+=1,"Next"}}
        }}
    }}
}

#[component]
pub fn GroupsPage(db_form: DbForm) -> Element {
    let session = use_context::<Signal<Option<Session>>>();
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let db = source.clone();
        async move {
            async {
                let actor = session().ok_or_else(|| anyhow::anyhow!("Sign in again"))?;
                groups::list(&pos_core::connect(&db.database_config()?).await?, &actor).await
            }
            .await
            .map_err(|e: anyhow::Error| format!("{e:#}"))
        }
    });
    let mut editor = use_signal(|| None::<(Group, bool)>);
    let mut query = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! {section{class:"customers_page phase6_page",
        header{class:"customers_header",h2{ActionLabel{label:"Category groups"}}div{class:"customers_actions",
            button{hidden:true,"data-page-refresh":"true",disabled:busy()||!data.finished(),onclick:move |_|data.restart(),"Refresh"}
            button{class:"customer_primary",disabled:busy(),onclick:{let db=db_form.clone();move |_|{let db=db.clone();busy.set(true);spawn(async move{
                let result=async{let actor=session().ok_or_else(||anyhow::anyhow!("Sign in again"))?;groups::reserve(&pos_core::connect(&db.database_config()?).await?,&actor).await}.await;
                busy.set(false);match result{Ok(g)=>editor.set(Some((g,true))),Err(e)=>error.set(format!("{e:#}"))}
            });}},ActionLabel{label:"Add group"}}
        }}
        div{class:"phase6_filters",label{"Search" input{r#type:"search",value:"{query}",oninput:move|e|query.set(e.value())}}}
        if !error().is_empty(){p{role:"alert","{error}"}}
        match data.read().as_ref(){None=>rsx!{p{"Loading..."}},Some(Err(e))=>rsx!{p{role:"alert","{e}"}},Some(Ok(rows))=>rsx!{
            div{class:"customers_table_scroll",table{class:"customers_table",thead{tr{th{"Group"}th{"Description"}th{"Sort order"}th{"Favorite"}th{"Status"}th{""}}}tbody{
                for g in rows.iter().filter(|g|g.name.to_lowercase().contains(&query().to_lowercase())){tr{key:"{g.id}",
                    td{span{class:"group_swatch",style:"background:{g.color}"}"{g.name}"}td{"{g.description}"}td{"{g.sort_order}"}td{if g.is_favorite==1{"Yes"}else{"No"}}td{if g.is_active==1{"Active"}else{"Inactive"}}
                    td{button{onclick:{let g=g.clone();move |_|editor.set(Some((g.clone(),false)))},ActionLabel{label:"Edit"}}}
                }}
            }}}
            if rows.is_empty(){p{"No category groups."}}
        }}
        if let Some((group,new))=editor(){GroupEditor{db_form:db_form.clone(),group,new,onclose:move |_|editor.set(None),onsaved:move |_|{editor.set(None);data.restart();}}}
    }}
}

#[component]
fn GroupEditor(
    db_form: DbForm,
    group: Group,
    new: bool,
    onclose: EventHandler<()>,
    onsaved: EventHandler<()>,
) -> Element {
    let session = use_context::<Signal<Option<Session>>>();
    let mut form = use_signal(|| group.clone());
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! {div{class:"modal_backdrop",section{class:"customer_dialog phase6_dialog",role:"dialog",aria_modal:"true",aria_label:"Category group",
        h2{if new{"Add group"}else{"Edit group"}}
        div{class:"customer_form",
            label{"Name" input{value:form().name,disabled:busy(),oninput:move|e|form.write().name=e.value()}}
            label{"Sort order" input{r#type:"number",min:0,value:"{form().sort_order}",disabled:busy(),oninput:move|e|{if let Ok(v)=e.value().parse(){form.write().sort_order=v;}}}}
            label{"Description" textarea{value:form().description,disabled:busy(),oninput:move|e|form.write().description=e.value()}}
            label{"Color" input{r#type:"color",value:form().color,disabled:busy(),oninput:move|e|form.write().color=e.value()}}
            label{class:"phase6_check",input{r#type:"checkbox",checked:form().is_active==1,disabled:busy(),onchange:move|e|form.write().is_active=i32::from(e.checked())}"Active"}
            label{class:"phase6_check",input{r#type:"checkbox",checked:form().is_favorite==1,disabled:busy(),onchange:move|e|form.write().is_favorite=i32::from(e.checked())}"Favorite"}
        }
        if !error().is_empty(){p{role:"alert","{error}"}}
        div{class:"customers_actions",button{disabled:busy(),onclick:move |_|onclose.call(()),ActionLabel{label:"Cancel"}}
            button{class:"customer_primary",disabled:busy(),onclick:move |_|{
                if busy(){return}let db=db_form.clone();let old=(!new).then(||group.clone());let mut next=form();next.name=next.name.trim().into();form.set(next.clone());busy.set(true);
                spawn(async move{let result=async{let actor=session().ok_or_else(||anyhow::anyhow!("Sign in again"))?;groups::save(&pos_core::connect(&db.database_config()?).await?,&actor,old.as_ref(),&next).await}.await;
                    busy.set(false);match result{Ok(())=>onsaved.call(()),Err(e)=>error.set(format!("{e:#}"))}
                });
            },ActionLabel{label:"Save"}}
        }
    }}}
}
