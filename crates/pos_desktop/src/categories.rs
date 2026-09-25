use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{
    categories::{self, CategoryRecord},
    connect,
};

fn ordered(rows: &[CategoryRecord]) -> Vec<(CategoryRecord, usize, String)> {
    fn visit(
        rows: &[CategoryRecord],
        row: &CategoryRecord,
        depth: usize,
        path: String,
        seen: &mut Vec<i32>,
        out: &mut Vec<(CategoryRecord, usize, String)>,
    ) {
        if seen.contains(&row.id) {
            return;
        }
        seen.push(row.id);
        out.push((row.clone(), depth, path.clone()));
        let path = format!("{path} {}", row.name);
        for child in rows.iter().filter(|c| c.parent_id == Some(row.id)) {
            visit(rows, child, depth + 1, path.clone(), seen, out);
        }
    }
    let mut out = vec![];
    let mut seen = vec![];
    for row in rows.iter().filter(|r| r.parent_id.is_none()) {
        visit(rows, row, 0, String::new(), &mut seen, &mut out);
    }
    for row in rows {
        visit(rows, row, 0, String::new(), &mut seen, &mut out);
    }
    out
}

#[component]
pub fn CategoriesPage(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        async move {
            async { categories::list(&connect(&source.database_config()?).await?).await }
                .await
                .map_err(|e| format!("{e:#}"))
        }
    });
    let mut search = use_signal(String::new);
    let mut editor = use_signal(|| None::<CategoryRecord>);
    let mut deletion = use_signal(|| None::<CategoryRecord>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let state = data.read();
    let rows = state
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let parents = rows.iter().filter(|r| r.parent_id.is_none()).count();
    let query = search().trim().to_lowercase();
    let visible = ordered(&rows)
        .into_iter()
        .filter(|(r, _, path)| format!("{} {path}", r.name).to_lowercase().contains(&query))
        .collect::<Vec<_>>();
    rsx! {
        section {class:"customers_page products_touch categories_touch",
            header {class:"customers_header customers_touch_header",h2 {crate::icons::ActionLabel { label:"Categories" }}
                div {class:"customers_actions",
                    button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}
                }
            }
            section {class:"products_items",
                header {class:"products_items_header",div {class:"category_heading",h3 {crate::icons::ActionLabel { label:"Categories" }}
                    if data.finished() && matches!(state.as_ref(),Some(Ok(_))) {
                        small {class:"category_counts","{rows.len()} categories · {parents} parents" if !query.is_empty() {" · {visible.len()} matches"}}
                    }
                }
                    button {class:"customer_primary",disabled:!data.finished(),onclick:move |_|editor.set(Some(CategoryRecord {status:"active".into(),..Default::default()})),crate::icons::ActionLabel { label:"+ Add category" }}
                }
                div {class:"category_manage_search",input {aria_label:"Search categories",placeholder:"Search categories",value:"{search}",oninput:move |e|search.set(e.value())}
                    button {disabled:search().is_empty(),onclick:move |_|search.set(String::new()),"Clear search"}
                }
                div {class:"products_manage_list",
                    if !data.finished() {div {class:"product_loading",role:"status",span {class:"loading_spinner"} strong {"Loading..."}}}
                    else if let Some(Err(message))=state.as_ref() {p {role:"alert","{message}"}}
                    else if visible.is_empty() {p {"No categories found."}}
                    else {for (row,depth,path) in visible {
                        article {key:"{row.id}",class:match depth {0=>"category_row category_parent",1=>"category_row category_child",_=>"category_row category_subchild"},style:"margin-left:{depth.min(5)*20}px",
                            div {class:"category_identity",div {strong {"{row.name}"} span {class:"category_level",match depth {0=>"Parent",1=>"Child",_=>"Sub-child"}} span {class:"category_status","{row.status}"}}
                                small {"{row.products} products · {row.children} child categories" if !path.trim().is_empty(){" · Under {path}"}}
                            }
                            div {class:"customers_actions",
                                button {onclick:move |_|editor.set(Some(CategoryRecord {parent_id:Some(row.id),status:"active".into(),..Default::default()})),"+ Child"}
                                button {disabled:row.is_system!=0,onclick:{let row=row.clone();move |_|editor.set(Some(row.clone()))},crate::icons::ActionLabel { label:"Edit" }}
                                button {class:"product_delete",disabled:row.is_system!=0,onclick:move |_|deletion.set(Some(row.clone())),crate::icons::ActionLabel { label:"Delete" }}
                            }
                        }
                    }}
                }
            }
        }
        if let Some(record)=editor() {CategoryEditor {record,rows,db_form:db_form.clone(),on_close:move |_|editor.set(None),on_saved:move |_|{editor.set(None);data.restart();},on_error:move |e|error.set(e)}}
        if let Some(record)=deletion() {
            div {class:"modal_backdrop",section {class:"customer_dialog",role:"dialog",aria_modal:"true",aria_label:"Delete category",h2 {"Delete category?"} p {"{record.name}"}
                div {class:"customers_actions",button {disabled:busy(),onclick:move |_|deletion.set(None),crate::icons::ActionLabel { label:"Cancel" }}
                    button {class:"product_delete",disabled:busy(),onclick:move |_|{if busy(){return;}busy.set(true);let source=db_form.clone();spawn(async move {let result=async {categories::delete(&connect(&source.database_config()?).await?,record.id).await}.await;busy.set(false);match result {Ok(())=>{deletion.set(None);data.restart();},Err(e)=>error.set(format!("{e:#}"))}});},crate::icons::ActionLabel { label:"Delete" }}
                }
            }}
        }
        if !error().is_empty() {MessageBox {title:"Category Error".to_string(),message:error(),on_close:move |_|error.set(String::new())}}
    }
}

#[component]
fn CategoryEditor(
    record: CategoryRecord,
    rows: Vec<CategoryRecord>,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut form = use_signal(|| record);
    let mut busy = use_signal(|| false);
    rsx! {div {class:"modal_backdrop",section {class:"customer_dialog",role:"dialog",aria_modal:"true",aria_label:"Category",
        h2 {if form().id==0 {"Add category"}else{"Edit category"}}
        div {class:"customer_form",
            label {"Name" input {value:form().name,disabled:busy(),oninput:move |e|form.write().name=e.value()}}
            label {"Parent category" select {value:form().parent_id.map(|id|id.to_string()).unwrap_or_default(),disabled:busy(),onchange:move |e|form.write().parent_id=e.value().parse().ok(),
                option {value:"",selected:form().parent_id.is_none(),"No parent"}
                for row in rows.iter().filter(|r|r.id!=form().id) {option {value:"{row.id}",selected:form().parent_id==Some(row.id),"{row.name}"}}
            }}
            label {"Description" textarea {value:form().description,disabled:busy(),oninput:move |e|form.write().description=e.value()}}
            label {"Status" select {value:form().status,disabled:busy(),onchange:move |e|form.write().status=e.value(),for status in ["active","inactive"] {option {value:status,selected:form().status==status,"{status}"}}}}
        }
        div {class:"customers_actions",
            button {disabled:busy(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel { label:"Cancel" }}
            button {class:"customer_primary",disabled:busy(),onclick:move |_|{if busy(){return;}let record=form();let source=db_form.clone();busy.set(true);spawn(async move {let result=async {categories::save(&connect(&source.database_config()?).await?,&record).await}.await;busy.set(false);match result {Ok(())=>on_saved.call(()),Err(e)=>on_error.call(format!("{e:#}"))}});},if busy(){"Saving..."}else{"Save category"}}
        }
    }}}
}
