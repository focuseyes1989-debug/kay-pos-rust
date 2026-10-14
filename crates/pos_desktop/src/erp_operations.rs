use crate::{icons::ActionLabel, DbForm};
use dioxus::prelude::*;
use pos_core::{
    auth::Session,
    erp_accounting::{self, AccountMapping},
    erp_operations::{self, OperationsData},
};

#[component]
pub fn ErpOperationsPage(db_form: DbForm, actor: Session) -> Element {
    let mut revision = use_signal(|| 0u64);
    let source = db_form.clone();
    let user = actor.clone();
    let mut data = use_resource(move || {
        let _ = revision();
        let form = source.clone();
        let actor = user.clone();
        async move {
            async {
                let pool = pos_core::connect(&form.database_config()?).await?;
                erp_operations::load(&pool, &actor).await
            }
            .await
            .map_err(|error: anyhow::Error| format!("{error:#}"))
        }
    });
    let busy = use_signal(|| None::<i64>);
    let mut notice = use_signal(String::new);
    let mut editor = use_signal(|| None::<AccountMapping>);

    rsx! { section { class: "erp_ops_page",
        header { class: "erp_ops_header",
            div { h2 { ActionLabel { label: "ERP Operations" } } small { "Outbound delivery, inbound sync and accounting controls" } }
            button { disabled: !data.finished(), onclick: move |_| data.restart(), ActionLabel { label: "Refresh" } }
        }
        if !notice().is_empty() { p { class: "erp_ops_notice", role: "status", "{notice}" } }
        match data.read().as_ref() {
            None => rsx! { p { "Loading ERP operations..." } },
            Some(Err(error)) => rsx! { p { role: "alert", "{error}" } },
            Some(Ok(state)) => render_state(state, &db_form, &actor, busy, notice, revision, editor)
        }
        if let Some(mapping) = editor() {
            MappingEditor { db_form: db_form.clone(), actor: actor.clone(), mapping,
                on_close: move |_| editor.set(None),
                on_saved: move |_| { editor.set(None); revision += 1; notice.set("Account mapping saved.".into()); }
            }
        }
    }}
}

