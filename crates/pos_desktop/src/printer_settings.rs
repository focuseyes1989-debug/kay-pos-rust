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

/// Opens the selected driver's per-user Windows preferences without a shell.
async fn printing_preferences(printer: String) -> Result<(), String> {
    if printer.trim().is_empty() { return Err("Select a Windows printer first.".into()); }
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let output = std::process::Command::new("rundll32.exe")
                .args(["printui.dll,PrintUIEntry", "/e", "/n", &printer])
                .creation_flags(0x08000000)
                .output().map_err(|e| format!("Could not open Printing Preferences: {e}"))?;
            if !output.status.success() {
                return Err(format!("Printing Preferences failed ({}): {}", output.status, String::from_utf8_lossy(&output.stderr).trim()));
            }
            Ok(())
        }
        #[cfg(not(windows))]
        { let _ = printer; Err("Printing Preferences requires Windows.".into()) }
    }).await.map_err(|e| e.to_string())?
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
    let mut barcode_selected = use_signal(|| settings.get("barcode_printer_name").cloned().unwrap_or_default());
    let mut saving = use_signal(|| false);
    let mut preferences_open = use_signal(|| false);
    let paper_size=crate::receipt_editor::Geometry::from(&settings).paper_mm.to_string();
    let mut paper = use_signal(|| paper_size.clone());
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
    let open_preferences = EventHandler::new(move |printer: String| {
        if preferences_open() { return; }
        preferences_open.set(true);notice.set(String::new());
        spawn(async move {
            let result = printing_preferences(printer).await;
            preferences_open.set(false);
            if let Err(error) = result { failed.set(true);notice.set(error); }
        });
    });
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
                option {value:"112","112 mm roll"}
                if !["58","80","112"].contains(&paper().as_str()){option {value:paper(),"{paper} mm custom"}}
            }
          }
          button {r#type:"button",disabled:saving()||preferences_open()||!printers.finished()||selected().is_empty()||!names.contains(&selected()),onclick:move |_|open_preferences.call(selected()), crate::icons::ActionLabel { label:"Printing Preferences" } }
          button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:saving()||!printers.finished(),onclick:move |_|printers.restart(), crate::icons::ActionLabel { label:"Refresh Printers" } }
        }
        p {role:"status",if !printers.finished(){"Loading printers..."}else if let Some(Err(e))=state.as_ref(){"{e}"}else{"{names.len()} Windows printer(s) available."}}
        div {class:"printer_auto_options",
        label {class:"tax_discount_toggle",input {r#type:"checkbox",checked:auto_print(),disabled:saving(),onchange:move |e|auto_print.set(e.checked())} span {"Auto-print receipt"}}
        label {class:"tax_discount_toggle",input {r#type:"checkbox",checked:auto_drawer(),disabled:saving(),onchange:move |e|auto_drawer.set(e.checked())} span {"Auto-open drawer"}}
        }
        }
        fieldset {
          legend {"Barcode Printer"}
          div {class:"printer_settings_row barcode_printer_settings_row",
            div {class:"printer_settings_field",
              label {r#for:"barcode_printer","Windows Printer"}
              select {id:"barcode_printer",value:"{barcode_selected}",disabled:saving()||!printers.finished(),onchange:move |e|{barcode_selected.set(e.value());notice.set(String::new());},
                option {value:"",selected:barcode_selected().is_empty(),"Select printer"}
                if !barcode_selected().is_empty()&&!names.contains(&barcode_selected()){option {value:"{barcode_selected}",selected:true,"{barcode_selected} (unavailable)"}}
                for name in &names {option {value:"{name}",selected:barcode_selected()==*name,"{name}"}}
              }
            }
            button {r#type:"button",disabled:saving()||preferences_open()||!printers.finished()||barcode_selected().is_empty()||!names.contains(&barcode_selected()),onclick:move |_|open_preferences.call(barcode_selected()), crate::icons::ActionLabel { label:"Printing Preferences" } }
          }
        }
        }
        div {class:"settings_page_actions",button {class:"settings_save",disabled:saving()||!printers.finished()||(!selected().is_empty()&&!names.contains(&selected()))||(!barcode_selected().is_empty()&&!names.contains(&barcode_selected())),onclick:move |_|{
            if saving(){return;}
            if (auto_print() || auto_drawer()) && selected().trim().is_empty() { failed.set(true);notice.set("Select a Windows printer before enabling automatic printing or drawer opening.".into());return; }
            saving.set(true);let mut values=vec![("barcode_printer_name".to_string(),barcode_selected()),("receipt_printer_name".to_string(),selected()),("receipt_paper_size".into(),paper()),("print_receipt_after_sale".into(),if auto_print(){"1"}else{"0"}.into()),("open_cash_drawer_after_sale".into(),if auto_drawer(){"1"}else{"0"}.into())];if paper()!=paper_size {let printable=match paper().as_str(){"58"=>"48","112"=>"104",_=>"72"};values.push(("receipt_printable_width".into(),printable.into()));}let source=db_form.clone();
            spawn(async move {let result=async {let pool=pos_core::connect(&source.database_config()?).await?;pos_core::db::save_settings(&pool,&values).await}.await;saving.set(false);match result {Ok(())=>{on_saved.call(values.into_iter().collect());failed.set(false);notice.set("Printer settings saved.".into());},Err(e)=>{failed.set(true);notice.set(format!("{e:#}"));}}});
        },if saving(){"Saving..."}else{"Save Printer"}}}
        if !notice().is_empty(){p {role:if failed(){"alert"}else{"status"},"{notice}"}}
    }}
}
