use crate::{DbForm, WorkspaceView, format_ks};
use dioxus::prelude::*;

fn bar_width(amount: f64, maximum: f64) -> f64 {
    if !amount.is_finite() || !maximum.is_finite() || maximum <= 0.0 { return 0.0; }
    (amount / maximum * 100.0).clamp(0.0, 100.0)
}

fn amount_chart(rows: &[pos_core::dashboard::Row], daily: bool) -> Element {
    let maximum = rows.iter().map(|r| r.amount).fold(0.0, f64::max);
    rsx! {
        div { class: "dashboard_chart_rows",
            for row in rows {
                div { class: "dashboard_chart_row",
                    span { class: "dashboard_chart_label", "{row.label}"
                        if daily { small { "{row.count} receipts" } }
                    }
                    div { class: "dashboard_chart_track", aria_hidden: "true", span { style: "width:{bar_width(row.amount,maximum)}%" } }
                    strong { "{format_ks(row.amount)}" }
                }
            }
        }
    }
}

fn hourly_chart(rows: &[pos_core::dashboard::Row], by_count: bool) -> Element {
    let value = |r: &pos_core::dashboard::Row| if by_count {r.count as f64} else {r.amount};
    let maximum = rows.iter().map(value).fold(0.0,f64::max);
    let mut ranked: Vec<_> = rows.iter().filter(|r| r.count > 0).collect();
    ranked.sort_by(|a,b|value(b).total_cmp(&value(a)).then(a.label.cmp(&b.label)));
    rsx! {
        if ranked.is_empty() {
            div {class:"dashboard_chart_empty", "No completed sales in this period."}
        } else {
            div {class:"dashboard_peak_summary",
                for (index,row) in ranked.iter().take(3).enumerate() {
                    div {class:"dashboard_peak_stat",
                        small {if index==0 {"Peak hour"}else{"Top selling hour"}}
                        strong {"{row.label}:00 – {row.label}:59"}
                        span {"{format_ks(row.amount)} · {row.count} receipts"}
                    }
                }
            }
            div {class:"dashboard_hourly_scroll",
                div {class:"dashboard_hourly_plot",role:"img",aria_label:"Hourly completed sales. Each bar represents one hour; exact sales and receipt counts are available below.",
                    for row in rows {
                        div {class:if maximum>0.0 && value(row)==maximum {"dashboard_hour_column peak"} else {"dashboard_hour_column"},
                            title:"{row.label}:00 – {row.label}:59 · {format_ks(row.amount)} · {row.count} receipts",
                            div {class:"dashboard_hour_track",span {style:"height:{bar_width(value(row),maximum)}%"}}
                            small {"{row.label}"}
                        }
                    }
                }
            }
            small {class:"dashboard_hour_hint", "00–23 hours · Hours use the same shop time as receipts · Point to a bar for details"}
            details {class:"dashboard_hour_details",
                summary {"View hourly figures"}
                table {class:"customers_table",thead {tr {th {"Hour"} th {"Sales"} th {"Receipts"}}} tbody {
                    for row in rows {tr {td {"{row.label}:00 – {row.label}:59"} td {"{format_ks(row.amount)}"} td {"{row.count}"}}}
                }}
            }
        }
    }
}

#[cfg(test)]
mod chart_tests {
    use super::*;
    #[test]
    fn chart_widths_handle_empty_and_invalid_amounts() {
        assert_eq!(bar_width(25.0,100.0),25.0);
        assert_eq!(bar_width(0.0,0.0),0.0);
        assert_eq!(bar_width(-5.0,100.0),0.0);
        assert_eq!(bar_width(200.0,100.0),100.0);
        assert_eq!(bar_width(f64::NAN,100.0),0.0);
    }
}

