use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{
    connect,
    zkteco::{self, Device},
};
use serde_json::{json, Value};

fn draft(d: &Device) -> Value {
    json!({"device_no":d.device_no.to_string(),"name":d.name,"ip_address":d.ip_address,"port":d.port.to_string(),"comm_key":d.comm_key.to_string(),"active":d.is_active==1})
}
fn parse(v: &Value, id: i32) -> anyhow::Result<Device> {
    use anyhow::Context;
    let text = |key: &str| v[key].as_str().unwrap_or("").trim().to_string();
    let d = Device {
        id,
        device_no: text("device_no")
            .parse()
            .context("Device ID must be a positive whole number")?,
        name: text("name"),
        ip_address: text("ip_address"),
        port: text("port").parse().context("Enter a TCP port")?,
        comm_key: text("comm_key")
            .parse()
            .context("Comm Key must be a non-negative whole number")?,
        is_active: i32::from(v["active"].as_bool().unwrap_or(false)),
        ..Device::default()
    };
    zkteco::validate(&d)?;
    Ok(d)
}
#[component]
pub fn ZktecoSettings(db_form: DbForm) -> Element {
    let session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let form = source.clone();
        let actor = session.read().clone();
        async move {
            async {
                let actor = actor.ok_or_else(|| anyhow::anyhow!("Sign in again"))?;
                zkteco::list(&connect(&form.database_config()?).await?, &actor).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let mut original = use_signal(|| None::<Device>);
    let mut values = use_signal(|| draft(&Device::default()));
    let mut busy = use_signal(|| false);
    let mut notice = use_signal(String::new);
    let mut failed = use_signal(|| false);
    let state = data.read();
    let rows = state
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let selected = original.read().as_ref().map(|d| d.id);
    rsx! {section {class:"zkteco_settings",
        crate::attendance_sync::AttendanceSync {db_form:db_form.clone(),on_synced:move |_|data.restart()}
        button {hidden:true,"data-page-refresh":"true",disabled:busy()||!data.finished(),onclick:move |_|data.restart()}
        fieldset {class:"zkteco_configuration",disabled:busy(),legend {"Device Configuration"}
            div {class:"zkteco_fields",
                for (key,label,kind) in [("device_no","Device ID","number"),("name","Name","text"),("ip_address","IP address","text"),("port","TCP port","number"),("comm_key","Comm Key","password")] {
                    label {r#for:"zk_{key}","{label}"}
                    input {id:"zk_{key}",r#type:kind,autocomplete:"off",value:values.read()[key].as_str().unwrap_or(""),oninput:move |e|values.write()[key]=json!(e.value())}
                }
                span {"Status"} label {class:"zkteco_active",input {r#type:"checkbox",checked:values.read()["active"].as_bool().unwrap_or(false),onchange:move |e|values.write()["active"]=json!(e.checked())} "Active"}
            }
            div {class:"settings_page_actions",
                button {onclick:move |_|{original.set(None);values.set(draft(&Device::default()));notice.set(String::new());},"New"}
                for (action,label) in [("test","Test TCP Connection"),("save","Save Device")] {
                    button {class:if action=="save"{"settings_save"}else{""},onclick:{let form=db_form.clone();move |_|{
                        if busy(){return;}
                        let Some(actor)=session.read().clone()else{return;};
                        let old=original.read().clone();
                        let device=match parse(&values.read(),selected.unwrap_or(0)){Ok(d)=>d,Err(e)=>{failed.set(true);notice.set(e.to_string());return;}};
                        let form=form.clone();busy.set(true);failed.set(false);notice.set(if action=="test"{"Testing TCP connection..."}else{"Saving device..."}.into());
                        spawn(async move {
                            let result: anyhow::Result<Option<Device>>=async {
                                let pool=connect(&form.database_config()?).await?;
                                if action=="test"{zkteco::test_tcp(&pool,&actor,&device).await?;Ok(None)}else{Ok(Some(zkteco::save(&pool,&actor,&device,old.as_ref()).await?))}
                            }.await;
                            busy.set(false);
                            match result {
                                Ok(Some(saved))=>{values.set(draft(&saved));original.set(Some(saved));notice.set("Device saved".into());data.restart();},
                                Ok(None)=>notice.set("TCP port is reachable. Device authentication, Comm Key and attendance sync have not been tested.".into()),
                                Err(e)=>{failed.set(true);notice.set(format!("{e:#}"));}
                            }
                        });
                    }},"{label}"}
                }
            }
        }
        if !notice().is_empty(){p {role:if failed(){"alert"}else{"status"},"{notice}"}}
        match state.as_ref(){None=>rsx!{p {role:"status","Loading devices..."}},Some(Err(e))=>rsx!{p {role:"alert","{e}"}},Some(Ok(_))=>rsx!{
            div {class:"zkteco_table_scroll",table {class:"employee_table",thead {tr {for label in ["Device ID","Name","IP","Port","Serial","Last Sync","Status"]{th {"{label}"}}}}
                tbody {for device in &rows {tr {key:"{device.id}",class:if selected==Some(device.id){"selected"}else{""},
                    td {button {class:"zkteco_select",disabled:busy(),aria_label:format!("Edit {}",device.name),onclick:{let d=device.clone();move |_|{values.set(draft(&d));original.set(Some(d.clone()));notice.set(String::new());}},"{device.device_no}"}}
                    td {"{device.name}"} td {"{device.ip_address}"} td {"{device.port}"} td {"{device.serial_no}"} td {"{device.last_sync_at}"} td {if device.is_active==1{"Active"}else{"Inactive"}}
                }}}
            }}
            if rows.is_empty(){p {"No devices found"}}
        }}
    }}
}
