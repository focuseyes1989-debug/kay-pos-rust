use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{attendance_sync, connect};

#[component]
pub fn AttendanceSync(db_form: DbForm, on_synced: EventHandler<()>) -> Element {
    let session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let mut open = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut synced = use_signal(|| false);
    let mut selected = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut failed = use_signal(|| false);
    let source = db_form.clone();
    let mut devices = use_resource(move || {
        let show = open();
        let form = source.clone();
        let actor = session.read().clone();
        async move {
            if !show {
                return Ok(Vec::new());
            }
            async {
                let actor = actor.ok_or_else(|| anyhow::anyhow!("Sign in again"))?;
                attendance_sync::devices(&connect(&form.database_config()?).await?, &actor).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let state = devices.read();
    let rows = state
        .as_ref()
        .and_then(|v| v.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let chosen = selected()
        .parse::<i32>()
        .ok()
        .and_then(|id| rows.iter().find(|r| r.id == id));
    rsx! {
        button {class:"attendance_sync_trigger",onclick:move |_|{selected.set(String::new());notice.set(String::new());synced.set(false);open.set(true);},"Attendance Sync"}
        if open(){div {class:"modal_backdrop",section {class:"customer_dialog attendance_sync_dialog",role:"dialog",aria_modal:"true",aria_label:"Attendance Sync",
            h2 {"Attendance Sync"}
            label {r#for:"attendance_sync_device","Device"}
            select {id:"attendance_sync_device","data-touch-combo-skip":"1",disabled:busy()||!devices.finished(),value:selected(),onchange:move |e|{selected.set(e.value());notice.set(String::new());},
                option {value:"","Select device"}
                for device in &rows {option {value:"{device.id}","{device.device_no} · {device.name}"}}
            }
            if let Some(device)=chosen {p {"{device.mappings} active employee mapping(s)"}}
            if let Some(Err(e))=state.as_ref(){p {role:"alert","{e}"} button {disabled:busy(),onclick:move |_|devices.restart(),"Retry"}}
            if !devices.finished(){p {role:"status","Loading devices..."}}
            else if matches!(state.as_ref(),Some(Ok(_)))&&rows.is_empty(){p {"No active devices"}}
            if chosen.is_some_and(|d|d.mappings==0){p {role:"alert","Configure employee mappings for this device in Main POS first."}}
            if busy(){div {class:"attendance_sync_progress",role:"status",span {class:"loading_spinner"} "Reading device and importing attendance..."}}
            if !notice().is_empty(){p {role:if failed(){"alert"}else{"status"},"{notice}"}}
            div {class:"employee_editor_actions",
                button {disabled:busy(),onclick:move |_|{open.set(false);if synced(){on_synced.call(());}},crate::icons::ActionLabel { label:"Close" }}
                button {class:"customer_primary",disabled:busy()||!devices.finished()||chosen.is_none_or(|d|d.mappings==0),onclick:move |_|{
                    if busy(){return;}
                    let Some(actor)=session.read().clone()else{return;};
                    let Ok(id)=selected().parse::<i32>()else{return;};
                    let form=db_form.clone();busy.set(true);notice.set(String::new());failed.set(false);
                    spawn(async move {
                        let result=async {attendance_sync::sync(&connect(&form.database_config()?).await?,&actor,id).await}.await;
                        busy.set(false);
                        match result {Ok(report)=>{notice.set(report.message());synced.set(true);},Err(e)=>{failed.set(true);notice.set(format!("Sync failed: {e:#}"));}}
                    });
                },if busy(){"Syncing..."}else{crate::icons::ActionLabel { label:"Sync Attendance" }}}
            }
        }}}
    }
}
