use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{
    connect,
    customers::{self, Customer},
};

#[component]
pub fn CustomersPage(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let connection = db_form.clone();
    let mut loaded = use_resource(move || {
        let form = connection.clone();
        async move {
            let result =
                async { customers::list(&connect(&form.database_config()?).await?).await }.await;
            result.map_err(|err: anyhow::Error| format!("{err:#}"))
        }
    });
    let mut query = use_signal(String::new);
    let mut applied_query = use_signal(String::new);
    let mut page = use_signal(|| 0usize);
    let mut selected = use_signal(|| None::<Customer>);
    let mut editor = use_signal(|| None::<Customer>);
    let mut error = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut more = use_signal(|| false);
    let mut action = use_signal(String::new);
    let mut exporting = use_signal(|| false);
    let mut points_history=use_signal(||None::<Customer>);
    let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
    let state = loaded.read();
    let search = applied_query().trim().to_lowercase();
    let rows = match state.as_ref() {
        Some(Ok(rows)) => rows
            .iter()
            .filter(|c| {
                format!("{} {} {}", c.name, c.phone, c.email)
                    .to_lowercase()
                    .contains(&search)
            })
            .cloned()
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let pages = rows.len().div_ceil(25).max(1);
    let current = page().min(pages - 1);
    rsx! {
        section { class: "customers_page customers_touch",
            button { hidden:true, "data-page-refresh":"true", disabled:!loaded.finished(), onclick:move |_|loaded.restart() }
            header { class: "customers_header customers_touch_header",
                div { small { class: "customer_eyebrow", "CUSTOMER MANAGEMENT" } h2 { crate::icons::ActionLabel { label:"Customers" } } }
                div { class: "customers_actions",
                    button { class: "customer_primary", onclick: move |_| editor.set(Some(Customer::default())), crate::icons::ActionLabel { label:"+ Add customer" } }
                    div { class: "customer_more", onkeydown: move |event| { if event.key() == Key::Escape { more.set(false); } },
                        button { aria_expanded: more(), onclick: move |_| more.set(!more()), "More..." }
                        if more() {
                            div { class: "customer_menu_dismiss", onclick: move |_| more.set(false) }
                            div { class: "customer_more_menu",
                                button { disabled: selected().is_none(), onclick: move |_| { more.set(false); editor.set(selected()); }, crate::icons::ActionLabel { label:"Edit" } }
                                for label in ["Credit Sale", "Payment Collection", "Ledger", "Outstanding Report", "Delete"] {
                                    button { disabled: label != "Outstanding Report" && selected().is_none(), onclick: move |_| { more.set(false); action.set(label.into()); }, crate::icons::ActionLabel {label} }
                                }
                                button { disabled: exporting() || !matches!(state.as_ref(), Some(Ok(_))), onclick: {
                                    let rows = rows.clone();
                                    move |_| {
                                        more.set(false); exporting.set(true);
                                        let rows = rows.clone();
                                        spawn(async move {
                                            if let Some(file) = rfd::AsyncFileDialog::new().add_filter("Excel", &["xlsx"]).set_file_name("Customers.xlsx").save_file().await {
                                                match export_customers(&rows, file.path()) { Ok(()) => status.set(format!("Exported to {}", file.path().display())), Err(err) => error.set(format!("Export failed: {err:#}")) }
                                            }
                                            exporting.set(false);
                                        });
                                    }
                                }, crate::icons::ActionLabel { label:"Export Excel" } }
                            }
                        }
                    }
                }
            }
            div { class: "customers_touch_search",
                label { "Search customers"
                    input { r#type: "search", placeholder: "Customer name, phone or email", value: "{query}",
                        oninput: move |event| query.set(event.value()),
                        onkeydown: move |event| { if event.key()==Key::Enter { applied_query.set(query()); page.set(0); selected.set(None); } }
                    }
                }
                button { class: "customer_primary", onclick: move |_| { applied_query.set(query()); page.set(0); selected.set(None); }, "Search" }
                button { onclick: move |_| { query.set(String::new()); applied_query.set(String::new()); page.set(0); selected.set(None); loaded.restart(); }, "Reset" }
            }
            if let Some(Err(message)) = state.as_ref() {
                div { role: "alert", class: "customers_notice", "{message}" }
            }
            if !status().is_empty() { div { role: "status", class: "customers_notice", "{status}" } }
            div { class: "customers_touch_columns",
                section { class: "customers_touch_list",
                    h3 { crate::icons::ActionLabel { label:"Customers" } }
                    p { class: "customer_list_count", "{rows.len()} customers · Page {current + 1} of {pages}" }
                        for customer in rows.iter().skip(current * 25).take(25) {
                            button { key: "{customer.id}", class: if selected().as_ref().map(|c| c.id) == Some(customer.id) { "customer_list_item selected" } else { "customer_list_item" },
                                aria_pressed: selected().as_ref().map(|c| c.id) == Some(customer.id),
                                onclick: { let customer = customer.clone(); move |_| selected.set(Some(customer.clone())) },
                                span { class: "customer_avatar", {customer.name.chars().next().unwrap_or('?').to_string()} }
                                span { class: "customer_list_identity", strong { "{customer.name}" } small { "{contact(&customer.phone)} · #{customer.id}" } }
                                span { class: "customer_list_balance", strong { "{crate::format_ks(customer.current_balance)}" }
                                    small { class: if customer.current_balance>0.0 { "customer_balance_due" } else { "customer_settled" }, if customer.current_balance>0.0 { "Outstanding" } else { "No balance" } }
                                }
                            }
                        }
                if state.is_none() { p { class: "customers_notice", "Loading..." } }
                else if rows.is_empty() { p { class: "customers_notice", "No customers found." } }
            footer { class: "customers_pagination",
                button { disabled: current == 0, onclick: move |_| page.set(current.saturating_sub(1)), "Previous" }
                span { "Page {current + 1} of {pages}" }
                button { disabled: current + 1 >= pages, onclick: move |_| page.set(current + 1), "Next" }
            }
                }
                aside { class: "customer_detail",
                    if let Some(customer)=selected() {
                        header { class: "customer_detail_header",
                            span { class: "customer_avatar", {customer.name.chars().next().unwrap_or('?').to_string()} }
                            div { h3 { "{customer.name}" } small { "Customer #{customer.id}" } }
                        }
                        dl { class: "customer_contact",
                            dt { "Phone" } dd { "{contact(&customer.phone)}" }
                            dt { "Email" } dd { "{contact(&customer.email)}" }
                            dt { "Address" } dd { "{contact(&customer.address)}" }
                            dt { "Remarks" } dd { "{contact(&customer.remarks)}" }
                        }
                        dl { class: "customer_metrics",
                            div { dt { "Current balance" } dd { "{crate::format_ks(customer.current_balance)}" } }
                            div { dt { "Credit limit" } dd { "{crate::format_ks(customer.credit_limit)}" } }
                            div { dt { "Available credit" } dd { if customer.credit_limit<=0.0 { "Unlimited" } else { "{crate::format_ks((customer.credit_limit-customer.current_balance).max(0.0))}" } } }
                            div { dt { "Points" } dd { "{customer.points}" } }
                            div { dt { "Visits" } dd { "{customer.total_visit}" } }
                            div { dt { "Total spent" } dd { "{crate::format_ks(customer.total_spent)}" } }
                        }
                        div { class: "customer_detail_actions",
                            button { onclick: move |_| points_history.set(selected()), crate::icons::ActionLabel { label:"Points history" } }
                            button { onclick: move |_| action.set("Ledger".into()), crate::icons::ActionLabel { label:"View ledger" } }
                            button { class: "customer_primary", onclick: move |_| action.set("Payment Collection".into()), "Payment Collection" }
                            button { onclick: move |_| editor.set(selected()), crate::icons::ActionLabel { label:"Edit" } }
                            button { class: "customer_delete_action", onclick: move |_| action.set("Delete".into()), crate::icons::ActionLabel { label:"Delete" } }
                        }
                    } else {
                        p { class: "customer_detail_empty", "Select a customer to view details." }
                    }
                }
            }
        }
        if let Some(customer) = editor() {
            CustomerEditor { customer, db_form: db_form.clone(),
                on_close: move |_| editor.set(None),
                on_saved: move |_| { editor.set(None); selected.set(None); status.set("Customer saved.".into()); loaded.restart(); },
                on_error: move |message| error.set(message)
            }
        }
        if !action().is_empty() {
            CustomerAction { action: action(), customer: selected(), db_form: db_form.clone(),
                on_close: move |_| action.set(String::new()),
                on_saved: move |_| { action.set(String::new()); selected.set(None); status.set("Customer transaction saved.".into()); loaded.restart(); },
                on_error: move |message| error.set(message)
            }
        }
        if let Some(c)=points_history(){if let Some(actor)=session(){crate::loyalty::History{db_form:db_form.clone(),actor,customer_id:c.id,name:c.name,onclose:move |_|points_history.set(None)}}}
        if !error().is_empty() { MessageBox { title: "Customer Error".to_string(), message: error(), on_close: move |_| error.set(String::new()) } }
    }
}

fn contact(value: &str) -> &str {
    if value.trim().is_empty() {
        "—"
    } else {
        value
    }
}

fn export_customers(rows: &[Customer], path: &std::path::Path) -> anyhow::Result<()> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    for (col, header) in [
        "Name",
        "Phone",
        "Email",
        "Address",
        "Visits",
        "Spent",
        "Points",
        "Credit Limit",
        "Balance",
        "Remarks",
    ]
    .iter()
    .enumerate()
    {
        sheet.write_string(0, col as u16, *header)?;
        sheet.set_column_width(col as u16, 22)?;
    }
    for (row, customer) in rows.iter().enumerate() {
        let row = row as u32 + 1;
        for (col, text) in [
            &customer.name,
            &customer.phone,
            &customer.email,
            &customer.address,
        ]
        .iter()
        .enumerate()
        {
            sheet.write_string(row, col as u16, text.as_str())?;
        }
        for (col, value) in [
            customer.total_visit as f64,
            customer.total_spent,
            customer.points as f64,
            customer.credit_limit,
            customer.current_balance,
        ]
        .iter()
        .enumerate()
        {
            sheet.write_number(row, col as u16 + 4, *value)?;
        }
        sheet.write_string(row, 9, &customer.remarks)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    workbook.save(path)?;
    Ok(())
}

#[component]
fn CustomerAction(
    action: String,
    customer: Option<Customer>,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    use pos_core::customer_credit;
    let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
    let customer_id = if action == "Outstanding Report" {
        None
    } else {
        customer.as_ref().map(|c| c.id)
    };
    let save_action = action.clone();
    let source = db_form.clone();
    let data = use_resource(move || {
        let form = source.clone();
        async move {
            let result = async {
                let pool = connect(&form.database_config()?).await?;
                let invoices = customer_credit::invoices(&pool, customer_id).await?;
                let payments = if let Some(id) = customer_id {
                    customer_credit::payments(&pool, id).await?
                } else {
                    vec![]
                };
                let methods = pos_core::db::list_payment_types(&pool).await?;
                Ok::<_, anyhow::Error>((invoices, payments, methods))
            }
            .await;
            result.map_err(|err| format!("{err:#}"))
        }
    });
    let mut fields = use_signal(|| {
        let today = chrono::Local::now().date_naive();
        std::collections::HashMap::<String, String>::from([
            (
                "Invoice".into(),
                format!("CR{}", chrono::Local::now().format("%Y%m%d%H%M%S%f")),
            ),
            ("Date".into(), today.to_string()),
            (
                "Due date".into(),
                (today + chrono::Duration::days(15)).to_string(),
            ),
            ("Total".into(), String::new()),
            ("Paid".into(), "0".into()),
        ])
    });
    let mut saving = use_signal(|| false);
    let mut invoice_id = use_signal(|| 0i32);
    let mut method = use_signal(String::new);
    let mut from = use_signal(String::new);
    let mut to = use_signal(String::new);
    let report = action == "Ledger" || action == "Outstanding Report";
    let labels: Vec<&str> = match action.as_str() {
        "Credit Sale" => vec!["Invoice", "Total", "Paid", "Date", "Due date", "Notes"],
        "Payment Collection" => vec!["Amount", "Date", "Reference", "Notes"],
        "Delete" => vec!["Admin username", "Admin password"],
        _ => vec![],
    };
    let state = data.read();
    rsx! {
        div { class: "modal_backdrop",
            section { class: if report { "customer_dialog customer_report" } else { "customer_dialog" }, role: "dialog", aria_modal: "true", aria_label: "{action}",
                h2 { "{action}" }
                if action != "Outstanding Report" {
                    if let Some(customer) = &customer { p { "{customer.name}" } }
                }
                if action == "Delete" { p { "Permanently delete this customer?" } }
                if let Some(Err(message)) = state.as_ref() { p { role: "alert", "{message}" } }
                if state.is_none() { p { "Loading..." } }
                if report {
                    div { class: "customer_form",
                        label { "From" input { r#type: "date", value: "{from}", oninput: move |event| from.set(event.value()) } }
                        label { "To" input { r#type: "date", value: "{to}", oninput: move |event| to.set(event.value()) } }
                    }
                    if let Some(Ok((invoices,payments,_))) = state.as_ref() {
                        div { class: "customers_table_scroll",
                            table { class: "customers_table",
                                thead { tr { for title in ["Customer", "Invoice", "Date", "Due", "Total", "Paid", "Balance", "Status"] { th { "{title}" } } } }
                                tbody {
                                    for row in invoices.iter().filter(|row| (action != "Outstanding Report" || (row.balance > Default::default() && row.status != "refunded")) && (from().is_empty() || row.date >= from()) && (to().is_empty() || row.date.get(..10).unwrap_or(&row.date) <= to().as_str())) {
                                        tr { td { "{row.customer}" } td { "{row.invoice}" } td { "{row.date}" } td { "{row.due}" } td { "{row.total}" } td { "{row.paid}" } td { "{row.balance}" } td { "{row.status}" } }
                                    }
                                }
                            }
                        }
                        if action == "Ledger" {
                            h3 { "Payments & Adjustments" }
                            table { class: "customers_table",
                                thead { tr { for title in ["Date", "Invoice", "Amount", "Method", "Reference", "Note"] { th { "{title}" } } } }
                                tbody { for row in payments.iter().filter(|row| (from().is_empty() || row.date >= from()) && (to().is_empty() || row.date.get(..10).unwrap_or(&row.date) <= to().as_str())) {
                                    tr { td { "{row.date}" } td { "{row.invoice}" } td { "{row.amount}" } td { "{row.method}" } td { "{row.reference}" } td { "{row.note}" } }
                                } }
                            }
                        }
                    }
                } else {
                    div { class: "customer_form",
                        if action == "Payment Collection" {
                            if let Some(Ok((invoices,_,methods))) = state.as_ref() {
                                label { "Invoice" select { value: "{invoice_id}", disabled: saving(), onchange: move |event| invoice_id.set(event.value().parse().unwrap_or(0)),
                                    option { value: "0", "Oldest outstanding invoices" }
                                    for row in invoices.iter().filter(|row| row.balance > Default::default() && row.status != "refunded") { option { value: "{row.id}", "{row.invoice} - {row.balance}" } }
                                } }
                                label { "Payment method" select { value: "{method}", disabled: saving(), onchange: move |event| method.set(event.value()),
                                    option { value: "", "Select" }
                                    for item in methods { option { value: "{item.name}", "{item.name}" } }
                                } }
                            }
                        }
                        for label in labels {
                            label { span { "{label}" }
                                input { r#type: match label { "Date" | "Due date" => "date", "Admin password" => "password", _ => "text" }, disabled: saving(),
                                    value: fields.read().get(label).cloned().unwrap_or_default(),
                                    oninput: move |event| { fields.write().insert(label.into(),event.value()); }
                                }
                            }
                        }
                    }
                }
                div { class: "customers_actions",
                    button { disabled: saving(), onclick: move |_| on_close.call(()), crate::icons::ActionLabel { label:"Close" } }
                    if !report {
                        button { class: "customer_primary", disabled: saving() || !matches!(state.as_ref(),Some(Ok(_))), onclick: move |_| {
                            if saving() { return; }
                            let Some(id) = customer_id else { return; };
                            let values = fields(); let form = db_form.clone(); let kind = save_action.clone(); let invoice = invoice_id(); let method = method();
                            saving.set(true);
                            spawn(async move {
                                let get = |key: &str| values.get(key).map(String::as_str).unwrap_or("");
                                let result = async {
                                    let pool = connect(&form.database_config()?).await?;
                                    match kind.as_str() {
                                        "Credit Sale" => customer_credit::create(&pool,&session().ok_or_else(||anyhow::anyhow!("Sign in again"))?,id,get("Invoice"),get("Total"),get("Paid"),get("Date"),get("Due date"),get("Notes")).await,
                                        "Payment Collection" => customer_credit::collect(&pool,&session().ok_or_else(||anyhow::anyhow!("Sign in again"))?,id,if invoice == 0 { None } else { Some(invoice) },get("Amount"),get("Date"),&method,get("Reference"),get("Notes")).await,
                                        "Delete" => customer_credit::delete(&pool,id,get("Admin username"),get("Admin password")).await,
                                        _ => Ok(()),
                                    }
                                }.await;
                                saving.set(false);
                                match result { Ok(()) => on_saved.call(()), Err(err) => on_error.call(format!("{err:#}")) }
                            });
                        }, if saving() { "Saving..." } else if action == "Delete" { "Delete Customer" } else { crate::icons::ActionLabel { label:"Save" } } }
                    }
                }
            }
        }
    }
}

#[component]
fn CustomerEditor(
    customer: Customer,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut form = use_signal(|| customer.clone());
    let mut limit = use_signal(|| customer.credit_limit.to_string());
    let mut saving = use_signal(|| false);
    rsx! {
        div { class: "modal_backdrop",
            section { class: "customer_dialog", role: "dialog", aria_modal: "true", aria_label: "Customer",
                h2 { if customer.id == 0 { "Add Customer" } else { "Edit Customer" } }
                div { class: "customer_form",
                    for (key, label, value) in [("name", "Name *", form().name), ("phone", "Phone", form().phone), ("email", "Email", form().email), ("address", "Address", form().address)] {
                        label { span { "{label}" }
                            input { value: "{value}", disabled: saving(),
                                onmounted: move |event| async move { if key == "name" { let _ = event.set_focus(true).await; } },
                                oninput: move |event| { let mut data = form.write(); match key { "name" => data.name = event.value(), "phone" => data.phone = event.value(), "email" => data.email = event.value(), _ => data.address = event.value() } }
                            }
                        }
                    }
                    label { span { "Credit Limit" } input { r#type: "number", min: "0", step: "0.01", value: "{limit}", disabled: saving(), oninput: move |event| limit.set(event.value()) } }
                    label { class: "customer_remarks", span { "Remarks" } textarea { value: "{form().remarks}", disabled: saving(), oninput: move |event| form.write().remarks = event.value() } }
                }
                div { class: "customers_actions",
                    button { disabled: saving(), onclick: move |_| on_close.call(()), crate::icons::ActionLabel { label:"Cancel" } }
                    button { class: "customer_primary", disabled: saving() || form().name.trim().is_empty(), onclick: move |_| {
                        let Ok(credit_limit) = limit().parse::<f64>() else { on_error.call("Enter a valid credit limit.".into()); return; };
                        if !credit_limit.is_finite() || credit_limit < 0.0 { on_error.call("Credit limit must be zero or greater.".into()); return; }
                        let mut customer = form(); customer.credit_limit = credit_limit;
                        let db_form = db_form.clone(); saving.set(true);
                        spawn(async move {
                            let result = async { customers::save(&connect(&db_form.database_config()?).await?, &customer).await }.await;
                            saving.set(false);
                            match result { Ok(()) => on_saved.call(()), Err(err) => on_error.call(format!("{err:#}")) }
                        });
                    }, if saving() { "Saving..." } else { "Save Customer" } }
                }
            }
        }
    }
}
