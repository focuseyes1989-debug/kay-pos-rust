use crate::{icons::ActionLabel, DbForm};
use chrono::Datelike;
use dioxus::prelude::*;
use pos_core::{
    auth::{Permission, Session},
    reports::{Cell, Report},
};

#[component]
pub fn ReportsPage(db_form: DbForm, actor: Session) -> Element {
    let today = chrono::Local::now().date_naive();
    let mut from = use_signal(|| today.with_day(1).unwrap().to_string());
    let mut to = use_signal(|| today.to_string());
    let mut range = use_signal(|| (from(), to()));
    let mut tab = use_signal(|| 0usize);
    let mut page = use_signal(|| 0usize);
    let mut notice = use_signal(String::new);
    let mut exporting = use_signal(|| false);
    let source = db_form.clone();
    let user = actor.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        let user = user.clone();
        let (a, b) = range();
        async move {
            async {
                let pool = pos_core::connect(&source.database_config()?).await?;
                pos_core::reports::load(&pool, &user, &a, &b).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let loading = !data.finished();
    let snapshot = data.read();
    let report = if loading {
        None
    } else {
        snapshot.as_ref().and_then(|r| r.as_ref().ok())
    };
    let currency = crate::regional::current().symbol().to_string();
    let chosen = report.map(|r| r.tables[tab()].clone());
    let pages = chosen
        .as_ref()
        .map(|t| t.rows.len().div_ceil(50))
        .unwrap_or(1)
        .max(1);
    let current = page().min(pages - 1);
    rsx! {section {class:"customers_page reports_page",
        header {class:"reports_header",
            h2 {ActionLabel{label:"Reports"}}
            button {hidden:true,"data-page-refresh":"true",tabindex:-1,aria_hidden:"true",disabled:loading||exporting(),onclick:move |_|{notice.set(String::new());data.restart();},"Refresh"}
            button {disabled:loading||exporting()||report.is_none(),onclick:{
                let report=report.cloned(); let source=db_form.clone(); let user=actor.clone(); let currency=currency.clone();
                move |_| {
                    let Some(report)=report.clone() else{return;};
                    let source=source.clone();let user=user.clone();let currency=currency.clone();
                    exporting.set(true);notice.set(String::new());
                    spawn(async move {
                        if let Some(file)=rfd::AsyncFileDialog::new().add_filter("Excel",&["xlsx"]).set_file_name(format!("Reports-{}-{}.xlsx",report.from,report.to)).save_file().await {
                            let result=async {
                                let pool=pos_core::connect(&source.database_config()?).await?;
                                let mut connection=pool.acquire().await?;
                                user.authorize(&mut connection,Permission::Manage).await?;
                                export(&report,&currency,file.path())
                            }.await;
                            notice.set(match result {Ok(())=>"Export saved".into(),Err(e)=>format!("Export failed: {e:#}")});
                        }
                        exporting.set(false);
                    });
                }
            },ActionLabel{label:"Export Excel"}}
        }
        div {class:"dashboard_filters",
            label {"From" input {r#type:"date",value:from(),oninput:move |e|from.set(e.value())}}
            label {"To" input {r#type:"date",value:to(),oninput:move |e|to.set(e.value())}}
            button {class:"customer_primary",disabled:loading,onclick:move |_|{
                match pos_core::sale_summary::dates(&from(),&to()) {Ok(_)=>{notice.set(String::new());page.set(0);range.set((from(),to()));data.restart();},Err(_)=>notice.set("Enter a valid date range.".into())}
            },"Apply"}
            for (label,start) in [("Today",today),("This week",today-chrono::Duration::days(today.weekday().num_days_from_monday() as i64)),("This month",today.with_day(1).unwrap())] {
                button {disabled:loading,onclick:move |_|{from.set(start.to_string());to.set(today.to_string());range.set((start.to_string(),today.to_string()));page.set(0);notice.set(String::new());data.restart();},"{label}"}
            }
        }
        div {class:"receipt_tabs reports_tabs",role:"tablist",aria_label:"Reports",
            for (i,label) in ["Sales","Expenses","Profit & Loss","Financial Summary","Receivables","Payables"].iter().enumerate() {
                button {role:"tab",aria_selected:tab()==i,class:if tab()==i{"active"}else{""},onclick:move |_|{tab.set(i);page.set(0);},"{label}"}
            }
        }
        if !notice().is_empty() {p {role:"status","{notice}"}}
        if loading {p {role:"status","Loading reports..."}}
        else if let Some(Err(error))=snapshot.as_ref() {p {role:"alert","{error}"}}
        else if let (Some(report),Some(table))=(report,chosen.as_ref()) {
            div {class:"reports_metadata",span {"{report.from} / {report.to}"} span {"Snapshot: {report.as_of}"} span {"Currency: {currency}"}}
            if table.rows.is_empty() {p {"No records."}}
            div {class:"reports_table_scroll",role:"tabpanel",aria_label:"{table.title}",tabindex:0,
                table {class:"customers_table",thead {tr {for heading in &table.headers {th {scope:"col","{heading}"}}}}
                    tbody {for row in table.rows.iter().skip(current*50).take(50) {tr {for cell in row {td {class:if matches!(cell,Cell::Money(_)){"reports_money"}else{""},"{display(cell, &currency)}"}}}}}
                }
            }
            footer {class:"reports_pagination",
                span {"{table.rows.len()} records"}
                button {title:"Previous page",aria_label:"Previous page",disabled:current==0,onclick:move |_|page.set(current-1),crate::icons::Icon{name:"arrow_circle_left"}}
                span {"{current+1} / {pages}"}
                button {title:"Next page",aria_label:"Next page",disabled:current+1>=pages,onclick:move |_|page.set(current+1),crate::icons::Icon{name:"arrow_circle_right"}}
            }
            details {class:"reports_notes",summary {"Accounting notes"} for warning in &report.warnings {p {"{warning}"}}}
        }
    }}
}

fn display(cell: &Cell, currency: &str) -> String {
    if !matches!(cell, Cell::Money(_)) {
        return cell.plain();
    }
    let text = cell.plain();
    let (sign, text) = text
        .strip_prefix('-')
        .map_or(("", text.as_str()), |s| ("-", s));
    let (whole, fraction) = text
        .split_once('.')
        .map_or((text, None), |(a, b)| (a, Some(b)));
    let mut out = String::from(sign);
    for (i, c) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(f) = fraction {
        out.push('.');
        out.push_str(f);
    }
    format!("{out} {currency}")
}

pub(crate) fn export(report: &Report, currency: &str, path: &std::path::Path) -> anyhow::Result<()> {
    let mut book = rust_xlsxwriter::Workbook::new();
    let heading = rust_xlsxwriter::Format::new().set_bold();
    let money = rust_xlsxwriter::Format::new().set_num_format("#,##0.00");
    let notes = book.add_worksheet();
    notes.set_name("Scope")?;
    notes.set_column_width(0, 120)?;
    for (i, text) in [
        format!("Period: {} / {}", report.from, report.to),
        format!("Snapshot: {}", report.as_of),
        format!("Currency: {currency}"),
    ]
    .iter()
    .chain(report.warnings.iter())
    .enumerate()
    {
        notes.write_string(i as u32, 0, text)?;
    }
    for table in &report.tables {
        anyhow::ensure!(
            table.rows.len() <= 1_048_575,
            "Too many rows for Excel; select a shorter period"
        );
        let sheet = book.add_worksheet();
        sheet.set_name(&table.title)?;
        for (c, label) in table.headers.iter().enumerate() {
            sheet.write_string_with_format(0, c as u16, label, &heading)?;
            sheet.set_column_width(c as u16, 26)?;
        }
        for (r, row) in table.rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                match cell {
                    Cell::Money(_) => {
                        sheet.write_number_with_format(
                            (r + 1) as u32,
                            c as u16,
                            cell.plain().parse::<f64>()?,
                            &money,
                        )?;
                    }
                    _ => {
                        sheet.write_string((r + 1) as u32, c as u16, cell.plain())?;
                    }
                }
            }
        }
        sheet.set_freeze_panes(1, 0)?;
        if !table.rows.is_empty() {
            sheet.autofilter(
                0,
                0,
                table.rows.len() as u32,
                (table.headers.len() - 1) as u16,
            )?;
        }
    }
    // Stage beside the destination: failed workbook generation never truncates an existing file.
    let temp = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid export path"))?,
    )?;
    book.save(temp.path())?;
    temp.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn money_display_preserves_decimal_values() {
        let cell = Cell::Money("-1234567.25".parse().unwrap());
        assert_eq!(display(&cell, "Ks"), "-1,234,567.25 Ks");
        assert_eq!(display(&Cell::Unknown, "Ks"), "Unknown");
    }
    #[test]
    fn exports_full_snapshot_with_unknowns_and_untrusted_text_as_strings() {
        let dir = tempfile::tempdir().unwrap();
        let report = Report {
            from: "2026-01-01".into(),
            to: "2026-01-31".into(),
            as_of: "test".into(),
            warnings: vec!["Current balances".into()],
            tables: vec![pos_core::reports::Table {
                title: "Sales".into(),
                headers: vec!["Name".into(), "Cost".into()],
                rows: vec![vec!["=HYPERLINK(unsafe)".into(), Cell::Unknown]; 60],
            }],
        };
        let path = dir.path().join("report.xlsx");
        export(&report, "Ks", &path).unwrap();
        assert!(std::fs::metadata(path).unwrap().len() > 1000);
        if let Some(path) = std::env::var_os("REPORTS_TEST_EXPORT") {
            export(&report, "Ks", std::path::Path::new(&path)).unwrap();
        }
    }
}
