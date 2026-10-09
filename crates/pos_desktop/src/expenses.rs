use crate::{DbForm, MessageBox};
use dioxus::prelude::*;
use pos_core::{
    connect,
    expenses::{self, Expense},
    models::Money,
    PaymentType,
};

fn new_record() -> Expense {
    Expense {
        expense_no: format!("EXP{}", chrono::Local::now().format("%Y%m%d%H%M%S%f")),
        expense_date: chrono::Local::now().format("%Y-%m-%d").to_string(),
        ..Default::default()
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn category_totals(rows: &[Expense]) -> Vec<(String, Money, usize)> {
    let mut groups = std::collections::BTreeMap::<String, (Money, usize)>::new();
    for row in rows {
        let entry = groups.entry(row.category.clone()).or_default();
        entry.0 += row.amount;
        entry.1 += 1;
    }
    let mut totals: Vec<_> = groups.into_iter().map(|(name, (amount, count))| (name, amount, count)).collect();
    totals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    totals
}

#[component]
pub fn QuickExpense(db_form: DbForm, on_close: EventHandler<()>, on_saved: EventHandler<()>) -> Element {
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let source = source.clone();
        async move {
            async {
                let pool = connect(&source.database_config()?).await?;
                Ok::<_, anyhow::Error>((expenses::categories(&pool).await?, pos_core::db::list_payment_types(&pool).await?))
            }.await.map_err(|e| format!("{e:#}"))
        }
    });
    let mut error = use_signal(String::new);
    let expense = use_signal(new_record);
    rsx! {
        match data.read().as_ref() {
            Some(Ok((categories, methods))) => rsx! {
                ExpenseEditor { expense: expense(), categories: categories.clone(), methods: methods.clone(), db_form,
                    on_close, on_saved, on_error: move |message| error.set(message)
                }
            },
            state => rsx! {
                div { class: "modal_backdrop",
                    section { class: "customer_dialog", role: "dialog", aria_modal: "true", aria_label: "Add expense",
                        h2 { "Add expense" }
                        if let Some(Err(message)) = state {
                            p { role: "alert", "{message}" }
                            button { onclick: move |_| data.restart(), crate::icons::ActionLabel { label:"Retry" } }
                        } else { p { role: "status", "Loading..." } }
                        button { onclick: move |_| on_close.call(()), crate::icons::ActionLabel { label:"Cancel" } }
                    }
                }
            }
        }
        if !error().is_empty() { MessageBox { title: "Expense Error".to_string(), message: error(), on_close: move |_| error.set(String::new()) } }
    }
}

#[component]
pub fn ExpensesPage(
    db_form: DbForm,
    new_expense: Signal<bool>,
    on_sales: EventHandler<()>,
) -> Element {
    let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
    let Some(actor)=session() else {return rsx!{}};
    let mut tab=use_signal(||0usize);
    use_effect(move||{if new_expense(){tab.set(0)}});
    rsx!{section{class:"customers_page phase4_page phase4_expenses",
        nav{class:"receipt_tabs",aria_label:"Expense views",
            for (i,label) in ["Expenses","Budgets","Alerts","Comparison"].iter().enumerate(){button{role:"tab",aria_selected:(tab()==i).to_string(),class:if tab()==i{"active"}else{""},onclick:move |_|tab.set(i),"{label}"}}
        }
        if tab()==0 {ExpenseHistory{db_form,new_expense,on_sales}}
        else if tab()==3 {crate::expense_comparison::ExpenseComparison{db_form,actor}}
        else {crate::expense_management::BudgetPanel{key:"{tab()}",db_form,actor,alerts:tab()==2}}
    }}
}

