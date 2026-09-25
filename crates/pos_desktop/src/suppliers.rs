use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{
    connect,
    suppliers::{self, Supplier},
};

fn fields(s: &Supplier) -> Vec<(&'static str, String)> {
    vec![
        ("Supplier name", s.name.clone()),
        ("Company", s.company_name.clone()),
        ("Contact person", s.contact_person.clone()),
        ("Phone", s.phone.clone()),
        ("Email", s.email.clone()),
        ("Address", s.address.clone()),
        ("Tax number", s.tax_number.clone()),
        ("Website", s.website.clone()),
        ("Payment terms", s.payment_terms.clone()),
        ("Bank account", s.bank_account.clone()),
    ]
}

#[component]
pub fn SuppliersPage(db_form: DbForm, on_sales: EventHandler<()>) -> Element {
    let source = db_form.clone();
    let mut loaded = use_resource(move || {
        let form = source.clone();
        async move {
            let result =
                async { suppliers::list(&connect(&form.database_config()?).await?).await }.await;
            result.map_err(|err: anyhow::Error| format!("{err:#}"))
        }
    });
    let mut query = use_signal(String::new);
    let mut filter = use_signal(String::new);
    let mut applied = use_signal(|| (String::new(), String::new()));
    let mut page = use_signal(|| 0usize);
    let mut selected = use_signal(|| None::<Supplier>);
    let mut editor = use_signal(|| None::<Supplier>);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let state = loaded.read();
    let (search, status) = applied();
    let search = search.trim().to_lowercase();
    let rows = match state.as_ref() {
        Some(Ok(rows)) => rows
            .iter()
            .filter(|s| {
                (status.is_empty() || s.status == status)
                    && format!("{} {} {}", s.name, s.company_name, s.phone)
                        .to_lowercase()
                        .contains(&search)
            })
            .cloned()
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    let pages = rows.len().div_ceil(50).max(1);
    let current = page().min(pages - 1);
    rsx! {
        section { class: "customers_page customers_touch suppliers_touch",
            button { hidden:true, "data-page-refresh":"true", disabled:!loaded.finished(), onclick:move |_|loaded.restart() }
            header { class: "customers_header customers_touch_header",
                h2 { crate::icons::ActionLabel { label:"Suppliers" } }
                div { class: "customers_actions",
                    button { class: "customer_primary", onclick: move |_| editor.set(Some(Supplier { status: "Active".into(), ..Default::default() })), crate::icons::ActionLabel { label:"+ Add supplier" } }
                }
            }
            div { class: "customers_touch_search",
                label { "Search" input { placeholder: "Name, company or phone", value: "{query}", oninput: move |event|query.set(event.value()),
                    onkeydown: move |event| { if event.key()==Key::Enter { applied.set((query(),filter())); selected.set(None); page.set(0); } }
                } }
                label { "Status" select { value: "{filter}", onchange: move |event|filter.set(event.value()),
                    option { value: "", "All suppliers" } option { "Active" } option { "Inactive" }
                } }
                button { class: "customer_primary", onclick: move |_| { applied.set((query(),filter())); selected.set(None); page.set(0); }, "Search" }
                button { onclick: move |_| { query.set(String::new()); filter.set(String::new()); applied.set(Default::default()); selected.set(None); page.set(0); loaded.restart(); }, "Reset" }
            }
            if !notice().is_empty() { p { role: "status", "{notice}" } }
            if let Some(Err(message))=state.as_ref() { p { role: "alert", "{message}" } }
            div { class: "customers_touch_columns",
                section { class: "customers_touch_list",
                    h3 { "Supplier list" }
                    p { class: "customer_list_count", "{rows.len()} suppliers · Page {current+1} of {pages}" }
                    for supplier in rows.iter().skip(current*50).take(50) {
                        button { key: "{supplier.id}", class: if selected().map(|s|s.id)==Some(supplier.id) { "customer_list_item selected" } else { "customer_list_item" },
                            aria_pressed: selected().map(|s|s.id)==Some(supplier.id),
                            onclick: { let supplier=supplier.clone(); move |_|selected.set(Some(supplier.clone())) },
                            span { class: "customer_avatar", {supplier.name.chars().next().unwrap_or('?').to_string()} }
                            span { class: "customer_list_identity", strong { "{supplier.name}" } small { {if !supplier.company_name.is_empty() { supplier.company_name.clone() } else if !supplier.phone.is_empty() { supplier.phone.clone() } else { "No contact details".into() }} } }
                            span { "{supplier.status}" }
                        }
                    }
                    if state.is_none() { p { "Loading..." } } else if rows.is_empty() { p { "No suppliers found." } }
                    footer { class: "customers_pagination",
                        button { disabled: current==0, onclick: move |_|page.set(current.saturating_sub(1)), "Previous" }
                        button { disabled: current+1>=pages, onclick: move |_|page.set(current+1), "Next" }
                    }
                }
                aside { class: "customer_detail supplier_detail",
                    if let Some(supplier)=selected() {
                        header { class: "customer_detail_header", div { h3 { "{supplier.name}" } small { "{supplier.status} · Supplier #{supplier.id}" } } }
                        dl { class: "customer_contact",
                            for (label,value) in fields(&supplier).into_iter().skip(1) { dt { "{label}" } dd { if value.is_empty() { "—" } else { "{value}" } } }
                        }
                        button { class: "customer_primary", onclick: move |_|editor.set(selected()), "Edit supplier" }
                    } else { p { class: "customer_detail_empty", "Select a supplier to view details." } }
                }
            }
        }
        if let Some(supplier)=editor() {
            SupplierEditor { supplier, db_form,
                on_close: move |_|editor.set(None),
                on_saved: move |_| { editor.set(None); selected.set(None); notice.set("Supplier saved.".into()); loaded.restart(); },
                on_error: move |message|error.set(message)
            }
        }
        if !error().is_empty() { MessageBox { title: "Supplier Error".to_string(), message: error(), on_close: move |_|error.set(String::new()) } }
    }
}

#[component]
fn SupplierEditor(
    supplier: Supplier,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut form = use_signal(|| supplier.clone());
    let mut saving = use_signal(|| false);
    rsx! {
        div { class: "modal_backdrop",
            section { class: "customer_dialog supplier_dialog", role: "dialog", aria_modal: "true", aria_label: "Supplier",
                h2 { if supplier.id==0 { "Add supplier" } else { "Edit supplier" } }
                div { class: "customer_form",
                    for (label,value) in fields(&form()) {
                        label { span { "{label}" }
                            input { value: "{value}", disabled: saving(),
                                onmounted: move |event|async move { if label=="Supplier name" { let _=event.set_focus(true).await; } },
                                oninput: move |event| { let value=event.value(); let mut s=form.write(); match label {
                                    "Supplier name"=>s.name=value, "Company"=>s.company_name=value, "Contact person"=>s.contact_person=value, "Phone"=>s.phone=value, "Email"=>s.email=value, "Address"=>s.address=value, "Tax number"=>s.tax_number=value, "Website"=>s.website=value, "Payment terms"=>s.payment_terms=value, _=>s.bank_account=value,
                                } }
                            }
                        }
                    }
                    label { "Status" select { value: "{form().status}", disabled: saving(), onchange: move |event|form.write().status=event.value(), option { "Active" } option { "Inactive" } } }
                }
                div { class: "customers_actions",
                    button { disabled: saving(), onclick: move |_|on_close.call(()), crate::icons::ActionLabel { label:"Cancel" } }
                    button { class: "customer_primary", disabled: saving() || form().name.trim().is_empty(), onclick: move |_| {
                        if saving() { return; } let supplier=form(); let source=db_form.clone(); saving.set(true);
                        spawn(async move {
                            let result=async { suppliers::save(&connect(&source.database_config()?).await?,&supplier).await }.await;
                            saving.set(false); match result { Ok(())=>on_saved.call(()), Err(err)=>on_error.call(format!("{err:#}")) }
                        });
                    }, if saving() { "Saving..." } else { "Save supplier" } }
                }
            }
        }
    }
}
