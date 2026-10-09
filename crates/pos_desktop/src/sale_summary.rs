use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{connect, sale_summary};

#[component]
pub fn SaleSummaryPage(db_form:DbForm,on_sales:EventHandler<()>)->Element {
 let mut comparison=use_signal(||false);
 let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
 rsx!{section{class:"customers_page sales_summary_views",
  nav{class:"receipt_tabs",role:"tablist",aria_label:"Sale Summary views",
   button{role:"tab",aria_selected:!comparison(),class:if !comparison(){"active"}else{""},onclick:move |_|comparison.set(false),"Summary"}
   button{role:"tab",aria_selected:comparison(),class:if comparison(){"active"}else{""},onclick:move |_|comparison.set(true),"Comparison"}
  }
  if comparison(){if let Some(actor)=session(){crate::sales_comparison::SalesComparison{db_form,actor}}}
  else{SummaryDetails{db_form,on_sales}}
 }}
}

#[component]
fn SummaryDetails(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let today = chrono::Local::now().date_naive();
    let mut from = use_signal(|| today.to_string());
    let mut to = use_signal(|| today.to_string());
    let mut range = use_signal(|| (today.to_string(), today.to_string()));
    let mut error = use_signal(String::new);
    let mut tab = use_signal(|| 0usize);
    let mut selected_category=use_signal(||None::<(String,String,String)>);
    let dialog_source=db_form.clone();
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
        header {class:"customers_header customers_touch_header",h2 {crate::icons::ActionLabel { label:"Sale Summary" }} div {class:"customers_actions",button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}}}
        div {class:"summary_filters",
            crate::date_range::DateRangePicker { from:from(), to:to(), disabled:!data.finished(), onchange:move |(a,b):(String,String)|{from.set(a.clone());to.set(b.clone());error.set(String::new());range.set((a,b));} }
            crate::date_range::DateRangeShortcuts { disabled:!data.finished(), onchange:move |(a,b):(String,String)|{from.set(a.clone());to.set(b.clone());error.set(String::new());range.set((a,b));} }
        }
        if !error().is_empty(){p {role:"alert","{error}"}}
        if !data.finished(){div {class:"product_loading",role:"status",span {class:"loading_spinner"} strong {"Loading..."}}}
        else if let Some(Err(e))=state.as_ref(){p {role:"alert","{e}"}}
        else {
            div {class:"summary_metrics",
                div {small {crate::icons::ActionLabel { label:"Receipts" }} strong {"{receipts}"}}
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
                        for row in group {{let category=row.label.clone();rsx!{tr {class:if tab()==2{"summary_category_row"}else{""},title:if tab()==2{"Double-click to view category receipts"}else{""},ondoubleclick:move |_|{if tab()==2 {let (a,b)=range();selected_category.set(Some((category.clone(),a,b)));}},td {if tab()==2 {button {class:"summary_category_link",onclick:{let category=row.label.clone();move |_|{let (a,b)=range();selected_category.set(Some((category.clone(),a,b)));}},"{row.label}"}}else{"{row.label}"}} td {"{row.receipts}"} td {{crate::format_qty(row.quantity)}} for value in [row.gross,row.discount,row.net,row.cost,row.net-row.cost,row.savings] {td {{crate::format_ks(value)}}}}}}}
                    }
                } tfoot {tr {th {scope:"row","Total"} td {title:"Distinct receipts",{result.group_receipts.get(group_index).copied().unwrap_or(0).to_string()}} td {{crate::format_qty(group_totals[0])}} for value in group_totals.iter().skip(1) {td {{crate::format_ks(*value)}}}}}}}
            }
        }
        if let Some((category,a,b))=selected_category() {CategoryReceiptsDialog {db_form:dialog_source.clone(),category,from:a,to:b,on_close:move |_|selected_category.set(None)}}
    }}
}