#[component]
pub fn DashboardPage(db_form: DbForm, actor: pos_core::auth::Session, on_navigate: EventHandler<WorkspaceView>) -> Element {
    let today = chrono::Local::now().date_naive();
    let mut from = use_signal(|| today.to_string());
    let mut to = use_signal(|| today.to_string());
    let mut range = use_signal(|| (today.to_string(),today.to_string()));
    let mut error = use_signal(String::new);
    let mut hourly_by_count = use_signal(||false);
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
        style { {include_str!("../assets/dashboard.css")} }
        section { class:"customers_page dashboard_page",
            button { hidden:true,"data-page-refresh":"true",tabindex:-1,aria_hidden:"true",disabled:loading,onclick:move |_|data.restart(),"Refresh" }
            div {class:"dashboard_filters",
                crate::date_range::DateRangePicker { from:from(), to:to(), disabled:!data.finished(), onchange:move |(a,b):(String,String)|{from.set(a.clone());to.set(b.clone());error.set(String::new());range.set((a,b));} }
                div { class:"dashboard_date_shortcuts",
                    for (label,days) in [("Today",0i64),("Last 7 days",6),("Last 30 days",29)] {
                        button { disabled:loading, onclick:move |_| {
                            let end=chrono::Local::now().date_naive();
                            let start=(end-chrono::Duration::days(days)).to_string(); let end=end.to_string();
                            from.set(start.clone()); to.set(end.clone()); range.set((start,end)); error.set(String::new());
                        }, "{label}" }
                    }
                }
            }
            if !error().is_empty() {p {role:"alert","{error}"}}
            if loading {p {role:"status","Loading dashboard..."}}
            else if let Some(Err(message))=state.as_ref() {p {role:"alert","{message}"}}
            else if let Some(Ok((summary,alerts)))=state.as_ref() {
                div {class:"dashboard_section_heading",h3 {"Period activity"} small {"{range().0} / {range().1} · Updated {summary.loaded_at}"}}
                div {class:"dashboard_metrics dashboard_primary_metrics",
                    for label in ["Completed sales","Received at checkout","Recorded expenses","Credit sales"] {
                        if let Some(row)=summary.period.iter().find(|r|r.label==label) {
                            article {class:"dashboard_metric", "data-metric":label,
                                div {class:"dashboard_metric_heading",
                                    small {crate::icons::CardLabel {label:row.label.clone()}}
                                    small {class:"dashboard_metric_count","{row.count} records"}
                                }
                                strong {"{format_ks(row.amount)}"}
                            }
                        }
                    }
                }
                div {class:"dashboard_charts",
                    section {class:"dashboard_chart_card",
                        header { div {h3 {"Daily sales"} small {"Completed receipts · Selected period"}} }
                        if summary.daily.is_empty() {div {class:"dashboard_chart_empty", "No completed sales in this period."}}
                        else { {amount_chart(&summary.daily,true)} }
                    }
                    section {class:"dashboard_chart_card dashboard_comparison",
                        header {div {h3 {"Period totals"} small {"Compare sales, expenses and supplier purchases"}}}
                        {amount_chart(&summary.period.iter().filter(|r|matches!(r.label.as_str(),"Completed sales"|"Recorded expenses"|"Supplier purchases (ledger)"|"Supplier payments")).cloned().collect::<Vec<_>>(),false)}
                    }
                }
                section {class:"dashboard_chart_card dashboard_hourly_card",
                    header {div {h3 {"Best selling hours"} small {"Completed receipts · Selected period · Combined by hour"}}
                        div {class:"dashboard_hour_controls",role:"group",aria_label:"Rank selling hours by",
                            button {class:if !hourly_by_count(){"selected"}else{""},aria_pressed:(!hourly_by_count()).to_string(),onclick:move |_|hourly_by_count.set(false),"Sales amount"}
                            button {class:if hourly_by_count(){"selected"}else{""},aria_pressed:hourly_by_count().to_string(),onclick:move |_|hourly_by_count.set(true),"Receipt count"}
                        }
                    }
                    {hourly_chart(&summary.hourly,hourly_by_count())}
                }
                details {class:"dashboard_more_metrics",
                    summary {"More period details"}
                    div {class:"dashboard_metrics",
                        for row in summary.period.iter().filter(|r|!matches!(r.label.as_str(),"Completed sales"|"Received at checkout"|"Recorded expenses"|"Credit sales")) {
                            article {class:"dashboard_metric",
                                div {class:"dashboard_metric_heading",small {crate::icons::CardLabel {label:row.label.clone()}} small {class:"dashboard_metric_count","{row.count} records"}}
                                strong {"{format_ks(row.amount)}"}
                            }
                        }
                    }
                }
                div {class:"dashboard_links",
                    for (label,view) in [("Sale Summary",WorkspaceView::SaleSummary),("Expenses",WorkspaceView::Expenses),("Customers",WorkspaceView::Customers),("Suppliers",WorkspaceView::Suppliers),("Inventory",WorkspaceView::Inventory),("Employees",WorkspaceView::Employees),("Service Orders",WorkspaceView::ServiceOrders)] {
                        button {onclick:move |_|on_navigate.call(view),crate::icons::ActionLabel {label}}
                    }
                }
                div {class:"dashboard_section_heading",h3 {"Current balances & operations"} small {"Current totals across all dates"}}
                div {class:"dashboard_metrics dashboard_operations_metrics",
                    article {class:"dashboard_metric stock_value_card",
                        small {crate::icons::CardLabel {label:"Stock value"}}
                        strong {{crate::inventory::stock_money(summary.stock_value.total)}}
                        small {"Current stock × cost · Services excluded"}
                        small {"Missing cost: {summary.stock_value.missing_cost_products} products ({summary.stock_value.missing_cost_in_stock} in stock)"}
                        button {onclick:move |_|on_navigate.call(WorkspaceView::Inventory),"View inventory details"}
                    }

                    article {class:"dashboard_metric",small {crate::icons::CardLabel {label:"Customer receivables"}} strong {"{format_ks(summary.customers.iter().map(|r|r.amount).sum())}"} small {"{summary.customers.len()} customers"}}
                    article {class:"dashboard_metric",small {crate::icons::CardLabel {label:"Supplier payable (ledger)"}} strong {"{format_ks(summary.suppliers.iter().map(|r|r.amount.max(0.0)).sum())}"}}
                    article {class:"dashboard_metric",small {crate::icons::CardLabel {label:"Supplier advances (ledger)"}} strong {"{format_ks(summary.suppliers.iter().map(|r|(-r.amount).max(0.0)).sum())}"}}
                    article {class:"dashboard_metric",small {crate::icons::CardLabel {label:"Low stock / Out of stock"}} strong {"{alerts.iter().filter(|r|r.stock>0.0).count()} / {alerts.iter().filter(|r|r.stock<=0.0).count()}"}}
                    for row in &summary.operations {
                        article {class:"dashboard_metric",small {crate::icons::CardLabel {label:row.label.clone()}} strong {"{row.count}"} if row.amount!=0.0 {small {"{format_ks(row.amount)}"}}}
                    }
                }
                div {class:"dashboard_tables",
                    section {h3 {"Daily sales details"}
                        if summary.daily.is_empty() {p {"No sales in this period."}}
                        div {class:"dashboard_scroll",table {class:"customers_table",thead {tr {th {"Date"} th {crate::icons::ActionLabel { label:"Receipts" }} th {crate::icons::ActionLabel { label:"Sales" }}}} tbody {
                            for row in &summary.daily {tr {td {"{row.label}"} td {"{row.count}"} td {"{format_ks(row.amount)}"}}}
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
