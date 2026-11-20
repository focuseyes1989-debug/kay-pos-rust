use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{connect, sale_summary};

#[component]
pub fn SaleSummaryPage(db_form:DbForm,on_sales:EventHandler<()>)->Element {
 let mut comparison=use_signal(||false);
 let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
 rsx!{section{class:"customers_page sales_summary_views",
  nav{class:"receipt_tabs",role:"tablist",aria_label:"Sale Summary views",
   button{role:"tab",aria_selected:!comparison(),class:if !comparison(){"active"}else{""},onclick:move |_|comparison.set(false),crate::icons::TabLabel {label:"Summary"}}
   button{role:"tab",aria_selected:comparison(),class:if comparison(){"active"}else{""},onclick:move |_|comparison.set(true),crate::icons::TabLabel {label:"Comparison"}}
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
    let mut output_busy=use_signal(||false);
    let mut report_activity=use_context::<crate::status_bar::ReportActivity>().0;
    use_effect(move ||report_activity.set(output_busy()));
    use_drop(move ||{if let Ok(mut busy)=report_activity.try_write(){*busy=false;}});
    let mut output_notice=use_signal(String::new);
    let mut tab = use_signal(|| 0usize);
    let mut expanded_locations=use_signal(std::collections::BTreeSet::<String>::new);
    let mut selected_category=use_signal(||None::<(String,String,String)>);
    let dialog_source=db_form.clone();
    let mut data = use_resource(move || {
        let source = db_form.clone();
        let (from, to) = range();
        async move {
            async {
                let report=sale_summary::load(&connect(&source.database_config()?).await?, &from, &to).await?;
                Ok::<_,anyhow::Error>(((from,to),report))
            }
            .await
            .map_err(|e| format!("{e:#}"))
        }
    });
    let state = data.read();
    let result = state
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|(_,report)|report.clone())
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
        8 => 5,
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
    let loaded_range=state.as_ref().and_then(|r|r.as_ref().ok()).map(|(dates,_)|dates.clone());
    let output_ready=data.finished() && loaded_range.as_ref()==Some(&range());
    rsx! {section {class:"customers_page products_touch summary_touch",
        header {class:"customers_header customers_touch_header",h2 {crate::icons::ActionLabel { label:"Sale Summary" }} div {class:"customers_actions",button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}}}
        div {class:"summary_filters",
            crate::date_range::DateRangePicker { from:from(), to:to(), disabled:!data.finished(), onchange:move |(a,b):(String,String)|{from.set(a.clone());to.set(b.clone());error.set(String::new());range.set((a,b));} }
            crate::date_range::DateRangeShortcuts { disabled:!data.finished(), onchange:move |(a,b):(String,String)|{from.set(a.clone());to.set(b.clone());error.set(String::new());range.set((a,b));} }
        }
        if !output_notice().is_empty(){
            crate::MessageBox {title:"Sale Summary".to_string(),message:output_notice(),on_close:move |_|output_notice.set(String::new())}
        }
        if !error().is_empty(){p {role:"alert","{error}"}}
        if !data.finished(){div {class:"product_loading",role:"status",span {class:"loading_spinner"} strong {"Loading..."}}}
        else if let Some(Err(e))=state.as_ref(){p {role:"alert","{e}"}}
        else {
            div {class:"summary_metrics",
                div {small {crate::icons::CardLabel { label:"Receipts" }} strong {"{receipts}"}}
                for (label,value) in [("Sales",sales),("Discount",discount),("Refunds",refunds),("Cost of goods",cost),("Gross profit",sales-cost)] {div {small {crate::icons::CardLabel {label}} strong {{crate::format_ks(value)}}}}
            }
            if estimated>0 {p {class:"summary_warning","Estimated cost: {estimated} item lines use current product/variant cost."}}
            div {class:"receipt_tabs summary_tabs",role:"tablist",for (index,label) in ["Daily","Payments","Categories (Sales)","Parent","Items","Categories (Expense)","Wholesale","Discount","Location"].iter().enumerate() {button {role:"tab",aria_selected:tab()==index,class:if tab()==index{"active"}else{""},onclick:move |_|tab.set(index),crate::icons::TabLabel {label:*label}}}}
                div {class:"customers_actions",
                    for (pdf,label) in [(true,"Export Full PDF"),(false,"Print Full Report")] {
                        button {r#type:"button",disabled:!output_ready||output_busy(),onclick:{
                            let dates=loaded_range.clone();
                            move |_| {
                                if output_busy()||!data.finished()||dates.as_ref()!=Some(&range()){return;}
                                let report={
                                    let state=data.read();
                                    let Some(Ok(((from,to),summary)))=state.as_ref() else{return};
                                    if (from.clone(),to.clone())!=range(){return;}
                                    full_report(from,to,summary)
                                };
                                output_busy.set(true);output_notice.set(String::new());error.set(String::new());
                                spawn(async move {
                                    match output_report(report,pdf).await {
                                        Ok(message)=>output_notice.set(message),
                                        Err(e)=>error.set(format!("{e:#}")),
                                    }
                                    output_busy.set(false);
                                });
                            }
                        },crate::icons::ActionLabel {label}}
                    }

                }
            if tab()<2 {
            div {class:"summary_table",table {thead {tr {th {if tab()==1{"Payment method"}else{"Date"}} for title in &DAILY_COLUMNS[1..] {th {"{title}"}}}}
                tbody {if rows.is_empty(){tr {td {colspan:"9","No sales in this date range."}}} for row in rows {tr {for cell in daily_cells(row) {td {"{cell}"}}}}}
                tfoot {tr {th {scope:"row","Total"} td {"{receipts}"} for value in [sales,gross,discount,gross-discount,cost,sales-cost,refunds] {td {{crate::format_ks(value)}}}}}
            }}
            } else if tab()==5 {
                div {class:"summary_table",table {thead {tr {th {"Expense category"} th {"Records"} th {"Total expense"}}} tbody {
                    if result.expenses.is_empty(){tr {td {colspan:"3","No expenses in this date range."}}}
                    for row in &result.expenses {tr {td {"{row.label}"} td {"{row.records}"} td {{crate::format_ks(row.amount)}}}}
                } tfoot {tr {th {scope:"row","Total"} td {"{expense_records}"} td {{crate::format_ks(expense_amount)}}}}}}
            } else if tab()==8 {
                p {class:"summary_receipts_note","Parent totals include their own stock and all child locations. Expand a parent to see each selling location. Receipt counts are distinct within each parent."}
                div {class:"summary_table",table {
                    thead {tr {for label in ["Location","Receipts","Quantity","Item gross","Allocated discount","Net sales","Cost of goods","Net less cost","Wholesale savings"] {th {"{label}"}}}}
                    tbody {
                        if result.location_groups.is_empty(){tr {td {colspan:"9","No sales in this date range."}}}
                        for group in &result.location_groups {
                            tr {class:"summary_location_parent",td {button {class:"summary_category_link",aria_expanded:expanded_locations.read().contains(&group.total.label),onclick:{let label=group.total.label.clone();move |_|{let mut expanded=expanded_locations.write();if !expanded.remove(&label){expanded.insert(label.clone());}}},if expanded_locations.read().contains(&group.total.label){"▾ "}else{"▸ "}"{group.total.label}"}} td {"{group.total.receipts}"} td {{crate::format_qty(group.total.quantity)}} for value in [group.total.gross,group.total.discount,group.total.net,group.total.cost,group.total.net-group.total.cost,group.total.savings] {td {{crate::format_ks(value)}}}}
                            if expanded_locations.read().contains(&group.total.label) {
                                for (row,depth) in &group.children {
                                    tr {class:"summary_location_child",td {style:format!("padding-left:{}px",24+depth*16),"{row.label}"} td {"{row.receipts}"} td {{crate::format_qty(row.quantity)}} for value in [row.gross,row.discount,row.net,row.cost,row.net-row.cost,row.savings] {td {{crate::format_ks(value)}}}}
                                }
                            }
                        }
                    }
                    tfoot {tr {th {scope:"row","Total"} td {title:"Distinct receipts",{result.group_receipts.get(5).copied().unwrap_or(0).to_string()}} td {{crate::format_qty(group_totals[0])}} for value in group_totals.iter().skip(1) {td {{crate::format_ks(*value)}}}}}
                }}
            } else {
                div {class:"summary_table",table {thead {tr {for label in [if tab()==8{"Location"}else{"Name"},"Receipts","Quantity","Item gross","Allocated discount","Net sales","Cost of goods","Net less cost","Wholesale savings"] {th {"{label}"}}}} tbody {
                    if let Some(group)=result.groups.get(group_index) {
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
    p {class:"summary_receipts_note",{crate::regional::text("Receipt total သည် category တစ်ခုတည်း၏ ရောင်းရငွေ မဟုတ်ဘဲ ဘောင်ချာတစ်ခုလုံး၏ စုစုပေါင်း ဖြစ်ပါသည်။")}}
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


const DAILY_COLUMNS: [&str;9]=["Date","Receipts","Sales","Item gross","Discount","Net sales","Cost of goods","Gross profit","Refunds"];

fn daily_cells(row:&sale_summary::SummaryRow)->Vec<String>{
    let mut cells=vec![row.label.clone(),row.receipts.to_string()];
    cells.extend([row.sales,row.gross,row.discount,row.gross-row.discount,row.cost,row.sales-row.cost,row.refunds].map(crate::format_ks));
    cells
}

fn full_report(from:&str,to:&str,summary:&sale_summary::Summary)->serde_json::Value {
    use serde_json::json;
    let section=|title:&str, columns:Vec<&str>, rows:Vec<Vec<String>>, total:Vec<String>| {
        json!({"title":title,"columns":columns,"rows":rows,"total":total})
    };
    let summary_total=|rows:&[sale_summary::SummaryRow]| {
        let mut total=sale_summary::SummaryRow{label:"Total".into(),..Default::default()};
        for r in rows {
            total.receipts+=r.receipts;total.sales+=r.sales;total.gross+=r.gross;
            total.discount+=r.discount;total.cost+=r.cost;total.refunds+=r.refunds;
        }
        daily_cells(&total)
    };
    let group_cells=|label:String,receipts:i64,quantity:f64,gross:f64,discount:f64,net:f64,cost:f64,savings:f64| {
        let mut cells=vec![label,receipts.to_string(),crate::format_qty(quantity)];
        cells.extend([gross,discount,net,cost,net-cost,savings].map(crate::format_ks));
        cells
    };
    let row_cells=|row:&sale_summary::GroupRow,label:String| group_cells(label,row.receipts,row.quantity,row.gross,row.discount,row.net,row.cost,row.savings);
    let parent_section=|| {
        let index=1;
        let source=summary.groups.get(index).map(Vec::as_slice).unwrap_or_default();
        let mut total=[0.;6];
        for r in source {
            for (t,v) in total.iter_mut().zip([r.quantity,r.gross,r.discount,r.net,r.cost,r.savings]) {*t+=v;}
        }
        let count=summary.group_receipts.get(index).copied().unwrap_or(0);
        let rows=source.iter().map(|r|row_cells(r,r.label.clone())).collect();
        section("Parent",vec!["Name","Receipts","Quantity","Item gross","Alloc. discount","Net sales","Cost of goods","Net less cost","Wholesale savings"],
            rows,group_cells("Total".into(),count,total[0],total[1],total[2],total[3],total[4],total[5]))
    };
    let mut payment_columns=DAILY_COLUMNS.to_vec();
    payment_columns[0]="Payment method";
    let expenses=summary.expenses.iter().map(|r|vec![r.label.clone(),r.records.to_string(),crate::format_ks(r.amount)]).collect();
    let recorded_expenses=summary.expenses.iter().map(|r|r.amount).sum::<f64>();
    let expense_total=vec!["Total".into(),summary.expenses.iter().map(|r|r.records).sum::<i64>().to_string(),
        crate::format_ks(recorded_expenses)];
    let mut report=json!({"from":from,"to":to,"estimated":summary.daily.iter().map(|r|r.estimated).sum::<i64>(),"sections":[
        section("Daily",DAILY_COLUMNS.to_vec(),summary.daily.iter().map(daily_cells).collect(),summary_total(&summary.daily)),
        section("Payments",payment_columns,summary.payments.iter().map(daily_cells).collect(),summary_total(&summary.payments)),
        parent_section(),
        section("Categories (Expense)",vec!["Expense category","Records","Total expense"],expenses,expense_total),
        json!({
            "title":"Cash Drawer Balance","columns":["Description","Amount"],
            "rows":[["Received at checkout", &crate::format_ks(summary.received_at_checkout)],
                    ["Recorded expenses", &crate::format_ks(recorded_expenses)]],
            "total":["Cash Drawer Balance", &crate::format_ks(summary.received_at_checkout-recorded_expenses)],
            "note":"Calculated as received at checkout minus recorded expenses. Includes non-cash payments."
        })
    ]});
    let today=chrono::Local::now().date_naive().to_string();
    portrait_report(&mut report,from==today && to==today);
    report
}

fn portrait_report(report:&mut serde_json::Value, limit_today_rows:bool) {
    report["portrait"]=true.into();
    let sections=report["sections"].as_array_mut().expect("report sections");
    for (section, (keep, limit)) in sections.iter_mut().zip([
        (&[0,1,2,4,7,8][..],1),
        (&[0,1,2][..],4),
        (&[0,1,5,6,7][..],8),
        (&[0,1,2][..],6),
        (&[0,1][..],2),
    ]) {
        let select=|cells:&serde_json::Value| serde_json::Value::Array(keep.iter().map(|&i|cells[i].clone()).collect());
        section["columns"]=select(&section["columns"]);
        section["total"]=select(&section["total"]);
        let rows=section["rows"].as_array_mut().expect("report rows");
        let count=rows.len();
        if limit_today_rows {rows.truncate(limit);}
        for row in rows {*row=select(row);}
        if limit_today_rows && count>limit {
            section["title"]=format!("{} ({} of {} rows; total includes all)",section["title"].as_str().unwrap_or(""),limit,count).into();
        }
    }
}

#[cfg(test)]
mod portrait_report_tests {
    use super::*;
    fn with_runtime<T>(action:impl FnOnce()->T)->T {
        let mut dom=VirtualDom::new(||rsx!{div {}});
        dom.rebuild_in_place();
        dom.in_scope(ScopeId::APP,action)
    }
    fn sample(count:usize)->sale_summary::Summary {
        let daily=(0..count).map(|i|sale_summary::SummaryRow {
            label:(chrono::NaiveDate::from_ymd_opt(2026,10,1).unwrap()+chrono::Duration::days(i as i64)).to_string(),
            receipts:2,sales:25000.0,gross:25000.0,cost:5000.0,..Default::default()
        }).collect();
        sale_summary::Summary {
            daily,payments:vec![sale_summary::SummaryRow{label:"Cash".into(),receipts:2*count as i64,sales:25000.0*count as f64,gross:25000.0*count as f64,..Default::default()}],
            groups:vec![vec![],vec![sale_summary::GroupRow{label:"Service Group".into(),receipts:2*count as i64,net:25000.0*count as f64,gross:25000.0*count as f64,cost:5000.0*count as f64,..Default::default()}]],
            group_receipts:vec![0,2*count as i64],
            expenses:vec![sale_summary::ExpenseRow{label:"ရုံးအသုံးစရိတ်".into(),records:1,amount:2500.0}],
            received_at_checkout:25000.0*count as f64,..Default::default()
        }
    }
    #[test]
    fn historical_ranges_use_portrait_columns_without_losing_rows_or_totals() {
      with_runtime(|| {
        for count in [1,60] {
            let report=full_report("2026-10-01","2026-11-29",&sample(count));
            assert_eq!(report["portrait"],true);
            assert_eq!(report["sections"][0]["rows"].as_array().unwrap().len(),count);
            assert_eq!(report["sections"][0]["total"][1],(count*2).to_string());
            assert_eq!(report["sections"][0]["total"][2],crate::format_ks(count as f64*25000.0));
            assert_eq!(report["sections"].as_array().unwrap().iter().map(|s|s["columns"].as_array().unwrap().len()).collect::<Vec<_>>(),vec![6,3,5,3,2]);
        }
      });
    }
    #[test]
    fn today_keeps_its_existing_one_page_compact_policy() {
      with_runtime(|| {
        let today=chrono::Local::now().date_naive().to_string();
        let report=full_report(&today,&today,&sample(2));
        assert_eq!(report["portrait"],true);
        assert_eq!(report["sections"][0]["rows"].as_array().unwrap().len(),1);
        assert_eq!(report["sections"][0]["total"][1],"4");
      });
    }
    #[test]
    #[ignore="writes synthetic native PDF verification payloads to KAY_SUMMARY_QA_DIR"]
    fn write_portrait_qa_payloads() {
      with_runtime(|| {
        let directory=std::path::PathBuf::from(std::env::var("KAY_SUMMARY_QA_DIR").expect("QA directory"));
        std::fs::create_dir_all(&directory).unwrap();
        for (name,count,to) in [("historical-day",1,"2026-10-01"),("historical-range",60,"2026-11-29")] {
            let mut report=full_report("2026-10-01",to,&sample(count));
            report["mode"]="pdf".into();report["path"]=directory.join(format!("{name}.pdf")).to_string_lossy().into_owned().into();
            std::fs::write(directory.join(format!("{name}.json")),serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
      });
    }
}

async fn output_report(mut report:serde_json::Value,pdf:bool)->anyhow::Result<String>{
    #[cfg(not(windows))]
    { let _=(report,pdf); anyhow::bail!("Sale Summary output requires Windows"); }
    #[cfg(windows)]
    {
        let destination=if pdf {
            let name=format!("Sale Summary {} to {}.pdf",report["from"].as_str().unwrap_or(""),report["to"].as_str().unwrap_or(""));
            let Some(file)=rfd::AsyncFileDialog::new().set_file_name(name).add_filter("PDF",&["pdf"]).save_file().await else {
                return Ok("PDF export cancelled.".into());
            };
            Some(file.path().to_path_buf())
        }else{None};
        report["mode"]=if pdf {"pdf"}else{"print"}.into();
        let path=destination.clone();
        let sent=tokio::task::spawn_blocking(move ||crate::warranty_form::send_document(report,path,include_str!("../assets/sale-summary-print.ps1"))).await??;
        Ok(if !sent {"Print cancelled.".into()}else if let Some(path)=destination {
            format!("Sale Summary PDF saved: {}",path.display())
        }else{"Sale Summary report sent to printer.".into()})
    }
}
