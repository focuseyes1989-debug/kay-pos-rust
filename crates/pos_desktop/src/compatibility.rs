use crate::DbForm;
use dioxus::prelude::*;

#[component]
pub fn CompatibilityPanel(db_form: DbForm) -> Element {
    let actor = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let mut revision = use_signal(|| 0u32);
    let mut result = use_resource(move || {
        let source = db_form.clone();
        let user = actor();
        let run = revision();
        async move {
            if run == 0 {
                return Ok(vec![]);
            }
            async {
                let user = user.ok_or_else(|| anyhow::anyhow!("Sign in again"))?;
                let pool = pos_core::connect(&source.database_config()?).await?;
                pos_core::compatibility::load(&pool, &user).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    rsx! {section {class:"compatibility_panel",
        h3 {"Database compatibility"}
        button {disabled:!result.finished(),onclick:move |_|{revision+=1;result.restart();},crate::icons::ActionLabel{label:"Check compatibility"}}
        if !result.finished() {p {role:"status","Checking..."}}
        else if let Some(Err(error))=result.read().as_ref() {p {role:"alert","{error}"}}
        else if let Some(Ok(checks))=result.read().as_ref() {
            if !checks.is_empty() {
                p {role:"status","{checks.iter().filter(|c|!c.ok).count()} checks need attention"}
                div {class:"reports_table_scroll",table {class:"customers_table",thead {tr {th {"Module"} th {"Object"} th {"Status"} th {"Details"}}} tbody {
                    for c in checks {tr {td {"{c.area}"} td {"{c.object}"} td {if c.ok {"OK"}else{"Action required"}} td {"{c.detail}"}}}
                }}}
            }
        }
    }}
}
