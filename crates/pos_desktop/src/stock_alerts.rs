use crate::DbForm;
use dioxus::prelude::*;
use pos_core::{connect, stock_alerts::{self, StockAlert}};

#[component]
pub fn StockAlerts(form: DbForm) -> Element {
    let mut rows = use_signal(Vec::<StockAlert>::new);
    let mut ready = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut open = use_signal(|| false);
    let mut query = use_signal(String::new);
    let mut severity = use_signal(String::new);
    let mut data = use_resource(use_reactive((&form,), move |(form,)| async move {
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            stock_alerts::list(&connect(&form.database_config()?).await?).await
        }).await;
        match result {
            Ok(Ok(next)) => {
                if *rows.peek()!=next { rows.set(next); }
                ready.set(true);
                error.set(String::new());
            }
            _ => error.set("Stock alerts unavailable. Check the database connection and refresh.".into()),
        }
    }));
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            if data.finished() { data.restart(); }
        }
    });
    let all = rows();
    let out = all.iter().filter(|r|r.stock<=0.0).count();
    let low = all.len()-out;
    let search=query().to_lowercase();
    let filtered:Vec<_>=all.iter().filter(|r| {
        (severity().is_empty() || (severity()=="out")== (r.stock<=0.0))
            && format!("{} {}",r.name,r.sku).to_lowercase().contains(&search)
    }).collect();
    rsx! {
        button { class:"status_refresh stock_alert_trigger",title:"Stock notifications",aria_label:"Stock notifications",
            onclick:move |_|{open.set(true);if data.finished(){data.restart();}},
            if !error().is_empty(){"Stock alerts unavailable"}
            else if !ready(){"Checking stock..."}
            else {span {aria_live:"polite","Low {low} · Out {out}"}}
        }
        if open(){div {class:"modal_backdrop",
            section {class:"stock_alert_dialog",role:"dialog",aria_modal:"true",aria_label:"Stock notifications",
                header {h2 {"Stock notifications"}
                    button {aria_label:"Close stock notifications",title:"Close",onclick:move |_|open.set(false),"×"}
                }
                div {class:"stock_alert_filters",
                    input {aria_label:"Search stock alerts",placeholder:"Product, variant or SKU",value:query(),oninput:move |e|query.set(e.value())}
                    select {aria_label:"Stock status",value:severity(),onchange:move |e|severity.set(e.value()),
                        option {value:"","All ({all.len()})"} option {value:"low","Low stock ({low})"} option {value:"out","Out of stock ({out})"}
                    }
                    button { hidden:true, "data-page-refresh":"true", tabindex:-1, aria_hidden:"true",disabled:!data.finished(),onclick:move |_|data.restart(),"Refresh"}
                }
                if !error().is_empty(){p {role:"alert","{error}"}}
                div {class:"stock_alert_list",
                    table {thead {tr {th {"Product / Variant"} th {"SKU"} th {"Stock"} th {"Low at"} th {"Status"}}}
                        tbody {for row in filtered.iter(){tr {key:"{row.product_id}-{row.variant_id:?}",
                            td {"{row.name}"} td {"{row.sku}"} td {"{row.stock}"} td {"{row.threshold}"}
                            td {span {class:if row.stock<=0.0{"stock_alert_out"}else{"stock_alert_low"},if row.stock<=0.0{"Out of stock"}else{"Low stock"}}}
                        }}}
                    }
                    if ready()&&error().is_empty()&&filtered.is_empty(){p {"No stock alerts"}}
                }
            }
        }}
    }
}