fn render_state(
    state: &OperationsData,
    db_form: &DbForm,
    actor: &Session,
    mut busy: Signal<Option<i64>>,
    mut notice: Signal<String>,
    mut revision: Signal<u64>,
    mut editor: Signal<Option<AccountMapping>>,
) -> Element {
    let summary = &state.summary;
    let delivery_configured=matches!(pos_core::erp_sync::ErpSyncConfig::from_env(),Ok(Some(_)));
    rsx! {
        if !delivery_configured {
            p {class:"erp_ops_notice",role:"status",
                {crate::regional::text("ဤ client တွင် ERP delivery configuration မသတ်မှတ်ထားပါ သို့မဟုတ် မမှန်ပါ။ ERP API/Webhook URL ချိတ်ဆက်ပြီး app ပြန်ဖွင့်မှ delivery worker စတင်ပါမည်။ Pending သည် ERP ဆီ ပို့ရန်စောင့်နေသည့် မှတ်တမ်းဖြစ်ပြီး POS အရောင်းမသိမ်းရသေးဟု မဆိုလိုပါ။ အခြား client တွင် worker ဖွင့်ထားပါက ထို client ၏ status ကို အောက်တွင် စစ်ပါ။")}
            }
        }
        div { class: "erp_ops_metrics",
            Metric { label: "Pending", value: summary.pending.to_string(), tone: if summary.pending>0 {"warning"} else {"ok"} }
            Metric { label: "Processing", value: summary.processing.to_string(), tone: "info" }
            Metric { label: "Delivered", value: summary.sent.to_string(), tone: "ok" }
            Metric { label: "Dead", value: summary.dead.to_string(), tone: if summary.dead>0 {"danger"} else {"ok"} }
            Metric { label: "Mapped records", value: summary.mapped_entities.to_string(), tone: "info" }
            Metric { label: "Stock differences", value: summary.reconciliation_differences.to_string(), tone: if summary.reconciliation_differences>0 {"warning"} else {"ok"} }
        }
        section { class: "erp_ops_section",
            h3 { "Delivery status" }
            div { class: "erp_ops_kv",
                span { "Oldest queued" strong { "{summary.oldest_pending}" } }
                span { "Last delivered" strong { "{summary.last_sent}" } }
                span { "Latest stock snapshot" strong { "{summary.latest_reconciliation}" } }
                span { "Unmapped stock rows" strong { "{summary.reconciliation_unmapped}" } }
            }
        }
        section { class: "erp_ops_section",
            h3 { "Workers" }
            div { class: "erp_ops_table_wrap", table { class: "erp_ops_table",
                thead { tr { th { "Worker" } th { "Status" } th { "Last success" } th { "Updated" } th { "Last error" } } }
                tbody { for worker in &state.workers { tr { key: "{worker.worker_name}",
                    td { "{worker.worker_name}" } td { span { class: if worker.status=="healthy" {"erp_state_ok"} else {"erp_state_error"}, "{worker.status}" } }
                    td { "{worker.last_success}" } td { "{worker.updated_at}" } td { class: "erp_error_text", "{worker.last_error}" }
                }} }
            }}
            if state.workers.is_empty() { p { {crate::regional::text("ERP worker မှ status မပို့ရသေးပါ။ ERP connection configuration နှင့် worker ဖွင့်ထားသော client ကို စစ်ဆေးပါ။")} } }
        }
        section { class: "erp_ops_section",
            h3 { "Failed and delayed events" }
            div { class: "erp_ops_table_wrap", table { class: "erp_ops_table",
                thead { tr { th { "Event" } th { "State" } th { "Attempts" } th { "Next attempt" } th { "Error" } th { "" } } }
                tbody { for event in &state.failures { tr { key: "{event.id}",
                    td { strong { "{event.event_type}" } small { "{event.aggregate_type} #{event.aggregate_id}" } }
                    td { "{event.status}" } td { "{event.attempts}" } td { "{event.next_attempt}" } td { class: "erp_error_text", "{event.last_error}" }
                    td { button { disabled: busy().is_some(), onclick: {
                        let form=db_form.clone(); let actor=actor.clone(); let id=event.id;
                        move |_| { busy.set(Some(id)); notice.set(String::new()); let form=form.clone(); let actor=actor.clone(); spawn(async move {
                            let result=async { let pool=pos_core::connect(&form.database_config()?).await?; erp_operations::retry(&pool,&actor,id).await }.await;
                            busy.set(None); match result { Ok(())=>{notice.set(format!("Event #{id} queued for retry."));revision.set(revision()+1);},Err(error)=>notice.set(format!("Retry failed: {error:#}")) }
                        }); }
                    }, if busy()==Some(event.id) { "Retrying..." } else { "Retry" } } }
                }} }
            }}
            if state.failures.is_empty() { p { "No failed or delayed events." } }
        }
        section { class: "erp_ops_section",
            h3 { "Chart of Accounts mappings" }
            div { class: "erp_ops_table_wrap", table { class: "erp_ops_table",
                thead { tr { th { "Mapping" } th { "Account code" } th { "Account name" } th { "" } } }
                tbody { for mapping in &state.mappings { tr { key: "{mapping.mapping_key}",
                    td { "{mapping.mapping_key}" } td { "{mapping.account_code}" } td { "{mapping.account_name}" }
                    td { button { onclick: { let mapping=mapping.clone(); move |_| editor.set(Some(mapping.clone())) }, crate::icons::ActionLabel { label:"Edit" } } }
                }} }
            }}
        }
    }
}

#[component]
fn Metric(label: String, value: String, tone: String) -> Element {
    rsx! { div { class: "erp_ops_metric", "data-tone": tone, span { crate::icons::CardLabel {label} } strong { "{value}" } } }
}

#[component]
fn MappingEditor(db_form: DbForm, actor: Session, mapping: AccountMapping, on_close: EventHandler<()>, on_saved: EventHandler<()>) -> Element {
    let mut form = use_signal(|| mapping);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    rsx! { div { class: "modal_backdrop", section { class: "customer_dialog erp_mapping_dialog", role: "dialog", aria_modal: "true", aria_label: "ERP account mapping",
        h2 { "Account mapping" }
        div { class: "customer_form",
            label { "Mapping key" input { readonly: true, value: form().mapping_key } }
            label { "Account code" input { value: form().account_code, oninput: move |event| form.write().account_code=event.value() } }
            label { "Account name" input { value: form().account_name, oninput: move |event| form.write().account_name=event.value() } }
        }
        if !error().is_empty() { p { role: "alert", "{error}" } }
        footer { class: "customers_actions",
            button { disabled: busy(), onclick: move |_| on_close.call(()), crate::icons::ActionLabel { label:"Cancel" } }
            button { class: "customer_primary", disabled: busy(), onclick: move |_| { busy.set(true); error.set(String::new()); let form_value=form(); let db=db_form.clone(); let actor=actor.clone(); spawn(async move {
                let result=async { let pool=pos_core::connect(&db.database_config()?).await?; erp_accounting::save_mapping(&pool,&actor,&form_value).await }.await;
                busy.set(false); match result { Ok(())=>on_saved.call(()),Err(value)=>error.set(format!("{value:#}")) }
            }); }, if busy() { "Saving..." } else { "Save" } }
        }
    }} }
}