#[component]
fn ExpenseHistory(
    db_form: DbForm,
    mut new_expense: Signal<bool>,
    on_sales: EventHandler<()>,
) -> Element {
    let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
    let source = db_form.clone();
    let mut data = use_resource(move || {
        let form = source.clone();
        async move {
            let result = async {
                let pool = connect(&form.database_config()?).await?;
                Ok::<_, anyhow::Error>((
                    expenses::list(&pool,&session().ok_or_else(||anyhow::anyhow!("Sign in again"))?).await?,
                    expenses::categories(&pool).await?,
                    pos_core::db::list_payment_types(&pool).await?,
                ))
            }
            .await;
            result.map_err(|err| format!("{err:#}"))
        }
    });
    let mut query = use_signal(String::new);
    let mut category = use_signal(String::new);
    let mut category_view = use_signal(|| false);
    let mut detail_category = use_signal(|| None::<String>);
    let mut from = use_signal(today);
    let mut to = use_signal(today);
    let mut page = use_signal(|| 0usize);
    let mut selected = use_signal(|| None::<Expense>);
    let mut editor = use_signal(|| None::<Expense>);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let mut exporting = use_signal(|| false);
    let mut deleting = use_signal(|| None::<Expense>);
    let mut busy = use_signal(|| false);
    let mut applied = use_signal(|| { let date = today(); (date.clone(), date, String::new(), String::new()) });
    let mut attachments=use_signal(||None::<i32>);
    use_effect(move || {
        if new_expense() {
            editor.set(Some(new_record()));
            new_expense.set(false);
        }
    });
    use_effect(move || {
        let q = query(); let c = category();
        let a=from(); let b=to();
        if !a.is_empty() && !b.is_empty() && a>b { return; }
        let values=(a,b,q,c);
        if *applied.peek()!=values { applied.set(values); page.set(0); selected.set(None); detail_category.set(None); }
    });
    let state = data.read();
    let (all, categories, methods) = match state.as_ref() {
        Some(Ok(data)) => data.clone(),
        _ => (vec![], vec![], vec![]),
    };
    let (filter_from, filter_to, filter_query, filter_category) = applied();
    let search = filter_query.to_lowercase();
    let invalid_range = !from().is_empty() && !to().is_empty() && from() > to();
    let rows = all
        .iter()
        .filter(|row| {
            (filter_category.is_empty() || filter_category == row.category)
                && (filter_from.is_empty() || row.expense_date >= filter_from)
                && (filter_to.is_empty() || row.expense_date <= filter_to)
                && format!(
                    "{} {} {} {}",
                    row.expense_no, row.description, row.reference_no, row.category
                )
                .to_lowercase()
                .contains(&search)
        })
        .cloned()
        .collect::<Vec<_>>();
    let total: Money = rows.iter().map(|row| row.amount).sum();
    let category_summary = category_totals(&rows);
    let largest_category = category_summary.first().map(|row| row.1).unwrap_or(Money::ZERO);
    let average = if rows.is_empty() {
        Money::ZERO
    } else {
        total / Money::from(rows.len() as u64)
    };
    let pages = rows.len().div_ceil(25).max(1);
    let current = page().min(pages - 1);
    let mut filter_categories = categories.clone();
    filter_categories.extend(all.iter().map(|row| row.category.clone()));
    filter_categories.sort();
    filter_categories.dedup();
    rsx! {
        section { class: "customers_page expense_touch",
            button { hidden:true, "data-page-refresh":"true", disabled:!data.finished(), onclick:move |_|data.restart() }
            div { class: "expense_touch_summary",
                div { span { "Total spending" } strong { "{money(total)} {crate::regional::current().symbol()}" } }
                div { span { crate::icons::ActionLabel { label:"Expenses" } } strong { "{rows.len()}" } }
                div { span { "Average expense" } strong { "{money(average)} {crate::regional::current().symbol()}" } }
            }
            div { class: "expense_filters expense_touch_filters",
                crate::date_range::DateRangePicker { allow_empty:true, from:from(), to:to(), onchange:move |(a,b):(String,String)|{from.set(a);to.set(b);page.set(0);selected.set(None);} }
                label { "Category"
                    select { aria_label: "Expense category", value: "{category}", onchange: move |event| category.set(event.value()),
                        option { value: "", "All categories" }
                        for name in filter_categories.iter().filter(|name| !name.trim().is_empty()) {
                            option { value: "{name}", "{name}" }
                        }
                    }
                }
                label { "Search" input { r#type: "search", placeholder: "Category, description or reference", value: "{query}", oninput: move |event| query.set(event.value()) } }
                button { onclick: move |_| { let date=today(); from.set(date.clone()); to.set(date.clone()); query.set(String::new()); category.set(String::new()); applied.set((date.clone(),date,String::new(),String::new())); page.set(0); selected.set(None); data.restart(); }, "Reset" }
            }
            if invalid_range { div { role: "alert", class: "customers_notice", "From date must not be after To date." } }
            if let Some(Err(message)) = state.as_ref() { div { role: "alert", class: "customers_notice", "{message}" } }
            if !notice().is_empty() { div { role: "status", class: "customers_notice", "{notice}" } }
            div { class: "expense_results_toolbar",
            nav { class: "receipt_tabs expense_view_tabs", role: "tablist", aria_label: "Expense results",
                button { role: "tab", id: "expense-category-tab", aria_controls: "expense-category-panel", aria_selected: category_view().to_string(), class: if category_view() { "active" } else { "" }, onclick: move |_| category_view.set(true), "Spending by Category" }
                button { role: "tab", id: "expense-history-tab", aria_controls: "expense-history-panel", aria_selected: (!category_view()).to_string(), class: if !category_view() { "active" } else { "" }, onclick: move |_| category_view.set(false), "Expense History" }
            }
                div { class: "customers_actions",
                    button { disabled: exporting() || !matches!(state.as_ref(),Some(Ok(_))), onclick: {
                        let rows=rows.clone(); move |_| { let rows=rows.clone(); exporting.set(true); spawn(async move {
                            if let Some(file)=rfd::AsyncFileDialog::new().add_filter("Excel", &["xlsx"]).set_file_name("Expenses.xlsx").save_file().await {
                                match export(&rows,file.path()) { Ok(())=>notice.set(format!("Exported to {}",file.path().display())), Err(err)=>error.set(format!("{err:#}")) }
                            } exporting.set(false);
                        }); }
                    }, crate::icons::ActionLabel { label:"Export Excel" } }
                    button { class: "customer_primary", onclick: move |_| editor.set(Some(new_record())), crate::icons::ActionLabel { label:"+ Add expense" } }
                }
            }
            if category_view() {
            div {class:"expense_category_columns",id:"expense-category-panel",role:"tabpanel",aria_labelledby:"expense-category-tab",
            section { class: "expense_category_chart",
                header {
                    div { strong { "Spending by category" } small { "Totals for the applied date range and filters · Select a category to view its expenses" } }
                    if !filter_category.is_empty() {
                        button { onclick: move |_| {
                            category.set(String::new());
                            let (a,b,q,_) = applied();
                            applied.set((a,b,q,String::new()));
                            page.set(0); selected.set(None);
                        }, "All categories" }
                    }
                }
                div { class: "expense_category_bars",
                    for (name, amount, count) in &category_summary {
                        {
                            let width = if largest_category > Money::ZERO { (*amount / largest_category * Money::from(100)).round_dp(2) } else { Money::ZERO };
                            let share = if total > Money::ZERO { (*amount / total * Money::from(100)).round_dp(1).normalize() } else { Money::ZERO };
                            rsx! { button { class: if detail_category().as_ref()==Some(name) {"expense_category_bar active"}else{"expense_category_bar"},aria_pressed:(detail_category().as_ref()==Some(name)).to_string(),"data-search-value":name.clone(),"data-search-detail":format!("{} · {} expenses",amount,count), onclick: {
                                let name=name.clone(); move |_| {
                                    detail_category.set(Some(name.clone()));
                                    page.set(0); selected.set(None);
                                }
                            },
                                span { class: "expense_category_name", "{name}" }
                                span { class: "expense_category_track", aria_hidden: "true", span { style: "width:{width}%" } }
                                span { class: "expense_category_value", strong { "{money(*amount)} {crate::regional::current().symbol()}" } small { "{count} expenses · {share}%" } }
                            } }
                        }
                    }
                    if category_summary.is_empty() { p { class: "customers_notice", "No expenses for the applied filters." } }
                }
            }
            section {class:"expense_category_details panel",aria_label:"Category expense details",
                if let Some(name)=detail_category().filter(|name| rows.iter().any(|row| &row.category==name)) {
                    {
                        let matching=rows.iter().filter(|row|row.category==name).collect::<Vec<_>>();
                        let amount:Money=matching.iter().map(|row|row.amount).sum();
                        let detail_pages=matching.len().div_ceil(25).max(1);
                        let detail_page=page().min(detail_pages-1);
                        rsx! {
                            header {class:"transactions_head",
                                div {strong {"{name}"} small {"{filter_from} – {filter_to} · {matching.len()} expenses"}}
                                button {aria_label:"Close category details",title:"Close category details",onclick:move |_|{detail_category.set(None);page.set(0);},crate::icons::Icon {name:"close"}}
                            }
                            div {class:"expense_category_detail_total",span {"Category total"} strong {"{money(amount)} {crate::regional::current().symbol()}"}}
                            div {class:"expense_category_detail_list",
                                for row in matching.iter().skip(detail_page*25).take(25) {
                                    details {class:"expense_category_detail_record",key:"{row.id}",
                                        summary {div {strong {"{row.description}"} small {"{row.expense_date} · {row.expense_no}"}} strong {"{money(row.amount)} {crate::regional::current().symbol()}"}}
                                        div {class:"expense_detail_body",
                                            dl {
                                                div {dt {"Payment method"} dd {"{row.payment_method}"}}
                                                div {dt {"Reference"} dd {if row.reference_no.is_empty(){"-"}else{"{row.reference_no}"}}}
                                                div {dt {"Notes"} dd {if row.notes.is_empty(){"-"}else{"{row.notes}"}}}
                                            }
                                            div {class:"expense_category_record_actions",
                                                button {onclick:{let id=row.id;move |_|attachments.set(Some(id))},crate::icons::ActionLabel {label:"Attachments"}}
                                                button {onclick:{let row=(*row).clone();move |_|editor.set(Some(row.clone()))},crate::icons::ActionLabel {label:"Edit"}}
                                            }
                                        }
                                    }
                                }
                            }
                            if detail_pages>1 {footer {class:"customers_pagination",
                                button { title:"Previous page", aria_label:"Previous page",disabled:detail_page==0,onclick:move |_|page.set(detail_page.saturating_sub(1)),crate::icons::Icon { name:"arrow_circle_left" }}
                                span {"Page {detail_page+1} of {detail_pages}"}
                                button { title:"Next page", aria_label:"Next page",disabled:detail_page+1>=detail_pages,onclick:move |_|page.set(detail_page+1),crate::icons::Icon { name:"arrow_circle_right" }}
                            }}
                        }
                    }
                } else {div {class:"empty receipts_empty",strong {"Select a category"} span {"Its expenses and category total will appear here."}}}
            }
            }
            } else {
            div { class: "expense_columns", id: "expense-history-panel", role: "tabpanel", aria_labelledby: "expense-history-tab",
                section { class: "expense_list_panel panel",
                    div { class: "transactions_head",
                        div { strong { "Expense history" } small { "Select an expense to view details" } }
                        span { class: "count_badge", "{rows.len()}" }
                    }
                    div { class: "expense_touch_list",
                        for row in rows.iter().skip(current*25).take(25) {
                            article {
                                class: if selected().as_ref().is_some_and(|item|item.id==row.id) { "expense_touch_item active" } else { "expense_touch_item" },
                                "data-search-value":row.category.clone(),"data-search-detail":format!("{} · {} · {}",row.description,row.expense_date,money(row.amount)),"data-search-match":format!("{} {} {}",row.category,row.description,row.reference_no),
                                key: "{row.id}",
                                tabindex: "0",
                                role: "button",
                                onclick: { let row=row.clone(); move |_| selected.set(Some(row.clone())) },
                                div { class: "expense_date_badge", strong { {row.expense_date.get(8..10).unwrap_or("--")} } small { {row.expense_date.get(..7).unwrap_or("")} } }
                                div { class: "expense_item_text",
                                    span { class: "expense_item_category", "{row.category}" }
                                    strong { "{row.description}" }
                                    small { "{row.expense_no} · {row.reference_no}" }
                                }
                                div { class: "expense_item_amount", strong { "{money(row.amount)}" } small { "{crate::regional::current().symbol()}" } span { "{row.payment_method}" } }
                            }
                        }
                        if state.is_none() { p { class: "customers_notice", "Loading..." } }
                        else if rows.is_empty() { p { class: "customers_notice", "No expenses found." } }
                    }
                    footer { class: "customers_pagination",
                        button { title:"Previous page", aria_label:"Previous page", disabled: current==0, onclick: move |_| { page.set(current.saturating_sub(1)); selected.set(None); }, crate::icons::Icon { name:"arrow_circle_left" } }
                        span { "Page {current+1} of {pages}" }
                        button { title:"Next page", aria_label:"Next page", disabled: current+1>=pages, onclick: move |_| { page.set(current+1); selected.set(None); }, crate::icons::Icon { name:"arrow_circle_right" } }
                    }
                }
                if let Some(row)=selected() {
                    section { class: "expense_detail_panel panel",
                        div { class: "receipt_detail_head",
                            div { strong { "{row.expense_no}" } small { "{row.expense_date} · {row.category}" } }
                            button { onclick: move |_| selected.set(None), crate::icons::ActionLabel { label:"Close" } }
                        }
                        div { class: "expense_detail_body",
                            div { class: "expense_detail_amount", span { "Amount" } strong { "{money(row.amount)} {crate::regional::current().symbol()}" } }
                            dl {
                                div { dt { "Description" } dd { "{row.description}" } }
                                div { dt { "Payment method" } dd { "{row.payment_method}" } }
                                div { dt { "Reference" } dd { if row.reference_no.is_empty() { "-" } else { "{row.reference_no}" } } }
                                div { dt { "Notes" } dd { if row.notes.is_empty() { "-" } else { "{row.notes}" } } }
                            }
                        }
                        div { class: "receipt_detail_actions expense_detail_actions",
                            button { onclick: { let id=row.id; move |_| attachments.set(Some(id)) }, crate::icons::ActionLabel { label:"Attachments" } }
                            button { onclick: { let row=row.clone(); move |_| editor.set(Some(row.clone())) }, crate::icons::ActionLabel { label:"Edit" } }
                            button { class: "expense_delete", onclick: { let row=row.clone(); move |_| deleting.set(Some(row.clone())) }, crate::icons::ActionLabel { label:"Delete" } }
                        }
                    }
                } else {
                    section { class: "expense_detail_panel panel",
                        div { class: "empty receipts_empty",
                            strong { "Select an expense" }
                            span { "Expense details will appear here." }
                        }
                    }
                }
            }
            }
        }
        if let Some(expense)=editor() {
            ExpenseEditor { expense, categories, methods, db_form: db_form.clone(),
                on_close: move |_| editor.set(None),
                on_saved: move |_| { editor.set(None); selected.set(None); notice.set("Expense saved.".into()); data.restart(); },
                on_error: move |message| error.set(message)
            }
        }
        if let Some(row)=deleting() {
            div { class: "modal_backdrop",
                section { class: "customer_dialog", role: "dialog", aria_modal: "true", aria_label: "Delete expense",
                    h2 { "Delete expense?" }
                    p { "{row.expense_no} · {row.category} · {money(row.amount)} {crate::regional::current().symbol()}" }
                    div { class: "customers_actions",
                        button { disabled: busy(), onclick: move |_| deleting.set(None), crate::icons::ActionLabel { label:"Cancel" } }
                        button { disabled: busy(), onclick: {let db_form=db_form.clone();move |_| {
                            if busy() { return; } let source=db_form.clone(); busy.set(true);
                            spawn(async move {
                                let result=async { expenses::delete(&connect(&source.database_config()?).await?,row.id,&session().ok_or_else(||anyhow::anyhow!("Sign in again"))?).await }.await;
                                busy.set(false);
                                match result { Ok(())=> { deleting.set(None); notice.set("Expense deleted.".into()); data.restart(); }, Err(err)=>error.set(format!("{err:#}")) }
                            });
                        }}, if busy() { "Deleting..." } else { crate::icons::ActionLabel { label:"Delete" } } }
                    }
                }
            }
        }
        if let Some(id)=attachments(){
            if let Some(actor)=session(){crate::expense_management::Attachments{db_form:db_form.clone(),actor,expense_id:id,onclose:move |_|attachments.set(None)}}
        }
        if !error().is_empty() { MessageBox { title: "Expense Error".to_string(), message: error(), on_close: move |_| error.set(String::new()) } }
    }
}

pub(crate) fn money(amount: Money) -> String {
    let text = amount.round_dp(2).normalize().to_string();
    let (whole, decimal) = text.split_once('.').unwrap_or((&text, ""));
    let mut result = String::new();
    for (index, ch) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    if !decimal.is_empty() {
        result.push('.');
        result.push_str(decimal);
    }
    result
}

#[component]
fn ExpenseEditor(
    expense: Expense,
    categories: Vec<String>,
    methods: Vec<PaymentType>,
    db_form: DbForm,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
    on_error: EventHandler<String>,
) -> Element {
    let mut form = use_signal(|| expense.clone());
    let mut amount = use_signal(|| {
        if expense.id == 0 {
            String::new()
        } else {
            expense.amount.to_string()
        }
    });
    let mut saving = use_signal(|| false);
    let session=use_context::<Signal<Option<pos_core::auth::Session>>>();
    rsx! {
        div { class: "modal_backdrop",
            section { class: "customer_dialog", role: "dialog", aria_modal: "true", aria_label: "Expense",
                header {class:"expense_dialog_heading",
                    h2 { if expense.id==0 { "Add Expense" } else { "Edit Expense" } }
                    span {class:"expense_dialog_number","{expense.expense_no}"}
                }
                div { class: "customer_form",
                    label { "Category" select { value: "{form().category}", disabled: saving(), onchange: move |event| form.write().category=event.value(),
                        option { value: "", "Select category" }
                        if !expense.category.is_empty() && !categories.contains(&expense.category) { option { value: "{expense.category}", "{expense.category}" } }
                        for category in categories { option { value: "{category}", "{category}" } }
                    } }
                    label { "Date" input { r#type: "date", value: "{form().expense_date}", disabled: saving(), oninput: move |event| form.write().expense_date=event.value() } }
                    label { "Amount" input { r#type: "number", min: "0.01", step: "0.01", value: "{amount}", disabled: saving(), oninput: move |event| amount.set(event.value()) } }
                    label { "Payment method" select { value: "{form().payment_method}", disabled: saving(), onchange: move |event| form.write().payment_method=event.value(),
                        option { value: "", "Select payment" }
                        if !expense.payment_method.is_empty() && !methods.iter().any(|method|method.name==expense.payment_method) { option { value: "{expense.payment_method}", "{expense.payment_method}" } }
                        for method in methods { option { value: "{method.name}", "{method.name}" } }
                    } }
                    label { "Description" input { value: "{form().description}", disabled: saving(), oninput: move |event| form.write().description=event.value() } }
                    label { "Reference" input { value: "{form().reference_no}", disabled: saving(), oninput: move |event| form.write().reference_no=event.value() } }
                    label { class: "customer_remarks", "Notes" textarea { value: "{form().notes}", disabled: saving(), oninput: move |event| form.write().notes=event.value() } }
                }
                div { class: "customers_actions",
                    button { disabled: saving(), onclick: move |_| on_close.call(()), crate::icons::ActionLabel { label:"Cancel" } }
                    button { class: "customer_primary", disabled: saving(), onclick: move |_| {
                        if saving() { return; }
                        let mut expense=form();
                        match amount().parse::<Money>() { Ok(value)=>expense.amount=value, Err(_)=> { on_error.call("Enter a valid amount.".into()); return; } }
                        if let Err(err)=expenses::validate(&expense) { on_error.call(err.to_string()); return; }
                        let source=db_form.clone(); saving.set(true);
                        spawn(async move {
                            let result=async { expenses::save(&connect(&source.database_config()?).await?,&expense,&session().ok_or_else(||anyhow::anyhow!("Sign in again"))?).await }.await;
                            saving.set(false);
                            match result { Ok(())=>on_saved.call(()), Err(err)=>on_error.call(format!("{err:#}")) }
                        });
                    }, if saving() { "Saving..." } else { crate::icons::ActionLabel {label:"Save Expense"} } }
                }
            }
        }
    }
}

#[cfg(test)]
mod category_summary_tests {
    use super::*;

    #[test]
    fn combines_repeated_categories_and_sorts_by_total() {
        let rows = [("Raw materials", 20000), ("Other", 5000), ("Raw materials", 100000)]
            .into_iter().map(|(category, amount)| Expense { category:category.into(), amount:Money::from(amount), ..Default::default() }).collect::<Vec<_>>();
        assert_eq!(category_totals(&rows), vec![("Raw materials".into(), Money::from(120000), 2), ("Other".into(), Money::from(5000), 1)]);
        assert!(category_totals(&[]).is_empty());
    }
}

fn export(rows: &[Expense], path: &std::path::Path) -> anyhow::Result<()> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let sheet = workbook.add_worksheet();
    for (col, name) in [
        "Expense No.",
        "Date",
        "Category",
        "Description",
        "Amount",
        "Payment",
        "Reference",
        "Notes",
    ]
    .iter()
    .enumerate()
    {
        sheet.write_string(0, col as u16, *name)?;
        sheet.set_column_width(col as u16, 24)?;
    }
    for (index, row) in rows.iter().enumerate() {
        let r = index as u32 + 1;
        for (col, value) in [
            &row.expense_no,
            &row.expense_date,
            &row.category,
            &row.description,
            &row.amount.to_string(),
            &row.payment_method,
            &row.reference_no,
            &row.notes,
        ]
        .iter()
        .enumerate()
        {
            if col == 4 {
                sheet.write_number(r, col as u16, value.parse::<f64>()?)?;
            } else {
                sheet.write_string(r, col as u16, value.as_str())?;
            }
        }
    }
    sheet.set_freeze_panes(1, 0)?;
    workbook.save(path)?;
    Ok(())
}
