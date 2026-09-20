use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{connect, sale_summary};

#[component]
pub fn SaleSummaryPage(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let today = chrono::Local::now().date_naive();
    let mut from = use_signal(|| today.to_string());
    let mut to = use_signal(|| today.to_string());
    let mut range = use_signal(|| (today.to_string(), today.to_string()));
    let mut error = use_signal(String::new);
    let mut tab = use_signal(|| 0usize);
    let mut data = use_resource(move || {
        let source = db_form.clone();
        let (from, to) = range();
        async move {
            async {
                sale_summary::load(&connect(&source.database_config()?).await?, &from, &to).await
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let state = data.read();
    let result = state
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned()
        .unwrap_or_default();
    let receipts: i64 = result.daily.iter().map(|r| r.receipts).sum();
    let sales: f64 = result.daily.iter().map(|r| r.sales).sum();
    let discount: f64 = result.daily.iter().map(|r| r.discount).sum();
    let refunds: f64 = result.daily.iter().map(|r| r.refunds).sum();
    let cost: f64 = result.daily.iter().map(|r| r.cost).sum();
    let estimated: i64 = result.daily.iter().map(|r| r.estimated).sum();
    let rows = if tab() == 1 {
        &result.payments
    } else {
        &result.daily
    };
    let gross: f64 = rows.iter().map(|r| r.gross).sum();
    let group_index = match tab() {
        2 => 0,
        3 => 1,
        4 => 2,
        6 => 3,
        _ => 4,
    };
    let mut group_totals = [0.0; 7];
    if let Some(group) = result.groups.get(group_index) {
        for row in group {
            for (total, value) in group_totals.iter_mut().zip([
                row.quantity,
                row.gross,
                row.discount,
                row.net,
                row.cost,
                row.net - row.cost,
                row.savings,
            ]) {
                *total += value;
            }
        }
    }
    let expense_records: i64 = result.expenses.iter().map(|r| r.records).sum();
    let expense_amount: f64 = result.expenses.iter().map(|r| r.amount).sum();
    rsx! {section {class:"customers_page products_touch summary_touch",
        header {class:"customers_header customers_touch_header",h2 {"Sale Summary"} div {class:"customers_actions",button {disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}}}
        div {class:"summary_filters",
            label {"From" input {r#type:"date",value:"{from}",oninput:move |e|from.set(e.value())}}
            label {"To" input {r#type:"date",value:"{to}",oninput:move |e|to.set(e.value())}}
            button {class:"customer_primary",onclick:move |_|match sale_summary::dates(&from(),&to()){Ok(_)=>{error.set(String::new());range.set((from(),to()));},Err(e)=>error.set(e.to_string())},"Apply filters"}
            button {onclick:move |_|{let date=chrono::Local::now().date_naive().to_string();from.set(date.clone());to.set(date.clone());range.set((date.clone(),date));error.set(String::new());},"Today"}
            button {onclick:move |_|{let end=chrono::Local::now().date_naive();let start=end-chrono::Duration::days(6);from.set(start.to_string());to.set(end.to_string());range.set((start.to_string(),end.to_string()));error.set(String::new());},"Last 7 days"}
        }
        if !error().is_empty(){p {role:"alert","{error}"}}
        if !data.finished(){div {class:"product_loading",role:"status",span {class:"loading_spinner"} strong {"Loading..."}}}
        else if let Some(Err(e))=state.as_ref(){p {role:"alert","{e}"}}
        else {
            div {class:"summary_metrics",
                div {small {"Receipts"} strong {"{receipts}"}}
                for (label,value) in [("Sales",sales),("Discount",discount),("Refunds",refunds),("Cost of goods",cost),("Gross profit",sales-cost)] {div {small {"{label}"} strong {{crate::format_ks(value)}}}}
            }
            if estimated>0 {p {class:"summary_warning","Estimated cost: {estimated} item lines use current product/variant cost."}}
            div {class:"receipt_tabs summary_tabs",role:"tablist",for (index,label) in ["Daily","Payments","Categories (Sales)","Parent","Items","Categories (Expense)","Wholesale","Discount"].iter().enumerate() {button {role:"tab",aria_selected:tab()==index,class:if tab()==index{"active"}else{""},onclick:move |_|tab.set(index),"{label}"}}}
            if tab()<2 {
            div {class:"summary_table",table {thead {tr {th {if tab()==1{"Payment method"}else{"Date"}} for title in ["Receipts","Sales","Item gross","Discount","Net sales","Cost of goods","Gross profit","Refunds"] {th {"{title}"}}}}
                tbody {if rows.is_empty(){tr {td {colspan:"9","No sales in this date range."}}} for row in rows {tr {td {"{row.label}"} td {"{row.receipts}"} for value in [row.sales,row.gross,row.discount,row.gross-row.discount,row.cost,row.sales-row.cost,row.refunds] {td {{crate::format_ks(value)}}}}}}
                tfoot {tr {th {scope:"row","Total"} td {"{receipts}"} for value in [sales,gross,discount,gross-discount,cost,sales-cost,refunds] {td {{crate::format_ks(value)}}}}}
            }}
            } else if tab()==5 {
                div {class:"summary_table",table {thead {tr {th {"Expense category"} th {"Records"} th {"Total expense"}}} tbody {
                    if result.expenses.is_empty(){tr {td {colspan:"3","No expenses in this date range."}}}
                    for row in &result.expenses {tr {td {"{row.label}"} td {"{row.records}"} td {{crate::format_ks(row.amount)}}}}
                } tfoot {tr {th {scope:"row","Total"} td {"{expense_records}"} td {{crate::format_ks(expense_amount)}}}}}}
            } else {
                div {class:"summary_table",table {thead {tr {for label in ["Name","Receipts","Quantity","Item gross","Allocated discount","Net sales","Cost of goods","Net less cost","Wholesale savings"] {th {"{label}"}}}} tbody {
                    if let Some(group)=result.groups.get(match tab(){2=>0,3=>1,4=>2,6=>3,_=>4}) {
                        if group.is_empty(){tr {td {colspan:"9","No sales in this date range."}}}
                        for row in group {tr {td {"{row.label}"} td {"{row.receipts}"} td {{crate::format_qty(row.quantity)}} for value in [row.gross,row.discount,row.net,row.cost,row.net-row.cost,row.savings] {td {{crate::format_ks(value)}}}}}
                    }
                } tfoot {tr {th {scope:"row","Total"} td {title:"Distinct receipts",{result.group_receipts.get(group_index).copied().unwrap_or(0).to_string()}} td {{crate::format_qty(group_totals[0])}} for value in group_totals.iter().skip(1) {td {{crate::format_ks(*value)}}}}}}}
            }
        }
    }}
}