#[component]
fn CategoryReceiptsDialog(db_form:DbForm,category:String,from:String,to:String,on_close:EventHandler<()>)->Element {
 let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
 let actor=session().expect("Signed-in summary");
 let mut page=use_signal(||0usize);
 let mut selected=use_signal(||None::<i32>);
 let list_source=db_form.clone();let list_actor=actor.clone();let label=category.clone();let a=from.clone();let b=to.clone();
 let receipts=use_resource(move ||{let source=list_source.clone();let actor=list_actor.clone();let name=label.clone();let a=a.clone();let b=b.clone();let page=page();async move {async {sale_summary::category_receipts(&connect(&source.database_config()?).await?,&actor,&name,&a,&b,page).await}.await.map_err(|e|format!("{e:#}"))}});
 let detail=use_resource(move ||{let source=db_form.clone();let actor=actor.clone();let id=selected();async move {let Some(id)=id else{return Ok(None)};async {let pool=connect(&source.database_config()?).await?;actor.authorize(&mut *pool.acquire().await?,pos_core::auth::Permission::Manage).await?;let value=pos_core::get_receipt_detail(&pool,id).await?;actor.authorize(&mut *pool.acquire().await?,pos_core::auth::Permission::Manage).await?;Ok::<_,anyhow::Error>(Some(value))}.await.map_err(|e|format!("{e:#}"))}});
 let detail_state=detail.read();
 let state=receipts.read();let rows=state.as_ref().and_then(|r|r.as_ref().ok()).cloned().unwrap_or_default();
 rsx!{dialog {class:"summary_receipts_dialog",aria_label:"Category receipts",onmounted:move |_|{document::eval("document.querySelector('.summary_receipts_dialog:not([open])')?.showModal();");},oncancel:move |e|{e.prevent_default();on_close.call(());},
  header {class:"summary_receipts_heading",div {h3 {"{category} · Receipts"} small {"{from} – {to}"}} button {aria_label:"Close",onclick:move |_|on_close.call(()),crate::icons::Icon {name:"close"}}}
  div {class:"summary_receipts_content",
   if selected().is_some() {
    button {onclick:move |_|selected.set(None),"← Back to receipts"}
    if !detail.finished(){p {role:"status","Loading receipt…"}}else{match detail_state.as_ref(){Some(Ok(value))=>rsx!{crate::ReceiptDetailPanel {detail:value.clone(),read_only:true,on_close:move |_|selected.set(None),on_print:move |_|{},on_refund:move |_|{}}},Some(Err(e))=>rsx!{p {role:"alert","{e}"}},_=>rsx!{p {"Receipt not found"}}}}
   }else{
    p {class:"summary_receipts_note","Receipt total သည် category တစ်ခုတည်း၏ ရောင်းရငွေ မဟုတ်ဘဲ ဘောင်ချာတစ်ခုလုံး၏ စုစုပေါင်း ဖြစ်ပါသည်။"}
    if !receipts.finished(){p {role:"status","Loading receipts…"}}else if let Some(Err(e))=state.as_ref(){p {role:"alert","{e}"}}else{
     div {class:"summary_table",table {thead {tr {for heading in ["Receipt","Date","Customer","Receipt total","Payment"] {th {"{heading}"}}}} tbody {
      if rows.is_empty(){tr {td {colspan:"5","No receipts in this date range."}}}
      for row in rows.iter().take(50){tr {td {button {class:"summary_category_link",onclick:{let id=row.id;move |_|selected.set(Some(id))},"{row.invoice}"}} td {"{row.date}"} td {"{row.customer}"} td {{crate::format_ks(row.total)}} td {"{row.payment}"}}}
     }}}
     div {class:"summary_receipts_pages",button { title:"Previous page", aria_label:"Previous page",disabled:page()==0,onclick:move |_|{page-=1;},crate::icons::Icon { name:"arrow_circle_left" }} span {"Page {page()+1}"} button { title:"Next page", aria_label:"Next page",disabled:rows.len()<=50,onclick:move |_|{page+=1;},crate::icons::Icon { name:"arrow_circle_right" }}}
    }
   }
  }
 }}
}

#[cfg(test)]
mod detail_loading_tests {
 use super::*;
 use std::{rc::Rc,cell::Cell,time::Duration};
 #[derive(Clone)] struct ProbeState {selected:Signal<Option<i32>>,loaded:Rc<Cell<Option<i32>>>}
 fn probe()->Element {
  let selected=use_signal(||None::<i32>);let loaded=use_hook(||Rc::new(Cell::new(None)));
  let resource=use_resource(move ||{let id=selected();async move{tokio::time::sleep(Duration::from_millis(20)).await;id}});
  use_context_provider(||ProbeState{selected,loaded:loaded.clone()});
  // Subscribe before rendering the pending branch, just like the receipt dialog.
  let state=resource.read();
  if !resource.finished(){rsx!{p {"Loading receipt"}}}else{loaded.set(state.as_ref().copied().flatten());rsx!{p {"Loaded"}}}
 }
 #[tokio::test(flavor="current_thread")]
 async fn selected_receipt_completion_rerenders_pending_dialog() {
  let mut dom=VirtualDom::new(probe);dom.rebuild_in_place();
  let mut state=dom.in_scope(ScopeId::APP,consume_context::<ProbeState>);state.selected.set(Some(42));
  let deadline=tokio::time::Instant::now()+Duration::from_millis(200);
  while state.loaded.get()!=Some(42){tokio::select!{_=dom.wait_for_work()=>{dom.render_immediate_to_vec();},_=tokio::time::sleep_until(deadline)=>break}}
  assert_eq!(state.loaded.get(),Some(42),"resource completion must replace Loading receipt without another click");
 }
}
