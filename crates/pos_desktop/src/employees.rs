use crate::DbForm;
use base64::Engine;
use dioxus::prelude::*;
use pos_core::{
    connect,
    employees::{self, Command, Field, Page, Section},
};
use serde_json::{json, Value};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Pending {
    username: String,
    command: Command,
}
fn journal_path(form: &DbForm) -> anyhow::Result<std::path::PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow::anyhow!("Local application data unavailable"))?;
    let identity = format!(
        "{}:{}:{}",
        form.host.trim().to_lowercase(),
        form.port.trim(),
        form.database.trim()
    );
    Ok(std::path::PathBuf::from(root)
        .join("KAY POS Rust")
        .join("employee-pending")
        .join(format!(
            "{}.json",
            pos_core::auth::fingerprint(identity.as_bytes())
        )))
}
fn load_pending(form: &DbForm) -> anyhow::Result<Option<Pending>> {
    match std::fs::read(journal_path(form)?) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn save_pending(form: &DbForm, username: &str, command: &Command) -> anyhow::Result<()> {
    use std::io::Write;
    if let Some(existing) = load_pending(form)? {
        anyhow::ensure!(
            existing.username == username
                && serde_json::to_vec(&existing.command)? == serde_json::to_vec(command)?,
            "An unresolved employee request exists. Reopen Employees to recover it first"
        );
        return Ok(());
    }
    let path = journal_path(form)?;
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec(&Pending {
        username: username.into(),
        command: command.clone(),
    })?)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(())
}
fn clear_pending(form: &DbForm, command: &Command) -> anyhow::Result<()> {
    anyhow::ensure!(
        load_pending(form)?.is_some_and(|p| p.command.request_id == command.request_id),
        "Pending employee request changed"
    );
    std::fs::remove_file(journal_path(form)?)?;
    Ok(())
}

fn cell(v: &Value, key: &str) -> String {
    if matches!(
        key,
        "basic_salary"
            | "net_salary"
            | "amount"
            | "repaid_amount"
            | "balance"
            | "opening_cash"
            | "expected_cash"
            | "actual_cash"
            | "difference"
            | "sales_total"
            | "discount_total"
            | "target_amount"
            | "commission_amount"
    ) {
        return v[key].as_f64().map(crate::format_ks).unwrap_or_default();
    }
    if matches!(key, "is_overnight" | "active") {
        return if v[key].as_i64() == Some(1) {
            "Yes"
        } else {
            "No"
        }
        .into();
    }
    employees::text(v, key)
}
fn photo(row: &Value) -> Option<String> {
    let raw = row["photo_base64"].as_str()?;
    let clean = raw.split_whitespace().collect::<String>();
    if clean.is_empty() {
        return None;
    }
    let mime = if clean.starts_with("iVBOR") {
        "image/png"
    } else if clean.starts_with("/9j/") {
        "image/jpeg"
    } else {
        "image/webp"
    };
    Some(format!("data:{mime};base64,{clean}"))
}
fn action_fields(section: Section, action: &str) -> Vec<Field> {
    match action {
        "save" => section.fields().to_vec(),
        "pay" => vec![
            Field {
                key: "paid_date",
                label: "Paid date",
                kind: "date",
                default: "today",
            },
            Field {
                key: "payment_method",
                label: "Payment method",
                kind: "Cash|Bank Transfer|Mobile Payment",
                default: "Cash",
            },
        ],
        "repay" => vec![Field {
            key: "amount",
            label: "Repayment amount",
            kind: "number",
            default: "0",
        }],
        "close" => vec![Field {
            key: "actual_cash",
            label: "Actual cash",
            kind: "number",
            default: "0",
        }],
        "review" => vec![
            Field {
                key: "status",
                label: "Decision",
                kind: "Approved|Rejected|Cancelled",
                default: "Approved",
            },
            Field {
                key: "review_notes",
                label: "Review notes",
                kind: "memo",
                default: "",
            },
        ],
        _ => vec![],
    }
}
#[derive(Clone, PartialEq)]
struct Edit {
    section: Section,
    action: String,
    record: Value,
}

