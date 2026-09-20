use crate::DbForm;
use dioxus::prelude::*;
use std::collections::HashMap;

async fn list_printers() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(|| {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let output=std::process::Command::new("powershell.exe")
                .args(["-NoProfile","-NonInteractive","-Command","[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); $ErrorActionPreference='Stop'; ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Printer | Select-Object -ExpandProperty Name)"])
                .creation_flags(0x08000000).output().map_err(|e|e.to_string())?;
            if !output.status.success(){return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());}
            let mut printers:Vec<String>=serde_json::from_slice(&output.stdout).map_err(|e|format!("Printer list error: {e}"))?;
            printers.sort();printers.dedup();Ok(printers)
        }
        #[cfg(not(windows))]
        {Err("Windows printer discovery is unavailable on this platform".into())}
    }).await.map_err(|e|e.to_string())?
}

#[component]
pub fn PrinterSettings(
    settings: HashMap<String, String>,
    db_form: DbForm,
    on_saved: EventHandler<HashMap<String, String>>,
) -> Element {
    let mut printers = use_resource(list_printers);
    let mut selected = use_signal(|| {
        settings
            .get("receipt_printer_name")
            .cloned()
            .unwrap_or_default()
    });
    let mut saving = use_signal(|| false);
    let mut paper = use_signal(|| crate::receipt_layout::paper_mm(&settings).to_string());
    let mut auto_print = use_signal(|| {
        settings
            .get("print_receipt_after_sale")
            .is_some_and(|v| v == "1")
    });
    let mut auto_drawer = use_signal(|| {
        settings
            .get("open_cash_drawer_after_sale")
            .is_some_and(|v| v == "1")
    });
    let mut notice = use_signal(String::new);
    let mut failed = use_signal(|| false);
    let state = printers.read();
    let names = state
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    rsx! {section {class:"printer_settings",
        div {class:"tax_discount_form printer_preferences",
        fieldset {
        legend {"Receipt Printer"}
        div {class:"printer_settings_row",
          div {class:"printer_settings_field",
            label {r#for:"receipt_printer","Windows Printer"}
            select {id:"receipt_printer",value:"{selected}",disabled:saving()||!printers.finished(),onchange:move |e|{selected.set(e.value());notice.set(String::new());},
                option {value:"",selected:selected().is_empty(),"Select printer"}
                if !selected().is_empty()&&!names.contains(&selected()){option {value:"{selected}",selected:true,"{selected} (unavailable)"}}
                for name in &names {option {value:"{name}",selected:selected()==*name,"{name}"}}
            }
          }
          div {class:"printer_settings_field",
            label {r#for:"receipt_paper_size","Paper size"}
            select {id:"receipt_paper_size",value:paper(),disabled:saving(),onchange:move |e|{paper.set(e.value());notice.set(String::new());},
                option {value:"58","58 mm roll"}
                option {value:"80","80 mm roll"}
            }
          }
          button {disabled:saving()||!printers.finished(),onclick:move |_|printers.restart(),"Refresh Printers"}
        }
        p {role:"status",if !printers.finished(){"Loading printers..."}else if let Some(Err(e))=state.as_ref(){"{e}"}else{"{names.len()} Windows printer(s) available."}}
        label {class:"tax_discount_toggle",input {r#type:"checkbox",checked:auto_print(),disabled:saving(),onchange:move |e|auto_print.set(e.checked())} span {"Print receipt automatically after completing a sale"}}
        label {class:"tax_discount_toggle",input {r#type:"checkbox",checked:auto_drawer(),disabled:saving(),onchange:move |e|auto_drawer.set(e.checked())} span {"Open cash drawer automatically after completing a sale"}}
        }
        }
        div {class:"settings_page_actions",button {class:"settings_save",disabled:saving()||!printers.finished()||(!selected().is_empty()&&!names.contains(&selected())),onclick:move |_|{
            if saving(){return;}
            if (auto_print() || auto_drawer()) && selected().trim().is_empty() { failed.set(true);notice.set("Select a Windows printer before enabling automatic printing or drawer opening.".into());return; }
            saving.set(true);let values=vec![("receipt_printer_name".to_string(),selected()),("receipt_paper_size".into(),paper()),("print_receipt_after_sale".into(),if auto_print(){"1"}else{"0"}.into()),("open_cash_drawer_after_sale".into(),if auto_drawer(){"1"}else{"0"}.into())];let source=db_form.clone();
            spawn(async move {let result=async {let pool=pos_core::connect(&source.database_config()?).await?;pos_core::db::save_settings(&pool,&values).await}.await;saving.set(false);match result {Ok(())=>{on_saved.call(values.into_iter().collect());failed.set(false);notice.set("Printer setting saved.".into());},Err(e)=>{failed.set(true);notice.set(format!("{e:#}"));}}});
        },if saving(){"Saving..."}else{"Save Printer"}}}
        if !notice().is_empty(){p {role:if failed(){"alert"}else{"status"},"{notice}"}}
    }}
}
