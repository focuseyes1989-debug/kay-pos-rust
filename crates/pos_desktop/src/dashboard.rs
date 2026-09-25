use crate::{DbForm, WorkspaceView, format_ks};
use dioxus::prelude::*;
use chrono::Datelike;

#[component]
pub fn DashboardPage(db_form: DbForm, actor: pos_core::auth::Session, on_navigate: EventHandler<WorkspaceView>) -> Element {
    let today = chrono::Local::now().date_naive();
    let mut from = use_signal(|| today.to_string());
    let mut to = use_signal(|| today.to_string());
    let mut range = use_signal(|| (today.to_string(),today.to_string()));
    let mut error = use_signal(String::new);
    let mut data = use_resource(move || {
        let source=db_form.clone(); let actor=actor.clone(); let (a,b)=range();
        async move {
            async {
                let pool=pos_core::connect(&source.database_config()?).await?;
                let summary=pos_core::dashboard::load(&pool,&actor,&a,&b).await?;
                let alerts=pos_core::stock_alerts::list(&pool).await?;
                Ok::<_,anyhow::Error>((summary,alerts))
            }.await.map_err(|e|format!("{e:#}"))
        }
    });
    let loading=!data.finished();
    let state=data.read();
    rsx! {
        section { class:"customers_page dashboard_page",
            header { class:"customers_header", h2 {crate::icons::ActionLabel { label:"Dashboard" }}
                button { hidden:true,"data-page-refresh":"true",tabindex:-1,aria_hidden:"true",disabled:loading,onclick:move |_|data.restart(),"Refresh" }
            }
            div {class:"dashboard_filters",
                label {"From" input {r#type:"date",value:from(),oninput:move |e|from.set(e.value())}}
                label {"To" input {r#type:"date",value:to(),oninput:move |e|to.set(e.value())}}
                button {class:"customer_primary",disabled:loading,onclick:move |_|{
                    match pos_core::sale_summary::dates(&from(),&to()) {Ok(_)=>{error.set(String::new());range.set((from(),to()));},Err(_)=>error.set("Enter a valid date range.".into())}
                },"Apply"}
                for (label,start) in [("Today",today),("This week",today-chrono::Duration::days(today.weekday().num_days_from_monday() as i64)),("This month",today.with_day(1).unwrap())] {
                    button {disabled:loading,onclick:move |_|{from.set(start.to_string());to.set(today.to_string());range.set((start.to_string(),today.to_string()));error.set(String::new());},"{label}"}
                }
            }
            if !error().is_empty() {p {role:"alert","{error}"}}
            if loading {p {role:"status","Loading dashboard..."}}
            else if let Some(Err(message))=state.as_ref() {p {role:"alert","{message}"}}
            else if let Some(Ok((summary,alerts)))=state.as_ref() {
                div {class:"dashboard_section_heading",h3 {"Period activity"} small {"{range().0} / {range().1} · Updated {summary.loaded_at}"}}
                div {class:"dashboard_metrics",
                    for row in &summary.period {
                        article {class:"dashboard_metric",small {"{row.label}"} strong {"{format_ks(row.amount)}"} small {"{row.count} records"}}
                    }
                }
                div {class:"dashboard_links",
                    for (label,view) in [("Sale Summary",WorkspaceView::SaleSummary),("Expenses",WorkspaceView::Expenses),("Customers",WorkspaceView::Customers),("Suppliers",WorkspaceView::Suppliers),("Inventory",WorkspaceView::Inventory),("Employees",WorkspaceView::Employees),("Service Orders",WorkspaceView::ServiceOrders)] {
                        button {onclick:move |_|on_navigate.call(view),crate::icons::ActionLabel {label}}
                    }
                }
                h3 {"Current balances & operations"}
                div {class:"dashboard_metrics",
                    article {class:"dashboard_metric",small {"Customer receivables"} strong {"{format_ks(summary.customers.iter().map(|r|r.amount).sum())}"} small {"{summary.customers.len()} customers"}}
                    article {class:"dashboard_metric",small {"Supplier payable (ledger)"} strong {"{format_ks(summary.suppliers.iter().map(|r|r.amount.max(0.0)).sum())}"}}
                    article {class:"dashboard_metric",small {"Supplier advances (ledger)"} strong {"{format_ks(summary.suppliers.iter().map(|r|(-r.amount).max(0.0)).sum())}"}}
                    article {class:"dashboard_metric",small {"Low stock / Out of stock"} strong {"{alerts.iter().filter(|r|r.stock>0.0).count()} / {alerts.iter().filter(|r|r.stock<=0.0).count()}"}}
                    for row in &summary.operations {
                        article {class:"dashboard_metric",small {"{row.label}"} strong {"{row.count}"} if row.amount!=0.0 {small {"{format_ks(row.amount)}"}}}
                    }
                }
                div {class:"dashboard_tables",
                    section {h3 {"Daily sales"}
                        if summary.daily.is_empty() {p {"No sales in this period."}}
                        div {class:"dashboard_scroll",table {class:"customers_table",thead {tr {th {"Date"} th {crate::icons::ActionLabel { label:"Receipts" }} th {crate::icons::ActionLabel { label:"Sales" }}}} tbody {
                            for row in &summary.daily {tr {td {"{row.label}"} td {"{row.count}"} td {"{format_ks(row.amount)}" meter {min:0,max:summary.daily.iter().map(|r|r.amount).fold(1.0,f64::max),value:row.amount,aria_label:"Sales on {row.label}"}}}}
                        }}}
                    }
                    for (title,rows) in [("Customer outstanding",&summary.customers),("Supplier ledger balances",&summary.suppliers)] {
                        section {h3 {"{title}"}
                            if rows.is_empty() {p {"No outstanding balances."}}
                            div {class:"dashboard_scroll",table {class:"customers_table",thead {tr {th {"Name"} th {"Balance"}}} tbody {
                                for row in rows {tr {td {"{row.label}"} td {"{format_ks(row.amount)}"}}}
                            }}}
                        }
                    }
                    section {h3 {"Stock alerts"}
                        if alerts.is_empty() {p {"No stock alerts."}}
                        div {class:"dashboard_scroll",table {class:"customers_table",thead {tr {th {"Product / Variant"} th {"Stock"} th {"Alert at"}}} tbody {
                            for row in alerts {tr {td {"{row.name}"} td {"{crate::format_qty(row.stock)}"} td {"{crate::format_qty(row.threshold)}"}}}
                        }}}
                    }
                }
            }
        }
    }
}