#[component]
pub fn EmployeesPage(db_form: DbForm) -> Element {
    let session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let mut section = use_signal(|| Section::Employees);
    let mut from = use_signal(|| chrono::Local::now().format("%Y-%m-01").to_string());
    let mut to = use_signal(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let mut range = use_signal(|| (from(), to()));
    let mut query = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut branch = use_signal(String::new);
    let mut department = use_signal(String::new);
    let mut position = use_signal(String::new);
    let mut selected = use_signal(|| None::<i64>);
    let mut edit = use_signal(|| None::<Edit>);
    let mut notice = use_signal(String::new);
    let mut exporting = use_signal(|| false);
    let recovery_form = db_form.clone();
    let mut recovery = use_signal(move || load_pending(&recovery_form).map_err(|e| e.to_string()));
    let mut recovering = use_signal(|| false);
    let mut page = use_signal(|| 0usize);
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let form = source.clone();
        let s = section();
        let dates = range();
        let actor = session.read().clone();
        async move {
            async {
                let actor = actor.ok_or_else(|| anyhow::anyhow!("Sign in again"))?;
                employees::list(
                    &connect(&form.database_config()?).await?,
                    &actor,
                    s,
                    &dates.0,
                    &dates.1,
                )
                .await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let state = data.read();
    let loaded = state.as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let perms = loaded
        .as_ref()
        .map(|p| p.permissions.clone())
        .unwrap_or_default();
    let can_manage =
        employees::has(&perms, &section().manage()) && matches!(&*recovery.read(), Ok(None));
    let term = query().trim().to_lowercase();
    let rows = loaded
        .as_ref()
        .map(|p| {
            p.rows
                .iter()
                .filter(|v| {
                    let matches = section()
                        .columns()
                        .iter()
                        .any(|(key, _)| employees::text(v, key).to_lowercase().contains(&term));
                    let stat = if section() == Section::Employees {
                        "employment_status"
                    } else {
                        "status"
                    };
                    matches
                        && (status().is_empty() || employees::text(v, stat) == status())
                        && (branch().is_empty() || employees::text(v, "branch") == branch())
                        && (department().is_empty()
                            || employees::text(v, "department") == department())
                        && (position().is_empty() || employees::text(v, "position") == position())
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let current = rows
        .iter()
        .find(|r| r["id"].as_i64() == selected())
        .cloned();
    let current_page = page().min(rows.len().saturating_sub(1) / 30);
    let options = |key: &str| {
        let mut values = loaded
            .as_ref()
            .map(|p| {
                p.rows
                    .iter()
                    .map(|v| employees::text(v, key))
                    .filter(|v| !v.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        values.sort();
        values.dedup();
        values
    };
    let status_options = options(if section() == Section::Employees {
        "employment_status"
    } else {
        "status"
    });
    rsx! {
        section {class:"employees_page",
            header {class:"employees_header",h2 {"Employee Management"} button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|{selected.set(None);data.restart();},"Refresh"}}
            match recovery.read().clone() {
                Err(e)=>rsx!{p {role:"alert","Employee recovery file could not be read: {e}. Resolve it before making changes."}},
                Ok(Some(pending))=>rsx!{div {role:"alert",p {"Unresolved employee request: sign in as {pending.username} to recover it before making changes."}
                    button {disabled:recovering(),onclick:{let form=db_form.clone();move |_|{let Some(actor)=session.read().clone()else{return;};if actor.username()!=pending.username {notice.set("Sign in as the original operator".into());return;}let form=form.clone();let cmd=pending.command.clone();recovering.set(true);spawn(async move{
                        let result=async{employees::execute(&connect(&form.database_config()?).await?,&actor,&cmd).await}.await;
                        recovering.set(false);
                        match result {Ok(_)=>match clear_pending(&form,&cmd){Ok(())=>{recovery.set(Ok(None));notice.set("Recovered successfully".into());data.restart();},Err(e)=>notice.set(e.to_string())},Err(e)=>{
                            notice.set(format!("{e:#}"));
                        }}
                    });}},"Retry Same Request"}
                }},
                _=>rsx!{}
            }
            if let Some(loaded)=loaded.as_ref(){div {class:"employee_summary",
                for (key,label) in [("active","Active Employees"),("pending_leave","Pending Leave"),("expiring","Documents Expiring"),("advances","Outstanding Advances")]{
                    if !loaded.summary[key].is_null(){div {span {"{label}"} strong {if key=="advances"{{crate::format_ks(loaded.summary[key].as_f64().unwrap_or_default())}}else{{employees::text(&loaded.summary,key)}}}}}
                }
            }}
            nav {class:"receipt_tabs employee_tabs",aria_label:"Employee sections",
                for tab in employees::SECTIONS {button {class:if section()==*tab || (section()==Section::Assignments&&*tab==Section::Shifts)||(section()==Section::Commission&&*tab==Section::Advances){"active"}else{""},onclick:{let tab=*tab;move |_|{section.set(tab);selected.set(None);page.set(0);status.set(String::new());query.set(String::new());branch.set(String::new());department.set(String::new());position.set(String::new());}},"{tab.title()}"}}
            }
            if matches!(section(),Section::Shifts|Section::Assignments|Section::Advances|Section::Commission){
                div {class:"receipt_tabs employee_subtabs",for tab in if matches!(section(),Section::Shifts|Section::Assignments){vec![Section::Shifts,Section::Assignments]}else{vec![Section::Advances,Section::Commission]}{
                    button {class:if section()==tab{"active"}else{""},onclick:move |_|{section.set(tab);selected.set(None);status.set(String::new());page.set(0);},"{tab.title()}"}
                }}
            }
            div {class:"employee_filters",
                input {r#type:"search",aria_label:"Search employees and records",placeholder:"Name, employee ID or details",value:"{query}",oninput:move |e|{query.set(e.value());page.set(0);}}
                if !status_options.is_empty(){select {aria_label:"Status",value:"{status}",onchange:move |e|{status.set(e.value());page.set(0);},option {value:"","All statuses"} for option in status_options {option {value:"{option}","{option}"}}}}
                if matches!(section(),Section::Employees|Section::Performance){select {aria_label:"Branch",value:"{branch}",onchange:move |e|{branch.set(e.value());page.set(0);},option {value:"","All branches"} for option in options("branch"){option {value:"{option}","{option}"}}}}
                if section()==Section::Employees {
                    select {aria_label:"Position",value:"{position}",onchange:move |e|{position.set(e.value());page.set(0);},option {value:"","All positions"} for option in options("position"){option {value:"{option}","{option}"}}}
                    select {aria_label:"Department",value:"{department}",onchange:move |e|{department.set(e.value());page.set(0);},option {value:"","All departments"} for option in options("department"){option {value:"{option}","{option}"}}}
                }
                if matches!(section(),Section::Attendance|Section::Payroll|Section::Leave|Section::Advances|Section::Performance|Section::Cash){
                    label {"From" input {r#type:"date",value:"{from}",oninput:move |e|from.set(e.value())}}
                    label {"To" input {r#type:"date",value:"{to}",oninput:move |e|to.set(e.value())}}
                    button {onclick:move |_|{range.set((from(),to()));page.set(0);selected.set(None);},"Apply"}
                    if section()==Section::Attendance {
                        for (preset,label) in [("today","Today"),("week","This week"),("month","This month")] {
                            button {aria_pressed:range()==pos_core::attendance_sync::date_range(chrono::Local::now().date_naive(),preset),onclick:move |_|{
                                let dates=pos_core::attendance_sync::date_range(chrono::Local::now().date_naive(),preset);
                                from.set(dates.0.clone());to.set(dates.1.clone());range.set(dates);page.set(0);selected.set(None);
                            },"{label}"}
                        }
                    }
                }
            }
            div {class:"employee_actions",
                if section()==Section::Attendance && ["settings","manage_attendance","manage_employees"].iter().all(|p|employees::has(&perms,p)) {
                    crate::attendance_sync::AttendanceSync {db_form:db_form.clone(),on_synced:move |_|data.restart()}
                }
                if can_manage && section()!=Section::Performance {
                    button {class:"customer_primary",onclick:move |_|edit.set(Some(Edit{section:section(),action:"save".into(),record:json!({})})),match section(){Section::Cash=>"Open Session",Section::Payroll=>"Create Payroll",Section::Leave=>"New Request",Section::Advances=>"Salary Advance",Section::Assignments=>"Assign Shift",Section::Documents=>"Add Document",Section::Attendance=>"Record Attendance",Section::Shifts=>"New Shift",Section::Commission=>"New Rule",_=>"Add Employee"}}
                    if section().editable(){button {disabled:current.is_none(),onclick:{let current=current.clone();move |_|if let Some(row)=&current{edit.set(Some(Edit{section:section(),action:"save".into(),record:row.clone()}));}},crate::icons::ActionLabel { label:"Edit" }}}
                    for (action,label) in match section(){Section::Payroll=>vec![("pay","Mark Paid")],Section::Leave=>vec![("review","Review Leave")],Section::Advances=>vec![("repay","Record Repayment")],Section::Cash=>vec![("close","Close Session")],Section::Assignments=>vec![("delete","Delete Assignment")],_=>vec![]} {
                        button {disabled:current.is_none() || current.as_ref().is_some_and(|v|match action{"pay"=>employees::text(v,"status")!="Draft","close"=>employees::text(v,"status")!="Open","repay"=>employees::text(v,"status")=="Repaid",_=>false}),onclick:{let current=current.clone();move |_|if let Some(row)=&current{edit.set(Some(Edit{section:section(),action:action.into(),record:row.clone()}));}},"{label}"}
                    }
                }
                button {disabled:exporting()||loaded.is_none(),onclick:{let rows=rows.clone();let s=section();move |_|{let rows=rows.clone();exporting.set(true);spawn(async move{if let Some(file)=rfd::AsyncFileDialog::new().set_file_name("employees.xlsx").add_filter("Excel",&["xlsx"]).save_file().await{match export(s,&rows,file.path()){Ok(())=>notice.set("Export saved".into()),Err(e)=>notice.set(format!("{e:#}"))}}exporting.set(false);});}},crate::icons::ActionLabel { label:"Export Excel" }}
                span {"{rows.len()} records"}
            }
            if !notice().is_empty(){p {role:"status","{notice}"}}
            match state.as_ref(){None=>rsx!{p {role:"status","Loading..."}},Some(Err(e))=>rsx!{p {role:"alert","{e}"}},Some(Ok(_))=>rsx!{
                div {class:"employee_table_scroll",table {class:"employee_table",
                    thead {tr {if section()==Section::Employees{th {"Photo"}} for (_,label) in section().columns(){th {"{label}"}}}}
                    tbody {for (row_key,row) in rows.iter().skip(current_page*30).take(30).enumerate(){
                        tr {key:"{row_key}",tabindex:0,aria_selected:row["id"].as_i64()==selected(),class:if row["id"].as_i64()==selected(){"selected"}else{""},onclick:{let id=row["id"].as_i64();move |_|selected.set(id)},onkeydown:{let id=row["id"].as_i64();move |e|if e.key()==Key::Enter||e.key()==Key::Character(" ".into()){e.prevent_default();selected.set(id);}},
                            if section()==Section::Employees{td {if let Some(url)=photo(row){img {src:"{url}",alt:"Employee photo",class:"employee_avatar"}}else{span {class:"employee_avatar employee_initial",{employees::text(row,"full_name").chars().next().unwrap_or('?').to_string()}}}}}
                            for (key,_) in section().columns(){td {title:cell(row,key),"{cell(row,key)}"}}
                        }
                    }}
                }}
                if rows.is_empty(){p {"No records found"}}
                footer {class:"employee_pagination",button {disabled:current_page==0,onclick:move |_|page.set(current_page.saturating_sub(1)),"Previous"} span {"Page {current_page+1} / {rows.len().div_ceil(30).max(1)}"} button {disabled:(current_page+1)*30>=rows.len(),onclick:move |_|page.set(current_page+1),"Next"}}
            }}
            if let Some(editing)=edit(){if let Some(loaded)=loaded.clone() {
                EmployeeEditor {editing,loaded,db_form:db_form.clone(),on_close:move |_|edit.set(None),on_saved:move |_|{edit.set(None);selected.set(None);notice.set("Saved".into());data.restart();}}
            }}
        }
    }
}

#[component]
fn EmployeeEditor(
    editing: Edit,
    loaded: Page,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let fields = action_fields(editing.section, &editing.action);
    let initial = editing.clone();
    let mut values = use_signal(move || {
        if initial.action == "save" && initial.record["id"].is_number() {
            initial.record.clone()
        } else if initial.action == "save" {
            employees::defaults(initial.section)
        } else {
            let mut v = json!({});
            for f in action_fields(initial.section, &initial.action) {
                v[f.key] = json!(if f.default == "today" {
                    chrono::Local::now().format("%Y-%m-%d").to_string()
                } else {
                    f.default.into()
                });
            }
            if initial.action == "review"
                && employees::text(&initial.record, "status") == "Approved"
            {
                v["status"] = json!("Cancelled");
            }
            v
        }
    });
    let mut pending = use_signal(|| None::<Command>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut choosing = use_signal(|| false);
    let frozen = busy() || pending.read().is_some();
    let title = match editing.action.as_str() {
        "save" => {
            if editing.record["id"].is_number() {
                format!("Edit {}", editing.section.title())
            } else {
                format!("New {}", editing.section.title())
            }
        }
        "pay" => "Confirm Payroll Payment".into(),
        "repay" => "Record Repayment".into(),
        "close" => "Close Cash Session".into(),
        "review" => "Review Leave".into(),
        _ => "Delete Shift Assignment".into(),
    };
    rsx! {
        div {class:"modal_backdrop",section {class:"customer_dialog employee_editor",role:"dialog",aria_modal:"true",aria_label:"{title}",
            header {h2 {"{title}"} if !employees::text(&editing.record,"full_name").is_empty(){p {{employees::text(&editing.record,"full_name")}}}}
            if editing.action=="delete"{p {"Delete this shift assignment? Attendance categories will be recalculated; manual corrections remain unchanged."}}
            if editing.action=="pay"{p {"Net salary: " {cell(&editing.record,"net_salary")}}}
            if editing.action=="repay"{p {"Outstanding: " {cell(&editing.record,"balance")}}}
            div {class:if editing.section==Section::Employees&&editing.action=="save"{"employee_editor_columns"}else{""},
                div {class:"employee_form_fields",for f in fields {
                    label {class:if f.kind=="memo"{"employee_full_field"}else{""},"{f.label}",
                        if f.kind=="memo" {textarea {disabled:frozen,value:employees::text(&values.read(),f.key),oninput:move |e|values.write()[f.key]=json!(e.value())}}
                        else if matches!(f.kind,"employee"|"user"|"shift") {
                            select {disabled:frozen,value:employees::text(&values.read(),f.key),onchange:move |e|values.write()[f.key]=json!(e.value()),option {value:"",if f.kind=="user"{"No POS account"}else{"Select"}}
                                for row in if f.kind=="employee"{&loaded.employees}else if f.kind=="user"{&loaded.users}else{&loaded.shifts}{option {value:employees::text(row,"id"),if f.kind=="employee"{{format!("{} · {}",employees::text(row,"employee_no"),employees::text(row,"full_name"))}}else if f.kind=="user"{{employees::text(row,"username")}}else{{employees::text(row,"name")}}}}
                            }
                        }else if f.kind.contains('|') {
                            select {disabled:frozen,value:employees::text(&values.read(),f.key),onchange:move |e|values.write()[f.key]=json!(e.value()),for value in f.kind.split('|'){option {value,"{value}"}}}
                        }else if f.kind=="bool" {
                            input {r#type:"checkbox",disabled:frozen,checked:employees::text(&values.read(),f.key)=="1",onchange:move |e|values.write()[f.key]=json!(if e.checked(){1}else{0})}
                        }else {
                            input {disabled:frozen,r#type:match f.kind{"date"|"optional-date"=>"date","time"|"optional-time"=>"time","month"=>"month","integer"|"number"=>"number",_=>"text"},min:"0",step:if f.kind=="integer"{"1"}else{"any"},value:employees::text(&values.read(),f.key),oninput:move |e|values.write()[f.key]=json!(e.value())}
                            if f.kind=="file"{button {disabled:frozen,onclick:move |_|{spawn(async move{if let Some(file)=rfd::AsyncFileDialog::new().pick_file().await{values.write()["file_path"]=json!(file.path().to_string_lossy());}});},"Browse"}}
                        }
                    }
                }}
                if editing.section==Section::Employees&&editing.action=="save" {
                    aside {class:"employee_photo_editor",if let Some(url)=photo(&values.read()){img {src:"{url}",alt:"Employee photo"}}else{div {class:"employee_photo_empty","No photo"}}
                        button {disabled:frozen||choosing(),onclick:move |_|{choosing.set(true);spawn(async move{
                            if let Some(file)=rfd::AsyncFileDialog::new().add_filter("Photo",&["png","jpg","jpeg","webp"]).pick_file().await{
                                let bytes=file.read().await;
                                let result=normalize_photo(&bytes);
                                match result{Ok(bytes)=>values.write()["photo_base64"]=json!(base64::engine::general_purpose::STANDARD.encode(bytes)),Err(e)=>error.set(format!("{e:#}"))}
                            }choosing.set(false);
                        });},"Choose Photo"}
                        button {disabled:frozen||choosing(),onclick:move |_|values.write()["photo_base64"]=json!(""),"Remove Photo"}
                    }
                }
            }
            if !error().is_empty(){p {role:"alert","{error}"}}
            footer {class:"employee_editor_actions",
                button {disabled:frozen||choosing(),onclick:move |_|on_close.call(()),crate::icons::ActionLabel { label:"Cancel" }}
                button {class:"customer_primary",disabled:busy()||choosing(),onclick:move |_|{
                    let Some(actor)=session.read().clone()else{return;};
                    let retrying=pending.read().is_some();
                    let cmd=if let Some(cmd)=pending.read().clone(){cmd}else{
                        if editing.action=="save"{if let Err(e)=employees::validate(editing.section,&values.read()){error.set(e.to_string());return;}}
                        Command{request_id:pos_core::auth::new_request_id(),section:editing.section,action:editing.action.clone(),id:editing.record["id"].as_i64().map(|n|n as i32),revision:employees::text(&editing.record,"revision"),values:values.read().clone()}
                    };
                    if let Err(e)=save_pending(&db_form,actor.username(),&cmd){error.set(format!("{e:#}"));return;}
                    pending.set(Some(cmd.clone()));busy.set(true);error.set(String::new());let source=db_form.clone();
                    spawn(async move{let result=async{employees::execute(&connect(&source.database_config()?).await?,&actor,&cmd).await}.await;busy.set(false);match result{Ok(_)=>{match clear_pending(&source,&cmd){Ok(())=>{pending.set(None);on_saved.call(());},Err(e)=>error.set(format!("Saved, but recovery cleanup failed: {e}. Retry the same request."))}},Err(e)=>{if !retrying && !employees::uncertain(&e){match clear_pending(&source,&cmd){Ok(())=>pending.set(None),Err(clear)=>{error.set(format!("{e:#}; recovery cleanup: {clear}"));return;}}}error.set(format!("{e:#}"));}}});
                },if busy(){"Saving..."}else if pending.read().is_some(){"Retry Same Request"}else{"Confirm"}}
            }
        }}
    }
}
fn normalize_photo(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(bytes.len() <= 12 * 1024 * 1024, "Photo must be below 12 MB");
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(12000);
    limits.max_image_height = Some(12000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.thumbnail(600, 600);
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}
fn export(section: Section, rows: &[Value], path: &std::path::Path) -> anyhow::Result<()> {
    let mut book = rust_xlsxwriter::Workbook::new();
    let sheet = book.add_worksheet();
    for (c, (_, label)) in section.columns().iter().enumerate() {
        sheet.write_string(0, c as u16, *label)?;
        sheet.set_column_width(c as u16, 22)?;
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, (key, _)) in section.columns().iter().enumerate() {
            if let Some(n) = row[key].as_f64() {
                sheet.write_number((r + 1) as u32, c as u16, n)?;
            } else {
                sheet.write_string((r + 1) as u32, c as u16, employees::text(row, key))?;
            }
        }
    }
    book.save(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_request_survives_reload_and_cannot_be_replaced() {
        let form = DbForm {
            database: format!("employee-journal-test-{}", pos_core::auth::new_request_id()),
            ..DbForm::default()
        };
        let command = Command {
            request_id: pos_core::auth::new_request_id(),
            section: Section::Advances,
            action: "save".into(),
            id: None,
            revision: String::new(),
            values: json!({"amount":100,"employee_id":1}),
        };
        assert!(load_pending(&form).unwrap().is_none());
        save_pending(&form, "admin", &command).unwrap();
        save_pending(&form, "admin", &command).unwrap();
        let recovered = load_pending(&form).unwrap().unwrap();
        assert_eq!(recovered.username, "admin");
        assert_eq!(
            serde_json::to_value(&recovered.command).unwrap(),
            serde_json::to_value(&command).unwrap()
        );
        let mut other = command.clone();
        other.request_id = pos_core::auth::new_request_id();
        assert!(save_pending(&form, "admin", &other).is_err());
        assert!(save_pending(&form, "someone-else", &command).is_err());
        assert!(clear_pending(&form, &other).is_err());
        clear_pending(&form, &command).unwrap();
        assert!(load_pending(&form).unwrap().is_none());
    }

    #[test]
    fn photo_normalization_bounds_dimensions_and_rejects_invalid_bytes() {
        assert!(normalize_photo(b"not an image").is_err());
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1200, 800)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let result = normalize_photo(bytes.get_ref()).unwrap();
        let decoded = image::load_from_memory(&result).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (600, 400));
    }
}
