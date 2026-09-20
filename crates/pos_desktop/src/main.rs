#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use dioxus::prelude::*;
mod auth;
mod service_orders;
mod app_icon;
mod appearance_colors;
mod cash_drawer;
mod categories;
mod customer_display;
mod customers;
mod expenses;
mod inventory;
mod locations;
mod pending_checkout;
mod printer_settings;
mod products;
mod receipt_printer;
mod receipt_layout;
mod receipt_preview;
mod sale_summary;
mod status_bar;
mod stock_alerts;
mod suppliers;
mod users;
mod updates;
use dioxus_desktop::{Config, LogicalSize, WindowBuilder};
use pos_core::{
    complete_sale, connect,
    db::{
        delete_payment_type, list_categories, list_payment_types, list_settings, save_payment_type,
    },
    get_receipt_detail, list_receipts, refund_sale, DatabaseConfig, PaymentType, Product,
    ProductPriceTier, ProductVariant, ReceiptDetail, SaleDraft, SaleItemDraft, SaleSummary,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf};
use tokio::time::{sleep, Duration};

const APP_CSS: &str = include_str!("../assets/app.css");
const PRODUCT_BATCH_SIZE: usize = 60;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct CartLine {
    product: Product,
    variant: Option<ProductVariant>,
    qty: f64,
    unit_price_override: Option<f64>,
}

#[derive(Clone, PartialEq)]
struct ReceiptData {
    invoice_no: String,
    lines: Vec<CartLine>,
    subtotal: f64,
    discount: f64,
    total: f64,
    payment_type: String,
    payment: f64,
    change: f64,
}

#[derive(Clone, PartialEq)]
struct UiCategory {
    id: Option<i32>,
    name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceView {
    ServiceOrders,
    SaleSummary,
    Inventory,
    Locations,
    Categories,
    Products,
    Suppliers,
    Expenses,
    Customers,
    Sales,
    Receipts,
    Settings,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct DbForm {
    host: String,
    port: String,
    database: String,
    username: String,
    password: String,
}

impl Default for DbForm {
    fn default() -> Self {
        Self {
            host: "192.168.110.112".to_string(),
            port: "5432".to_string(),
            database: "zay_pos".to_string(),
            username: "zay_pos_user".to_string(),
            password: String::new(),
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--update-self-check") {
        std::process::exit(if args.get(2).is_some_and(|v| v == updates::VERSION) { 0 } else { 1 });
    }
    load_env_files();
    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new().with_menu(None).with_window(
                WindowBuilder::new()
                    .with_window_icon(Some(app_icon::icon()))
                    .with_title("KAY POS Rust")
                    .with_inner_size(LogicalSize::new(1366.0, 768.0))
                    .with_min_inner_size(LogicalSize::new(1280.0, 720.0)),
            ),
        )
        .launch(auth::Root);
}

#[component]
fn App() -> Element {
    let mut session = use_context::<Signal<Option<pos_core::auth::Session>>>();
    let actor = session
        .read()
        .clone()
        .expect("App is mounted only after login");
    let can_manage = actor.allows(pos_core::auth::Permission::Manage);
    let instance = use_hook(|| {
        std::rc::Rc::new(
            pending_checkout::acquire_instance(&load_saved_db_form()).map_err(|e| format!("{e:#}")),
        )
    });
    let mut pending = use_signal(|| match instance.as_ref() {
        Ok(_) => pending_checkout::load(&load_saved_db_form()).map_err(|e| format!("{e:#}")),
        Err(e) => Err(e.clone()),
    });
    let mut customer_display_enabled = use_signal(|| true);
    let mut drawer_busy = use_signal(|| false);
    use_effect(|| {
        let _ = document::eval(&format!(
            "{}\n{}",
            include_str!("../assets/touch-scanner.js"),
            include_str!("../assets/sales-scanner.js")
        ));
    });
    use_effect(|| {
        let script = include_str!("../assets/touch-combobox.js")
            .replace("document.addEventListener('DOMContentLoaded', () => {", "((ready) => ready())(() => {")
            .replace("attributeFilter: ['disabled']", "attributeFilter: ['disabled', 'selected', 'value']")
            .replace("input.select();\n      render(select, input.value);", "input.select();\n      render(select, '');")
            .replace("input.placeholder =", "input.setAttribute('role', 'combobox'); input.setAttribute('aria-autocomplete', 'list'); input.setAttribute('aria-expanded', 'false'); input.placeholder =");
        let _ = document::eval(&format!(
            "if (!window.kayRustComboboxInstalled) {{ window.kayRustComboboxInstalled = true; {script}\n{}\n}}",
            include_str!("../assets/combobox-desktop.js")
        ));
    });
    let mut products = use_signal(Vec::<Product>::new);
    let mut categories = use_signal(Vec::<UiCategory>::new);
    let mut selected_category = use_signal(|| None::<i32>);
    let mut db_status = use_signal(|| "DB not checked".to_string());
    let mut checkout_status = use_signal(String::new);
    let mut checkout_saving = use_signal(|| false);
    let mut checkout_discount = use_signal(String::new);
    let mut checkout_customer = use_signal(|| None::<i32>);
    let mut checkout_sale_type = use_signal(|| "Cash".to_string());
    let mut show_checkout = use_signal(|| false);
    let mut receipt = use_signal(|| None::<ReceiptData>);
    let mut received = use_signal(String::new);
    let mut show_db_dialog = use_signal(|| false);
    let mut show_side_menu = use_signal(|| false);
    let db_form = use_signal(load_saved_db_form);
    use_context_provider(|| db_form);
    let mut query = use_signal(String::new);
    let mut category_query = use_signal(String::new);
    let mut cart = use_signal(Vec::<CartLine>::new);
    let mut message_box = use_signal(|| None::<String>);
    let mut active_view = use_signal(|| WorkspaceView::Sales);
    let mut new_expense = use_signal(|| false);
    let mut quick_expense = use_signal(|| false);
    let mut receipts = use_signal(Vec::<SaleSummary>::new);
    let mut selected_receipt = use_signal(|| None::<ReceiptDetail>);
    let mut receipts_status = use_signal(String::new);
    let mut receipt_search = use_signal(String::new);
    let mut receipt_tab = use_signal(|| "receipts".to_string());
    let mut receipt_from = use_signal(current_receipt_date);
    let mut receipt_to = use_signal(current_receipt_date);
    let mut settings_query = use_signal(String::new);
    let mut settings_page = use_signal(|| "overview".to_string());
    let mut app_settings = use_signal(HashMap::<String, String>::new);
    let mut payment_types = use_signal(Vec::<PaymentType>::new);
    let mut selected_payment_id = use_signal(|| None::<i32>);
    let mut payment_form_name = use_signal(String::new);
    let mut payment_status = use_signal(String::new);
    let mut visible_product_limit = use_signal(|| PRODUCT_BATCH_SIZE);
    let mut product_cards_loading = use_signal(|| false);
    let mut service_product = use_signal(|| None::<Product>);
    let mut service_price = use_signal(String::new);
    let mut variant_product = use_signal(|| None::<Product>);
    let mut selected_variant_id = use_signal(|| None::<i32>);

    use_effect(move || {
        db_status.set("Auto connecting to PostgreSQL...".to_string());
        product_cards_loading.set(true);
        let form = db_form.read().clone();
        spawn(async move {
            match load_startup_data_from_database(form).await {
                Ok((loaded_categories, loaded_products, loaded_settings, loaded_payment_types)) => {
                    categories.set(loaded_categories);
                    products.set(loaded_products);
                    app_settings.set(loaded_settings);
                    payment_types.set(loaded_payment_types);
                    selected_category.set(None);
                    cart.set(Vec::new());
                    visible_product_limit.set(PRODUCT_BATCH_SIZE);
                    db_status.set("Connected".to_string());
                    product_cards_loading.set(false);
                }
                Err(error) => {
                    let message = format!("Auto connect failed: {error:#}");
                    db_status.set(message.clone());
                    message_box.set(Some(message));
                    product_cards_loading.set(false);
                }
            }
        });
    });

    let filtered_products = {
        let query = query.read().to_lowercase();
        let selected_category_id = *selected_category.read();
        products
            .read()
            .iter()
            .filter(|product| {
                let category_matches = selected_category_id
                    .map(|category_id| product.category_id == Some(category_id))
                    .unwrap_or(true);
                category_matches
                    && (query.is_empty()
                        || product.name.to_lowercase().contains(&query)
                        || product
                            .sku
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query)
                        || product
                            .barcode
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query))
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let visible_products = filtered_products
        .iter()
        .take(*visible_product_limit.read())
        .cloned()
        .collect::<Vec<_>>();
    let visible_product_count = visible_products.len();
    let filtered_product_count = filtered_products.len();
    let has_more_products = visible_product_count < filtered_product_count;

    let total = cart.read().iter().map(line_total).sum::<f64>();
    let cart_item_count = cart.read().iter().map(|line| line.qty).sum::<f64>();
    let filtered_categories = {
        let query = category_query.read().to_lowercase();
        categories
            .read()
            .iter()
            .filter(|category| query.is_empty() || category.name.to_lowercase().contains(&query))
            .cloned()
            .collect::<Vec<_>>()
    };
    let open_cash_drawer = use_callback(move |_: ()| {
        if drawer_busy() { return; }
                    let printer = app_settings
                        .read()
                        .get("receipt_printer_name")
                        .cloned()
                        .unwrap_or_default();
                    drawer_busy.set(true);
                    spawn(async move {
                        match cash_drawer::open(printer).await {
                            Ok(()) => checkout_status
                                .set("Drawer command sent. Check the physical drawer.".into()),
                            Err(error) => message_box.set(Some(format!("Cash drawer: {error:#}"))),
                        }
                        sleep(Duration::from_secs(1)).await;
                        drawer_busy.set(false);
                    });
    });
    use_future(move || async move {
        let mut listener = document::eval(include_str!("../assets/shortcuts.js"));
        while let Ok(action) = listener.recv::<String>().await {
            match action.as_str() {
                "customer-display" => customer_display_enabled.set(!customer_display_enabled()),
                "cash-drawer" => open_cash_drawer.call(()),
                _ => {}
            }
        }
    });
    let display_settings = app_settings.read();
    let display_cart = cart.read();
    let display_subtotal = display_cart.iter().map(line_total).sum::<f64>();
    let display_discount = if show_checkout() {
        checkout_discount()
            .parse::<f64>()
            .unwrap_or(0.0)
            .clamp(0.0, display_subtotal)
    } else {
        checkout_default_discount(&display_settings, display_subtotal)
    };
    let display_total = (display_subtotal - display_discount).max(0.0)
        + checkout_tax(&display_settings, display_subtotal - display_discount);
    let display_receipt = receipt.read();
    let display_lines = display_receipt
        .as_ref()
        .map(|r| &r.lines)
        .unwrap_or(&display_cart);
    let customer_display_visible = customer_display::use_customer_display(
        customer_display::DisplaySnapshot {
            shop: setting_value(&display_settings, "shop_name", "KAY POS"),
            lines: display_lines
                .iter()
                .map(|line| customer_display::DisplayLine {
                    name: line.display_name(),
                    quantity: format_qty(line.qty),
                    amount: format_ks(line_total(line)),
                })
                .collect(),
            total: format_ks(
                display_receipt
                    .as_ref()
                    .map(|r| r.total)
                    .unwrap_or(display_total),
            ),
            received: display_receipt
                .as_ref()
                .map(|r| format_ks(r.payment))
                .or_else(|| show_checkout().then(|| format_ks(received().parse().unwrap_or(0.0)))),
            change: display_receipt
                .as_ref()
                .map(|r| format_ks(r.change))
                .or_else(|| {
                    show_checkout().then(|| {
                        format_ks(
                            (received().parse::<f64>().unwrap_or(0.0) - display_total).max(0.0),
                        )
                    })
                }),
            completed: display_receipt.is_some(),
        },
        customer_display_enabled,
    );
    drop(display_receipt);
    drop(display_cart);
    drop(display_settings);
    let category_tiles = filtered_categories
        .iter()
        .cloned()
        .map(|category| {
            let tone = category_tone(category.id, &category.name);
            (category, tone)
        })
        .collect::<Vec<_>>();

    rsx! {
        style { "{APP_CSS}" }
        main { class: "shell",
            "data-theme": appearance_mode(&app_settings.read()),
            style: appearance_colors::style(&app_settings.read()),
            if !matches!(&*pending.read(), Ok(None)) {
                pending_checkout::Recovery { pending, form: db_form.read().clone(), saving: checkout_saving,
                    on_resolved: move |saved| { cart.set(Vec::new()); show_checkout.set(false); receipt.set(saved); }
                }
            }
            header { class: "topbar",
                div { class: "top_identity",
                button {
                    class: "menu_button",
                    aria_label: "Open menu",
                    title: "Open menu",
                    onclick: move |_| show_side_menu.set(true),
                    "☰"
                }
                div { class: "top_shop_brand",
                    if let Some(logo) = app_settings.read().get("shop_logo_image").filter(|v| v.starts_with("data:image/")) {
                        img { class: "top_shop_logo", src: "{logo}", alt: "Shop logo" }
                    } else {
                        span { class: "top_shop_logo top_shop_logo_fallback", "K" }
                    }
                    span { class: "top_shop_name", title: "{setting_value(&app_settings.read(), \"shop_name\", \"KAY POS\")}",
                        "{setting_value(&app_settings.read(), \"shop_name\", \"KAY POS\")}"
                    }
                }
                }
                nav { class: "top_navigation", aria_label: "Quick navigation",
                    button { class: "top_nav_link", aria_current: if *active_view.read()==WorkspaceView::Sales {"page"}else{"false"}, onclick: move |_| { show_side_menu.set(false); active_view.set(WorkspaceView::Sales); }, "Sales" }
                    if can_manage { button { class: "top_nav_link", onclick: move |_| { show_side_menu.set(false); if *active_view.read()==WorkspaceView::Expenses {new_expense.set(true);}else{quick_expense.set(true);} }, "Add Expense" } }
                    button { class: "top_nav_link", disabled: drawer_busy(), title: "Open cashdrawer (Ctrl+Shift+D)", onclick: move |_| open_cash_drawer.call(()), if drawer_busy() {"Opening..."}else{"Open Cashdrawer"} }
                    button { class: "top_nav_link", title: "Customer display (Ctrl+Shift+C)", aria_pressed: customer_display_visible(),
                        onclick: move |_| {
                            if customer_display_visible() { customer_display_enabled.set(false); }
                            else {
                                let window = dioxus_desktop::window();
                                let primary = window.current_monitor().or_else(|| window.primary_monitor());
                                let available = window.available_monitors().any(|m| primary.as_ref().is_some_and(|p| p.position()!=m.position()));
                                if available { customer_display_enabled.set(true); }
                                else { message_box.set(Some("No secondary monitor detected. Connect an extended display first.".into())); }
                            }
                        },
                        if customer_display_visible() {"Close Customer Display"}else{"Show Customer Display"}
                    }
                }
                div { class: "top_actions",
                    button { class: "top_sign_out", disabled: checkout_saving(), onclick: move |_| session.set(None), "Sign out" }
                }
            }

            if quick_expense() && can_manage {
                expenses::QuickExpense { db_form: db_form.read().clone(),
                    on_close: move |_| quick_expense.set(false),
                    on_saved: move |_| { quick_expense.set(false); checkout_status.set("Expense saved.".into()); }
                }
            }
            if *show_side_menu.read() {
                div {
                    class: "side_overlay",
                    onclick: move |_| show_side_menu.set(false)
                }
                aside { class: "side_menu",
                    div { class: "side_head",
                        div { class: "brand_mark", "K" }
                        div {
                            strong { "KAY POS" }
                            small { "Rust client" }
                        }
                        button { class: "side_close", aria_label: "Close menu", title: "Close menu", onclick: move |_| show_side_menu.set(false), "×" }
                    }
                    div { class: "side_group",
                        span { "SALES" }
                        button {
                            class: if *active_view.read() == WorkspaceView::Sales { "side_active" } else { "" },
                            onclick: move |_| {
                                show_side_menu.set(false);
                                active_view.set(WorkspaceView::Sales);
                            },
                            "Sales"
                        }
                        button {
                            class: if *active_view.read()==WorkspaceView::Receipts {"side_active"}else{""},
                            onclick: move |_| {
                                show_side_menu.set(false);
                                active_view.set(WorkspaceView::Receipts);
                                receipts_status.set("Loading receipts...".to_string());
                                let form = db_form.read().clone();
                                spawn(async move {
                                    match load_receipts_from_database(form).await {
                                        Ok(rows) => {
                                            let first_id = rows.first().map(|row| row.id);
                                            receipts.set(rows);
                                            receipts_status.set("Receipts loaded".to_string());
                                            selected_receipt.set(None);
                                            if let Some(sale_id) = first_id {
                                                let form = db_form.read().clone();
                                                spawn(async move {
                                                    match load_receipt_detail_from_database(form, sale_id).await {
                                                        Ok(detail) => selected_receipt.set(Some(detail)),
                                                        Err(error) => {
                                                            let message = format!("Receipt detail error: {error:#}");
                                                            receipts_status.set(message.clone());
                                                            message_box.set(Some(message));
                                                        }
                                                    }
                                                });
                                            }
                                        }
                                        Err(error) => {
                                            let message = format!("Receipt load error: {error:#}");
                                            receipts_status.set(message.clone());
                                            message_box.set(Some(message));
                                        }
                                    }
                                });
                            },
                            "Receipts"
                        }
                        if can_manage { button { class: if *active_view.read()==WorkspaceView::Expenses {"side_active"}else{""}, onclick: move |_| { new_expense.set(false); active_view.set(WorkspaceView::Expenses); show_side_menu.set(false); }, "Expenses" } }
                        button { class: if *active_view.read()==WorkspaceView::ServiceOrders {"side_active"}else{""}, onclick:move |_|{active_view.set(WorkspaceView::ServiceOrders);show_side_menu.set(false);},"Service Order" }
                    }
                    if can_manage {
                    div { class: "side_group",
                        span { "PEOPLE" }
                        button {
                            class: if *active_view.read()==WorkspaceView::Customers {"side_active"}else{""},
                            onclick: move |_| { active_view.set(WorkspaceView::Customers); show_side_menu.set(false); },
                            "Customers"
                        }
                        button { class: if *active_view.read()==WorkspaceView::Suppliers {"side_active"}else{""}, onclick: move |_| { active_view.set(WorkspaceView::Suppliers); show_side_menu.set(false); }, "Suppliers" }
                    }
                    div { class: "side_group",
                        span { "PRODUCTS & STOCK" }
                        button { class: if *active_view.read()==WorkspaceView::Products {"side_active"}else{""}, onclick: move |_| { active_view.set(WorkspaceView::Products); show_side_menu.set(false); }, "Products" }
                        button { class: if *active_view.read()==WorkspaceView::Categories {"side_active"}else{""}, onclick: move |_| { active_view.set(WorkspaceView::Categories); show_side_menu.set(false); }, "Categories" }
                        button { class: if *active_view.read()==WorkspaceView::Locations {"side_active"}else{""}, onclick: move |_| { active_view.set(WorkspaceView::Locations); show_side_menu.set(false); }, "Locations" }
                        button { class: if *active_view.read()==WorkspaceView::Inventory {"side_active"}else{""}, onclick: move |_| { active_view.set(WorkspaceView::Inventory); show_side_menu.set(false); }, "Inventory" }
                    }
                    div { class: "side_group",
                        span { "REPORTS" }
                        button { class: if *active_view.read()==WorkspaceView::SaleSummary {"side_active"}else{""}, onclick: move |_| { active_view.set(WorkspaceView::SaleSummary); show_side_menu.set(false); }, "Sale Summary" }
                    }
                    if actor.allows(pos_core::auth::Permission::Admin) {
                    div { class: "side_group",
                        span { "SETTINGS" }
                        button {
                            class: if *active_view.read() == WorkspaceView::Settings { "side_active" } else { "" },
                            onclick: move |_| {
                                show_side_menu.set(false);
                                active_view.set(WorkspaceView::Settings);
                                let form = db_form.read().clone();
                                spawn(async move {
                                    match load_settings_center_data_from_database(form).await {
                                        Ok((loaded_settings, loaded_payment_types)) => {
                                            app_settings.set(loaded_settings);
                                            payment_types.set(loaded_payment_types);
                                        }
                                        Err(error) => {
                                            let message = format!("Settings load error: {error:#}");
                                            message_box.set(Some(message));
                                        }
                                    }
                                });
                            },
                            "Settings Center"
                        }
                    }
                    }
                    }
                }
            }

            if *show_db_dialog.read() && actor.allows(pos_core::auth::Permission::Admin) {
                DbSettingsDialog {
                    form: db_form.read().clone(),
                    on_cancel: move |_| show_db_dialog.set(false),
                    on_save: move |form: DbForm| {
                        if let Err(e) = save_db_form(&form) { message_box.set(Some(format!("{e:#}"))); return; }
                        session.set(None);
                    }
                }
            }

            if !auth::can_open(&actor, *active_view.read()) {
                p { role: "alert", "Your account does not have permission to open this page." }
            } else if *active_view.read() == WorkspaceView::ServiceOrders {
                service_orders::ServiceOrdersPage {db_form:db_form.read().clone()}
            } else if *active_view.read() == WorkspaceView::SaleSummary {
                sale_summary::SaleSummaryPage { db_form: db_form.read().clone(), on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Inventory {
                inventory::InventoryPage { db_form: db_form.read().clone(), on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Locations {
                locations::LocationsPage { db_form: db_form.read().clone(), on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Categories {
                categories::CategoriesPage { db_form: db_form.read().clone(), on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Products {
                products::ProductsPage { db_form: db_form.read().clone(), on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Suppliers {
                suppliers::SuppliersPage { db_form: db_form.read().clone(), on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Expenses {
                expenses::ExpensesPage { db_form: db_form.read().clone(), new_expense, on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Customers {
                customers::CustomersPage { db_form: db_form.read().clone(), on_sales: move |_| active_view.set(WorkspaceView::Sales) }
            } else if *active_view.read() == WorkspaceView::Settings {
                SettingsCenterPage {
                    query: settings_query.read().clone(),
                    active_page: settings_page.read().clone(),
                    db_form: db_form.read().clone(),
                    db_status: db_status.read().clone(),
                    settings: app_settings.read().clone(),
                    on_saved: move |values: HashMap<String, String>| app_settings.write().extend(values),
                    payment_types: payment_types.read().clone(),
                    selected_payment_id: *selected_payment_id.read(),
                    payment_form_name: payment_form_name.read().clone(),
                    payment_status: payment_status.read().clone(),
                    on_query: move |value| settings_query.set(value),
                    on_page: move |value| settings_page.set(value),
                    on_open_db: move |_| show_db_dialog.set(true),
                    on_back: move |_| active_view.set(WorkspaceView::Sales),
                    on_payment_select: move |selection: (i32, String)| {
                        selected_payment_id.set((selection.0 > 0).then_some(selection.0));
                        payment_form_name.set(selection.1);
                        payment_status.set(String::new());
                    },
                    on_payment_name: move |value| payment_form_name.set(value),
                    on_payment_add: move |_| {
                        let name = payment_form_name.read().trim().to_string();
                        if name.is_empty() {
                            message_box.set(Some("Payment method name is required.".to_string()));
                            return;
                        }
                        payment_status.set("Saving payment method...".to_string());
                        let form = db_form.read().clone();
                        spawn(async move {
                            match save_payment_type_in_database(form, None, name).await {
                                Ok(rows) => {
                                    payment_types.set(rows);
                                    selected_payment_id.set(None);
                                    payment_form_name.set(String::new());
                                    payment_status.set("Payment method added.".to_string());
                                }
                                Err(error) => {
                                    let message = format!("Payment add failed: {error:#}");
                                    payment_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    },
                    on_payment_update: move |_| {
                        let Some(payment_id) = *selected_payment_id.read() else {
                            message_box.set(Some("Select a payment method to edit.".to_string()));
                            return;
                        };
                        let name = payment_form_name.read().trim().to_string();
                        if name.is_empty() {
                            message_box.set(Some("Payment method name is required.".to_string()));
                            return;
                        }
                        payment_status.set("Updating payment method...".to_string());
                        let form = db_form.read().clone();
                        spawn(async move {
                            match save_payment_type_in_database(form, Some(payment_id), name).await {
                                Ok(rows) => {
                                    payment_types.set(rows);
                                    selected_payment_id.set(None);
                                    payment_form_name.set(String::new());
                                    payment_status.set("Payment method updated.".to_string());
                                }
                                Err(error) => {
                                    let message = format!("Payment edit failed: {error:#}");
                                    payment_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    },
                    on_payment_delete: move |_| {
                        let Some(payment_id) = *selected_payment_id.read() else {
                            message_box.set(Some("Select a payment method to delete.".to_string()));
                            return;
                        };
                        payment_status.set("Deleting payment method...".to_string());
                        let form = db_form.read().clone();
                        spawn(async move {
                            match delete_payment_type_in_database(form, payment_id).await {
                                Ok(rows) => {
                                    payment_types.set(rows);
                                    selected_payment_id.set(None);
                                    payment_form_name.set(String::new());
                                    payment_status.set("Payment method deleted.".to_string());
                                }
                                Err(error) => {
                                    let message = format!("Payment delete failed: {error:#}");
                                    payment_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    }
                }
            } else if *active_view.read() == WorkspaceView::Receipts {
                ReceiptsPage {
                    receipts: receipts.read().clone(),
                    selected: selected_receipt.read().clone(),
                    status: receipts_status.read().clone(),
                    search: receipt_search.read().clone(),
                    active_tab: receipt_tab.read().clone(),
                    from_date: receipt_from.read().clone(),
                    to_date: receipt_to.read().clone(),
                    on_back: move |_| active_view.set(WorkspaceView::Sales),
                    on_refresh: move |_| {
                        receipts_status.set("Loading receipts...".to_string());
                        let form = db_form.read().clone();
                        spawn(async move {
                            match load_receipts_from_database(form).await {
                                Ok(rows) => {
                                    let first_id = rows.first().map(|row| row.id);
                                    receipts.set(rows);
                                    receipts_status.set("Receipts loaded".to_string());
                                    selected_receipt.set(None);
                                    if let Some(sale_id) = first_id {
                                        let form = db_form.read().clone();
                                        spawn(async move {
                                            match load_receipt_detail_from_database(form, sale_id).await {
                                                Ok(detail) => selected_receipt.set(Some(detail)),
                                                Err(error) => {
                                                    let message = format!("Receipt detail error: {error:#}");
                                                    receipts_status.set(message.clone());
                                                    message_box.set(Some(message));
                                                }
                                            }
                                        });
                                    }
                                }
                                Err(error) => {
                                    let message = format!("Receipt load error: {error:#}");
                                    receipts_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    },
                    on_search: move |value| receipt_search.set(value),
                    on_from_date: move |value| receipt_from.set(value),
                    on_to_date: move |value| receipt_to.set(value),
                    on_tab: move |value| receipt_tab.set(value),
                    on_quick_range: move |days| {
                        let today = chrono::Local::now().date_naive();
                        receipt_to.set(today.format("%Y-%m-%d").to_string());
                        receipt_from.set((today - chrono::Duration::days(days - 1)).format("%Y-%m-%d").to_string());
                    },
                    on_select: move |sale_id| {
                        receipts_status.set("Loading receipt detail...".to_string());
                        let form = db_form.read().clone();
                        spawn(async move {
                            match load_receipt_detail_from_database(form, sale_id).await {
                                Ok(detail) => {
                                    selected_receipt.set(Some(detail));
                                    receipts_status.set("Receipt detail loaded".to_string());
                                }
                                Err(error) => {
                                    let message = format!("Receipt detail error: {error:#}");
                                    receipts_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    },
                    on_close_detail: move |_| selected_receipt.set(None),
                    on_print: move |detail| {
                        let settings=app_settings.read().clone();
                        receipts_status.set("Sending receipt to printer...".into());
                        spawn(async move {
                            match receipt_printer::print_detail(detail, settings).await {
                                Ok(()) => receipts_status.set("Receipt sent to printer".into()),
                                Err(error) => {
                                    let message=format!("Print error: {error:#}");
                                    receipts_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    },
                    on_refund: move |sale_id| {
                        let Some(actor) = session.read().clone() else { return; };
                        if !actor.allows(pos_core::auth::Permission::Manage) { message_box.set(Some("Manager or administrator authorization is required for refunds".into())); return; }
                        receipts_status.set("Refunding receipt...".to_string());
                        let form = db_form.read().clone();
                        spawn(async move {
                            match refund_receipt_in_database(form.clone(), sale_id, actor).await {
                                Ok(()) => {
                                    match load_receipts_from_database(form.clone()).await {
                                        Ok(rows) => receipts.set(rows),
                                        Err(error) => {
                                            let message = format!("Receipt reload error: {error:#}");
                                            receipts_status.set(message.clone());
                                            message_box.set(Some(message));
                                            return;
                                        }
                                    }
                                    match load_receipt_detail_from_database(form, sale_id).await {
                                        Ok(detail) => selected_receipt.set(Some(detail)),
                                        Err(error) => {
                                            let message = format!("Receipt detail error: {error:#}");
                                            receipts_status.set(message.clone());
                                            message_box.set(Some(message));
                                            return;
                                        }
                                    }
                                    receipt_tab.set("refunded".to_string());
                                    receipts_status.set("Receipt refunded".to_string());
                                }
                                Err(error) => {
                                    let message = format!("Refund error: {error:#}");
                                    receipts_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    },
                }
            } else {
            section { class: "workspace",
                aside { class: "categories panel",
                    div { class: "panel_head",
                        strong { "Categories" }
                        span { class: "count_badge", "{categories.read().len()}" }
                    }
                    label { class: "category_search",
                        span { "⌕" }
                        input {
                            placeholder: "Search categories",
                            value: "{category_query}",
                            oninput: move |event| category_query.set(event.value())
                        }
                    }
                    nav {
                        for (category, tone) in category_tiles {
                            button {
                                class: if *selected_category.read() == category.id { format!("category tone{tone} active") } else { format!("category tone{tone}") },
                                onclick: move |_| {
                                    selected_category.set(category.id);
                                    visible_product_limit.set(PRODUCT_BATCH_SIZE);
                                },
                                "{category.name}"
                            }
                        }
                    }
                }

                section { class: "catalog panel",
                    div { class: "panel_head",
                        div {
                            strong { "Products" }
                            small { "Tap a category or search the live catalog" }
                        }
                        span { class: "count_badge", "{filtered_products.len()} items" }
                    }
                    label { class: "catalog_search",
                        span { "⌕" }
                        input {
                            id: "sales_product_search",
                            autofocus: true,
                            placeholder: "Search product, SKU or scan barcode (F2)",
                            onmounted: move |element| async move { let _ = element.data().set_focus(true).await; },
                            onkeydown: move |event| {
                                if event.key() != Key::Enter { return; }
                                event.prevent_default();
                                let code = normalize_scan(&query());
                                if code.is_empty() { return; }
                                let catalog = products.read();
                                let variant_match = catalog.iter().find_map(|p| p.variants.iter().find(|v| v.barcode.as_deref() == Some(code.as_str()) || v.sku.as_deref() == Some(code.as_str())).map(|v| (p.clone(), v.clone())));
                                if let Some((product, variant)) = variant_match {
                                    add_variant_to_cart(&mut cart, product, variant);
                                    query.set(String::new());
                                } else if let Some(product) = catalog.iter().find(|p| p.barcode.as_deref() == Some(code.as_str()) || p.sku.as_deref() == Some(code.as_str())).cloned() {
                                    query.set(String::new());
                                    if is_service_product(&product) {
                                        service_product.set(Some(product)); service_price.set(String::new());
                                    } else if is_variant_product(&product) && !product.variants.is_empty() {
                                        selected_variant_id.set(product.variants.iter().find(|v| v.stock > 0.0).map(|v| v.variant_id));
                                        variant_product.set(Some(product));
                                    } else { add_to_cart(&mut cart, product); }
                                } else {
                                    checkout_status.set(format!("Barcode / SKU not found: {code}"));
                                }
                            },
                            value: "{query}",
                            oninput: move |event| {
                                query.set(event.value());
                                visible_product_limit.set(PRODUCT_BATCH_SIZE);
                            }
                        }
                    }
                    div { class: "product_grid",
                        if *product_cards_loading.read() {
                            div { class: "product_loading",
                                span { class: "loading_spinner" }
                                strong { "Loading..." }
                                small { "Product cards are loading" }
                            }
                        } else if filtered_products.is_empty() {
                            div { class: "empty catalog_empty",
                                strong { "No products loaded" }
                                span { "Open DB Settings from the menu to refresh the database connection." }
                            }
                        } else {
                            for product in visible_products {
                                ProductCard {
                                    key: "{product.id}",
                                    product: product.clone(),
                                    on_add: move |_| {
                                        if is_service_product(&product) {
                                            service_product.set(Some(product.clone()));
                                            service_price.set(String::new());
                                        } else if is_variant_product(&product) && !product.variants.is_empty() {
                                            selected_variant_id.set(product.variants.iter().find(|variant| variant.stock > 0.0).map(|variant| variant.variant_id));
                                            variant_product.set(Some(product.clone()));
                                        } else {
                                            add_to_cart(&mut cart, product.clone());
                                        }
                                    }
                                }
                            }
                            if has_more_products {
                                button {
                                    class: "load_more_products",
                                    onclick: move |_| {
                                        let next_limit = *visible_product_limit.read() + PRODUCT_BATCH_SIZE;
                                        product_cards_loading.set(true);
                                        spawn(async move {
                                            sleep(Duration::from_millis(160)).await;
                                            visible_product_limit.set(next_limit);
                                            product_cards_loading.set(false);
                                        });
                                    },
                                    span { "Load more products" }
                                    small { "{visible_product_count} / {filtered_product_count}" }
                                }
                            }
                        }
                    }
                }

                aside { class: "cart panel",
                    div { class: "panel_head",
                        div {
                            strong { "Cart" }
                            small { "Tap products to add" }
                        }
                        span { class: "count_badge", "{cart.read().len()}" }
                    }
                    div { class: "cart_lines",
                        if cart.read().is_empty() {
                            div { class: "empty",
                                div { class: "empty_cart_icon", "🛒" }
                                strong { "Cart is empty" }
                                span { "Tap a product to add it to this sale." }
                            }
                        }
                        for (idx, line) in cart.read().iter().cloned().enumerate() {
                            div { class: "cart_line",
                                div {
                                    strong { "{line.product.name}" }
                                    if let Some(label) = line.variant_label() {
                                        small { class: "cart_variant", "{label}" }
                                    }
                                    span { "{line.qty} x {format_ks(line.unit_price())}" }
                                }
                                div { class: "qty_actions",
                                    button {
                                        onclick: move |_| decrement_cart(&mut cart, idx),
                                        "-"
                                    }
                                    button {
                                        onclick: move |_| increment_cart(&mut cart, idx),
                                        "+"
                                    }
                                    button {
                                        onclick: move |_| remove_from_cart(&mut cart, idx),
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                    div { class: "totals",
                        div { span { "Subtotal" } strong { "{format_ks(total)}" } }
                        div { class: "grand_total", span { "Total" } strong { "{format_ks(total)}" } }
                        if !checkout_status.read().is_empty() {
                            small { class: "checkout_status", "{checkout_status}" }
                        }
                        div { class: "cart_actions",
                            button {
                                disabled: cart.read().is_empty(),
                                onclick: move |_| {
                                    cart.set(Vec::new());
                                    checkout_status.set(String::new());
                                },
                                "Clear"
                            }
                            button {
                                class: "checkout",
                                disabled: cart.read().is_empty(),
                                onclick: move |_| {
                                    let settings = app_settings.read();
                                    let default_discount = checkout_default_discount(&settings, total);
                                    let payable_total = (total - default_discount).max(0.0);
                                    let available_payment_types = payment_types.read().clone();
                                    checkout_discount.set(if default_discount > 0.0 {
                                        format!("{default_discount:.0}")
                                    } else {
                                        String::new()
                                    });
                                    checkout_sale_type.set(default_payment_type(&settings, &available_payment_types));
                                    received.set(format!("{payable_total:.0}"));
                                    show_checkout.set(true);
                                    checkout_customer.set(None);
                                },
                                span { "Checkout" }
                                kbd { "F4" }
                            }
                        }
                    }
                }
            }
            }

            if *show_checkout.read() {
                CheckoutDialog {
                    db_form: db_form.read().clone(),
                    customer_id: checkout_customer(),
                    on_customer: move |id| checkout_customer.set(id),
                    item_count: cart_item_count,
                    total,
                    received: received.read().clone(),
                    discount: checkout_discount.read().clone(),
                    sale_type: checkout_sale_type.read().clone(),
                    payment_types: payment_types.read().clone(),
                    saving: *checkout_saving.read(),
                    on_received: move |value: String| received.set(value),
                    on_discount: move |value: String| checkout_discount.set(value),
                    on_sale_type: move |value: String| checkout_sale_type.set(value),
                    on_cancel: move |_| {
                        if !*checkout_saving.read() {
                            show_checkout.set(false);
                        }
                    },
                    on_save: move |_| {
                        if *checkout_saving.read() {
                            return;
                        }
                        let lines = cart.read().clone();
                        let payment = received.read().parse::<f64>().unwrap_or(0.0);
                        let discount = checkout_discount.read().parse::<f64>().unwrap_or(0.0);
                        let sale_type = checkout_sale_type.read().clone();
                        let customer_id = checkout_customer();
                        let checkout_db = db_form.read().clone();
                        let Some(actor) = session.read().clone() else { return; };
                        let request = match pending.read().as_ref() {
                            Ok(Some(value)) => value.clone(),
                            Ok(None) => pending_checkout::prepare(lines, payment, discount, sale_type, customer_id, &actor, &app_settings.read()),
                            Err(e) => { message_box.set(Some(e.clone())); return; }
                        };
                        if matches!(&*pending.read(), Ok(None)) {
                            if let Err(e) = pending_checkout::save(&checkout_db, &request) { message_box.set(Some(format!("{e:#}"))); return; }
                        }
                        pending.set(Ok(Some(request.clone())));
                        checkout_saving.set(true);
                        checkout_status.set("Saving sale...".to_string());
                        spawn(async move {
                            let request_id = request.draft.invoice_no.clone();
                            match pending_checkout::submit(checkout_db.clone(), request, actor).await {
                                Ok(receipt_data) => {
                                    match pending_checkout::clear(&checkout_db, &request_id) {
                                        Ok(()) => pending.set(Ok(None)),
                                        Err(e) => { checkout_saving.set(false); message_box.set(Some(format!("Sale saved, but recovery file could not be cleared: {e}. Do not save a new sale."))); return; }
                                    }
                                    cart.set(Vec::new());
                                    show_checkout.set(false);
                                    checkout_saving.set(false);
                                    checkout_discount.set(String::new());
                                    checkout_sale_type.set("Cash".to_string());
                                    checkout_status.set(format!("Saved {}", receipt_data.invoice_no));
                                    receipt.set(Some(receipt_data.clone()));
                                    let preferences = app_settings.read().clone();
                                    let mut device_errors = Vec::new();
                                    if preferences.get("print_receipt_after_sale").is_some_and(|v| v == "1") {
                                        if let Err(error) = receipt_printer::print(receipt_data.clone(), preferences.clone()).await {
                                            device_errors.push(format!("Print: {error:#}"));
                                        }
                                    }
                                    if preferences.get("open_cash_drawer_after_sale").is_some_and(|v| v == "1") && !drawer_busy() {
                                        drawer_busy.set(true);
                                        let printer = preferences.get("receipt_printer_name").cloned().unwrap_or_default();
                                        if let Err(error) = cash_drawer::open(printer).await { device_errors.push(format!("Cash drawer: {error:#}")); }
                                        drawer_busy.set(false);
                                    }
                                    if !device_errors.is_empty() {
                                        message_box.set(Some(format!("Sale {} saved successfully. Do not save it again.\n{}",receipt_data.invoice_no,device_errors.join("\n"))));
                                    }
                                }
                                Err(error) => {
                                    checkout_saving.set(false);
                                    let message = format!("Checkout error: {error:#}");
                                    checkout_status.set(message.clone());
                                    message_box.set(Some(message));
                                }
                            }
                        });
                    }
                }
            }

            if let Some(product) = service_product.read().clone() {
                ServicePriceDialog {
                    product: product.clone(),
                    price: service_price.read().clone(),
                    on_price: move |value: String| service_price.set(value),
                    on_cancel: move |_: ()| service_product.set(None),
                    on_add: move |_: ()| {
                        let price = service_price.read().parse::<f64>().unwrap_or(0.0);
                        if price <= 0.0 {
                            message_box.set(Some("Please enter a service price.".to_string()));
                        } else {
                            add_service_to_cart(&mut cart, product.clone(), price);
                            service_product.set(None);
                            service_price.set(String::new());
                        }
                    }
                }
            }

            if let Some(product) = variant_product.read().clone() {
                VariantDialog {
                    product: product.clone(),
                    selected_variant_id: *selected_variant_id.read(),
                    on_select: move |variant_id: i32| selected_variant_id.set(Some(variant_id)),
                    on_cancel: move |_| variant_product.set(None),
                    on_add: move |_| {
                        let chosen_variant_id = *selected_variant_id.read();
                        if let Some(variant_id) = chosen_variant_id {
                            if let Some(variant) = product.variants.iter().find(|variant| variant.variant_id == variant_id).cloned() {
                                if variant.stock <= 0.0 {
                                    message_box.set(Some("Selected variant is out of stock.".to_string()));
                                } else {
                                    add_variant_to_cart(&mut cart, product.clone(), variant);
                                    variant_product.set(None);
                                    selected_variant_id.set(None);
                                }
                            }
                        } else {
                            message_box.set(Some("Please choose one variant.".to_string()));
                        }
                    }
                }
            }

            if let Some(receipt_data) = receipt.read().clone() {
                ReceiptDialog {
                    settings: app_settings.read().clone(),
                    receipt: receipt_data,
                    on_done: move |_| receipt.set(None)
                }
            }

            if let Some(message) = message_box.read().clone() {
                MessageBox {
                    title: "KAY POS Error".to_string(),
                    message,
                    on_close: move |_| message_box.set(None)
                }
            }

            status_bar::StatusBar {
                form: db_form.read().clone(),
                operator: actor.username().to_string(),
                activity: if checkout_saving() { "Saving / checking sale...".into() }
                    else if !matches!(&*pending.read(), Ok(None)) { "Checkout needs recovery".into() }
                    else if product_cards_loading() { "Loading catalog...".into() }
                    else if *active_view.read() == WorkspaceView::Receipts && !receipts_status.read().is_empty() { receipts_status.read().clone() }
                    else if *active_view.read() == WorkspaceView::Sales && !checkout_status.read().is_empty() { checkout_status.read().clone() }
                    else { "Ready".into() }
            }
        }
    }
}

fn load_env_files() {
    let _ = dotenvy::from_filename(".env");
    let _ = dotenvy::from_filename("../.env");
}

async fn load_catalog_from_database(
    form: DbForm,
) -> anyhow::Result<(Vec<UiCategory>, Vec<Product>)> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    let categories = list_categories(&pool)
        .await?
        .into_iter()
        .map(|category| UiCategory {
            id: Some(category.id),
            name: category.name,
        })
        .collect::<Vec<_>>();
    let products = pos_core::db::search_product_metadata(&pool, "", 0).await?;

    let mut category_names = vec![UiCategory {
        id: None,
        name: "All".to_string(),
    }];
    category_names.extend(categories);

    Ok((category_names, products))
}

async fn load_startup_data_from_database(
    form: DbForm,
) -> anyhow::Result<(
    Vec<UiCategory>,
    Vec<Product>,
    HashMap<String, String>,
    Vec<PaymentType>,
)> {
    // Establish the shared pool first so parallel loads reuse the same connection pool.
    let _pool = connect(&form.database_config()?).await?;
    let ((categories, products), (settings, payment_types)) = tokio::try_join!(
        load_catalog_from_database(form.clone()),
        load_settings_center_data_from_database(form)
    )?;
    Ok((categories, products, settings, payment_types))
}

async fn load_app_settings_from_database(form: DbForm) -> anyhow::Result<HashMap<String, String>> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    Ok(list_settings(&pool)
        .await?
        .into_iter()
        .map(|setting| (setting.key, setting.value.unwrap_or_default()))
        .collect())
}

async fn load_payment_types_from_database(form: DbForm) -> anyhow::Result<Vec<PaymentType>> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    Ok(list_payment_types(&pool).await?)
}

async fn save_payment_type_in_database(
    form: DbForm,
    payment_id: Option<i32>,
    name: String,
) -> anyhow::Result<Vec<PaymentType>> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    save_payment_type(&pool, payment_id, &name).await?;
    Ok(list_payment_types(&pool).await?)
}

async fn delete_payment_type_in_database(
    form: DbForm,
    payment_id: i32,
) -> anyhow::Result<Vec<PaymentType>> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    delete_payment_type(&pool, payment_id).await?;
    Ok(list_payment_types(&pool).await?)
}

async fn load_settings_center_data_from_database(
    form: DbForm,
) -> anyhow::Result<(HashMap<String, String>, Vec<PaymentType>)> {
    let (settings, payment_types) = tokio::try_join!(
        load_app_settings_from_database(form.clone()),
        load_payment_types_from_database(form)
    )?;
    Ok((settings, payment_types))
}

async fn load_receipts_from_database(form: DbForm) -> anyhow::Result<Vec<SaleSummary>> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    Ok(list_receipts(&pool, "", 1000, 0).await?)
}

async fn load_receipt_detail_from_database(
    form: DbForm,
    sale_id: i32,
) -> anyhow::Result<ReceiptDetail> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    Ok(get_receipt_detail(&pool, sale_id).await?)
}

async fn refund_receipt_in_database(
    form: DbForm,
    sale_id: i32,
    actor: pos_core::auth::Session,
) -> anyhow::Result<()> {
    let config = form.database_config()?;
    let pool = connect(&config).await?;
    Ok(refund_sale(&pool, sale_id, &actor).await?)
}

#[component]
fn CheckoutDialog(
    db_form: DbForm,
    customer_id: Option<i32>,
    on_customer: EventHandler<Option<i32>>,
    item_count: f64,
    total: f64,
    received: String,
    discount: String,
    sale_type: String,
    payment_types: Vec<PaymentType>,
    saving: bool,
    on_received: EventHandler<String>,
    on_discount: EventHandler<String>,
    on_sale_type: EventHandler<String>,
    on_cancel: EventHandler<()>,
    on_save: EventHandler<()>,
) -> Element {
    let checkout_data = use_resource(move || {
        let source = db_form.clone();
        async move {
            let result = async {
                let pool = connect(&source.database_config()?).await?;
                let customers = pos_core::customers::list(&pool).await?;
                let settings: HashMap<String, String> = pos_core::db::list_settings(&pool)
                    .await?
                    .into_iter()
                    .map(|s| (s.key, s.value.unwrap_or_default()))
                    .collect();
                Ok::<_, anyhow::Error>((customers, settings))
            }
            .await;
            if let Ok((_, settings)) = &result {
                let value = checkout_default_discount(settings, total);
                on_discount.call(format!("{value:.2}"));
                on_received.call(format!(
                    "{:.2}",
                    (total - value).max(0.0) + checkout_tax(settings, total - value)
                ));
            }
            result.map_err(|e| format!("{e:#}"))
        }
    });
    let loaded = checkout_data.read();
    let ready = matches!(loaded.as_ref(), Some(Ok(_)));
    let discount_editable = loaded
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|(_, settings)| checkout_manual_discount(settings))
        .unwrap_or(false);
    let discount_value = discount.parse::<f64>().unwrap_or(0.0).clamp(0.0, total);
    let tax = loaded
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|(_, settings)| checkout_tax(settings, total - discount_value))
        .unwrap_or(0.0);
    let payable_total = (total - discount_value).max(0.0) + tax;
    let received_value = received.parse::<f64>().unwrap_or(0.0);
    let change = (received_value - payable_total).max(0.0);
    let credit = sale_type.eq_ignore_ascii_case("Credit");
    let ready_to_save = received_value.is_finite()
        && received_value >= 0.0
        && (if credit {
            customer_id.is_some() && received_value <= payable_total
        } else {
            received_value >= payable_total
        })
        && !saving
        && ready;
    let keypad_digits = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "00", "0", "."]
        .into_iter()
        .map(|digit| (digit.to_string(), received.clone()))
        .collect::<Vec<_>>();
    let backspace_received = received.clone();
    let mut payment_options = if payment_types.is_empty() {
        vec!["Cash".to_string(), "Credit".to_string()]
    } else {
        payment_types
            .into_iter()
            .map(|payment_type| payment_type.name)
            .collect::<Vec<_>>()
    };
    payment_options.retain(|p| !p.eq_ignore_ascii_case("Credit"));
    if !payment_options
        .iter()
        .any(|p| p.eq_ignore_ascii_case("Cash"))
    {
        payment_options.insert(0, "Cash".into());
    }
    if customer_id.is_some() {
        payment_options.push("Credit".into());
    }

    rsx! {
        div { class: "modal_backdrop",
            div { class: "checkout_workspace",
                section { class: "checkout_dialog",
                    div { class: "dialog_head checkout_head",
                        div {
                            strong { "Checkout" }
                            small { "Review payment before saving" }
                        }
                        button { disabled: saving, onclick: move |_| on_cancel.call(()), "×" }
                    }
                    div { class: "checkout_body",
                        div { class: "checkout_summary",
                            div { span { "Items" } strong { "{format_qty(item_count)}" } }
                            div { span { "Subtotal" } strong { "{format_ks(total)}" } }
                            div { span { "Discount" } strong { "{format_ks(discount_value)}" } }
                            div { span { "Tax" } strong { "{format_ks(tax)}" } }
                            div { class: "checkout_total_row", span { "Total" } strong { "{format_ks(payable_total)}" } }
                            div { span { "Received" } strong { "{format_ks(received_value)}" } }
                            div { span { "Change" } strong { "{format_ks(change)}" } }
                        }
                        div { class: "checkout_form",
                            label {
                                "Customer"
                                select {class:"checkout_select checkout_customer",disabled:saving||!ready,value:customer_id.map(|id|id.to_string()).unwrap_or_default(),onchange:move |e|{let id=e.value().parse().ok();on_customer.call(id);if id.is_none()&&credit {on_sale_type.call("Cash".into());on_received.call(format!("{payable_total:.2}"));}},
                                    option {value:"",selected:customer_id.is_none(),"Walk-in Customer"}
                                    if let Some(Ok((customers,_)))=loaded.as_ref() {for customer in customers {option {value:"{customer.id}",selected:customer_id==Some(customer.id),"{customer.name} · {customer.phone}"}}}
                                }
                            }
                            if !checkout_data.finished(){small {role:"status","Loading customers and settings..."}}
                            if let Some(Err(e))=loaded.as_ref(){small {role:"alert","{e}"}}
                            label {
                                "Sale type"
                                select { class: "checkout_select", value:"{sale_type}", disabled:saving||!ready,
                                    onchange:move |e|{let value=e.value();on_received.call(if value.eq_ignore_ascii_case("Credit"){"0".into()}else{format!("{payable_total:.2}")});on_sale_type.call(value);},
                                    for option in payment_options {
                                        option {
                                            value:"{option}",selected:sale_type==option,
                                            "{option}"
                                        }
                                    }
                                }
                            }
                            label {
                                "Discount"
                                input {
                                    value: "{discount}",
                                    readonly: saving || !discount_editable,
                                    oninput: move |event| on_discount.call(decimal_input(&event.value()))
                                }
                            }
                            label {
                                "Received"
                                input {
                                    class: "checkout_received",
                                    inputmode: "decimal",
                                    autofocus: true,
                                    value: "{received}",
                                    onmounted: move |element| async move {
                                        let _ = element.data().set_focus(true).await;
                                    },
                                    oninput: move |event| on_received.call(decimal_input(&event.value())),
                                    onkeydown: move |event| {
                                        match event.data.key().to_string().as_str() {
                                            "Enter" => {
                                                if ready_to_save {
                                                    on_save.call(());
                                                }
                                            }
                                            "Escape" => {
                                                if !saving {
                                                    on_cancel.call(());
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            small {
                                class: if received_value >= payable_total { "payment_ready" } else { "payment_due" },
                                if saving {
                                    "Saving sale..."
                                } else if received_value >= payable_total {
                                    "Ready to save."
                                } else {
                                    "Enter received amount."
                                }
                            }
                        }
                    }
                    div { class: "checkout_actions",
                        button { disabled: saving, onclick: move |_| on_cancel.call(()), "⊗  Cancel" }
                        button {
                            class: "primary",
                            disabled: !ready_to_save,
                            onclick: move |_| on_save.call(()),
                            if saving { "Saving..." } else { "▣  Save Sale" }
                        }
                    }
                }

                aside { class: "checkout_keypad",
                    strong { "Keypad - Received" }
                    div { class: "number_pad checkout_number_pad",
                        for (digit, current_received) in keypad_digits {
                            button {
                                disabled: saving,
                                onclick: move |_| {
                                    let mut next = current_received.clone();
                                    next.push_str(&digit);
                                    on_received.call(decimal_input(&next));
                                },
                                "{digit}"
                            }
                        }
                        button {
                            class: "keypad_clear",
                            disabled: saving,
                            onclick: move |_| on_received.call(String::new()),
                            "Clear"
                        }
                        button {
                            disabled: saving,
                            onclick: move |_| {
                                let mut next = backspace_received.clone();
                                next.pop();
                                on_received.call(next);
                            },
                            "⌫"
                        }
                    }
                    div { class: "quick_received",
                        strong { "Quick received" }
                        div {
                            for amount in quick_received_amounts(payable_total) {
                                button {
                                    disabled: saving,
                                    "aria-pressed": (received_value == amount).to_string(),
                                    onclick: move |_| on_received.call(amount.to_string()),
                                    "{format_ks(amount)}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct SettingsNavItem {
    id: &'static str,
    title: &'static str,
    keywords: &'static str,
}

fn settings_nav_items() -> Vec<SettingsNavItem> {
    vec![
        SettingsNavItem { id: "updates", title: "App Updates", keywords: "version github update release" },
        SettingsNavItem {
            id: "overview",
            title: "Overview",
            keywords: "status summary quick settings",
        },
        SettingsNavItem {
            id: "payments",
            title: "Payment Types",
            keywords: "payment method cash credit mobile money",
        },
        SettingsNavItem {
            id: "tax_discount",
            title: "Tax & Discount",
            keywords: "tax discount percentage fixed manual",
        },
        SettingsNavItem {
            id: "appearance",
            title: "Appearance",
            keywords: "theme display touch mode",
        },
        SettingsNavItem {
            id: "receipt",
            title: "Receipt",
            keywords: "receipt header footer branding shop name printer",
        },
        SettingsNavItem {
            id: "regional",
            title: "Regional",
            keywords: "currency language kyats",
        },
        SettingsNavItem {
            id: "printer",
            title: "Printer",
            keywords: "windows receipt local printer printing",
        },
        SettingsNavItem {
            id: "database",
            title: "Database",
            keywords: "postgresql server host port connection",
        },
        SettingsNavItem {
            id: "users",
            title: "Users",
            keywords: "user role admin cashier permission",
        },
    ]
}

#[component]
fn SettingsCenterPage(
    query: String,
    active_page: String,
    db_form: DbForm,
    db_status: String,
    settings: HashMap<String, String>,
    on_saved: EventHandler<HashMap<String, String>>,
    payment_types: Vec<PaymentType>,
    selected_payment_id: Option<i32>,
    payment_form_name: String,
    payment_status: String,
    on_query: EventHandler<String>,
    on_page: EventHandler<String>,
    on_open_db: EventHandler<MouseEvent>,
    on_back: EventHandler<MouseEvent>,
    on_payment_select: EventHandler<(i32, String)>,
    on_payment_name: EventHandler<String>,
    on_payment_add: EventHandler<MouseEvent>,
    on_payment_update: EventHandler<MouseEvent>,
    on_payment_delete: EventHandler<MouseEvent>,
) -> Element {
    let normalized_query = query.trim().to_lowercase();
    let filtered_items = settings_nav_items()
        .into_iter()
        .filter(|item| {
            normalized_query.is_empty()
                || item.title.to_lowercase().contains(&normalized_query)
                || item.keywords.to_lowercase().contains(&normalized_query)
        })
        .collect::<Vec<_>>();

    rsx! {
        section { class: "settings_center",
            aside { class: "settings_sidebar panel",
                div { class: "settings_sidebar_head",
                    strong { "Settings" }
                    small { "Setting Center" }
                }
                label { class: "settings_search",
                    span { "⌕" }
                    input {
                        placeholder: "Search settings...",
                        value: "{query}",
                        oninput: move |event| on_query.call(event.value())
                    }
                }
                nav {
                    for item in filtered_items {
                        button {
                            class: if active_page == item.id { "active" } else { "" },
                            onclick: move |_| on_page.call(item.id.to_string()),
                            "{item.title}"
                        }
                    }
                }
            }

            section { class: "settings_content panel",
                div { class: "settings_titlebar",
                    div {
                        strong { "{settings_page_title(&active_page)}" }
                        small { "{settings_page_subtitle(&active_page)}" }
                    }
                }
                div { class: "settings_page_body",
                    if active_page == "updates" {
                        updates::SettingsUpdates {}
                    } else if active_page == "overview" {
                        SettingsOverview { db_form: db_form.clone(), db_status: db_status.clone(), settings: settings.clone(), on_open_db }
                    } else if active_page == "payments" {
                        PaymentTypesSettings {
                            payment_types: payment_types.clone(),
                            selected_payment_id,
                            name: payment_form_name.clone(),
                            status: payment_status.clone(),
                            on_select: on_payment_select,
                            on_name: on_payment_name,
                            on_add: on_payment_add,
                            on_update: on_payment_update,
                            on_delete: on_payment_delete
                        }
                    } else if active_page == "tax_discount" {
                        SettingsSection {
                            title: "Tax & Discount",
                            key: "{active_page}",
                            db_form: db_form.clone(), on_saved,
                            description: "Tax and discount values loaded from PostgreSQL settings.",
                            rows: settings_rows(&settings, &["tax_enabled", "tax_rate", "discount_enabled", "discount_type", "discount_value"])
                        }
                    } else if active_page == "appearance" {
                        SettingsSection {
                            title: "Appearance",
                            key: "{active_page}",
                            db_form: db_form.clone(), on_saved,
                            description: "Appearance values loaded from PostgreSQL settings.",
                            rows: settings_rows(&settings, &["theme", "follow_system_theme", "window_resolution", "ui_header_color", "ui_status_color", "ui_button_color", "ui_hover_color", "ui_focus_color", "ui_category_color", "ui_product_hover_color", "ui_sidebar_color"])
                        }
                    } else if active_page == "receipt" {
                        SettingsSection {
                            title: "Receipt",
                            key: "{active_page}",
                            db_form: db_form.clone(), on_saved,
                            description: "Receipt branding and print values loaded from PostgreSQL settings.",
                            rows: settings_rows(&settings, &["shop_name", "shop_phone", "shop_address", "receipt_header", "receipt_footer", "shop_footer_message", "receipt_paper_size"])
                        }
                    } else if active_page == "printer" {
                        printer_settings::PrinterSettings { settings: settings.clone(), db_form: db_form.clone(), on_saved }
                    } else if active_page == "regional" {
                        SettingsSection {
                            title: "Regional",
                            key: "{active_page}",
                            db_form: db_form.clone(), on_saved,
                            description: "Regional values loaded from PostgreSQL settings.",
                            rows: settings_rows(&settings, &["language", "currency", "currency_symbol"])
                        }
                    } else if active_page == "database" {
                        SettingsDatabase { db_form, db_status, on_open_db }
                    } else if active_page == "users" {
                        users::UsersSettings { db_form: db_form.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsOverview(
    db_form: DbForm,
    db_status: String,
    settings: HashMap<String, String>,
    on_open_db: EventHandler<MouseEvent>,
) -> Element {
    let shop_name = setting_value(&settings, "shop_name", "KAY POS");
    let currency = setting_value(&settings, "currency_symbol", "Ks");
    let theme = setting_value(&settings, "theme", "System");
    let tax = setting_value(&settings, "tax_rate", "0");
    let receipt_paper = setting_value(&settings, "receipt_paper_size", "80");
    rsx! {
        div { class: "settings_overview_grid",
            SettingsCard {
                title: "Shop",
                value: shop_name,
                action: "settings.shop_name".to_string()
            }
            SettingsCard {
                title: "Database",
                value: format!("{}:{} / {}", db_form.host, db_form.port, db_form.database),
                action: db_status
            }
            SettingsCard {
                title: "Currency",
                value: currency,
                action: "settings.currency_symbol".to_string()
            }
            SettingsCard {
                title: "Receipt Paper",
                value: receipt_paper,
                action: "settings.receipt_paper_size".to_string()
            }
            SettingsCard {
                title: "Theme",
                value: theme,
                action: "settings.theme".to_string()
            }
            SettingsCard {
                title: "Tax Rate",
                value: tax,
                action: "settings.tax_rate".to_string()
            }
            div { class: "settings_action_card",
                strong { "PostgreSQL Connection" }
                span { "Open the existing DB Settings dialog to update this Rust POS client connection." }
                button { onclick: move |event| on_open_db.call(event), "Open DB Settings" }
            }
        }
    }
}

#[component]
fn SettingsDatabase(
    db_form: DbForm,
    db_status: String,
    on_open_db: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "settings_form_page",
            div { class: "settings_info_block",
                strong { "PostgreSQL Database" }
                span { "The Rust POS client reads and writes to the configured PostgreSQL server." }
            }
            div { class: "settings_kv",
                div { span { "Host" } strong { "{db_form.host}" } }
                div { span { "Port" } strong { "{db_form.port}" } }
                div { span { "Database" } strong { "{db_form.database}" } }
                div { span { "Username" } strong { "{db_form.username}" } }
                div { span { "Status" } strong { "{db_status}" } }
            }
            div { class: "settings_page_actions",
                button { onclick: move |event| on_open_db.call(event), "Open DB Settings" }
            }
        }
    }
}

#[component]
fn PaymentTypesSettings(
    payment_types: Vec<PaymentType>,
    selected_payment_id: Option<i32>,
    name: String,
    status: String,
    on_select: EventHandler<(i32, String)>,
    on_name: EventHandler<String>,
    on_add: EventHandler<MouseEvent>,
    on_update: EventHandler<MouseEvent>,
    on_delete: EventHandler<MouseEvent>,
) -> Element {
    let can_save = !name.trim().is_empty();
    let can_edit = selected_payment_id.is_some() && can_save;
    let can_delete = selected_payment_id.is_some();

    rsx! {
        div { class: "settings_form_page",
            div { class: "tax_discount_form payment_preferences",
            fieldset {
            legend { "Payment Types" }
            div { class: "payment_editor",
                label {
                    span { "Payment method name" }
                    input {
                        placeholder: "Cash, Kpay, WavePay...",
                        value: "{name}",
                        oninput: move |event| on_name.call(event.value())
                    }
                }
                div { class: "payment_actions",
                    button {
                        disabled: !can_save,
                        onclick: move |event| on_add.call(event),
                        "Add New"
                    }
                    button {
                        class: "secondary",
                        disabled: !can_edit,
                        onclick: move |event| on_update.call(event),
                        "Edit"
                    }
                    button {
                        class: "danger",
                        disabled: !can_delete,
                        onclick: move |event| on_delete.call(event),
                        "Delete"
                    }
                }
                if !status.is_empty() {
                    small { "{status}" }
                } else if selected_payment_id.is_some() {
                    small { "Selected payment method is ready to edit or delete." }
                } else {
                    small { "Type a new name to add, or select a row to edit." }
                }
            }
            div { class: "settings_table",
                div { class: "settings_table_head",
                    span { "#" }
                    span { "Payment Method" }
                }
                if payment_types.is_empty() {
                    div { class: "settings_table_empty", "No payment types loaded from database." }
                } else {
                    for (index, payment_type) in payment_types.iter().enumerate() {
                        button {
                            class: if selected_payment_id == Some(payment_type.id) { "settings_table_row active" } else { "settings_table_row" },
                            onclick: {
                                let payment_id = payment_type.id;
                                let payment_name = payment_type.name.clone();
                                move |_| on_select.call((payment_id, payment_name.clone()))
                            },
                            span { "{index + 1}" }
                            strong { "{payment_type.name}" }
                        }
                    }
                }
            }
            }
            }
        }
    }
}

#[component]
fn SettingsSection(
    title: &'static str,
    description: &'static str,
    rows: Vec<(String, String)>,
    db_form: DbForm,
    on_saved: EventHandler<HashMap<String, String>>,
) -> Element {
    let mut draft = use_signal(HashMap::<String, String>::new);
    let mut saving = use_signal(|| false);
    let mut notice = use_signal(String::new);
    let mut error = use_signal(|| false);
    let original = rows.clone();
    let organized = matches!(
        title,
        "Tax & Discount" | "Appearance" | "Regional" | "Receipt" | "Users"
    );
    rsx! {
        div { class: if organized { "settings_form_page settings_organized" } else { "settings_form_page" },
            if !organized { h3 { "{title}" } }
            if title == "Receipt" {
                div { class: "receipt_settings_columns",
                div { class: "tax_discount_form receipt_preferences",
                    fieldset {
                        legend { "Receipt details" }
                        div { class: "tax_discount_inputs",
                            for (key, label) in [
                                ("shop_name", "Shop Name"),
                                ("shop_phone", "Shop Phone"),
                                ("shop_address", "Shop Address"),
                                ("receipt_header", "Receipt Header"),
                                ("receipt_footer", "Receipt Footer"),
                                ("shop_footer_message", "Shop Footer Message")
                            ] {
                                label {
                                    span { "{label}" }
                                    input {
                                        r#type: "text",
                                        disabled: saving(),
                                        value: draft.read().get(key).cloned().or_else(|| original.iter().find(|(k,_)| k == key).map(|(_,v)| v.clone())).unwrap_or_default(),
                                        oninput: move |event| { draft.write().insert(key.into(), event.value()); notice.set(String::new()); }
                                    }
                                }
                            }
                        }
                    }
                }
                receipt_preview::ReceiptPreview { original: original.clone(), draft }
                }
            } else if title == "Regional" {
                div { class: "tax_discount_form regional_preferences",
                    fieldset {
                        legend { "Currency and language" }
                        div { class: "tax_discount_inputs",
                            for (key, label, options) in [
                                ("currency", "Currency", vec![("Kyats (Ks)", "Kyats (Ks)"), ("Dollar ($)", "Dollar ($)"), ("Baht (B)", "Baht (B)")]),
                                ("language", "App language", vec![("en", "English"), ("my", "Myanmar")])
                            ] {
                                label {
                                    span { "{label}" }
                                    select {
                                        disabled: saving(),
                                        value: draft.read().get(key).cloned().or_else(|| original.iter().find(|(k,_)| k == key).map(|(_,v)| v.clone())).unwrap_or_else(|| if key == "language" { "en".into() } else { String::new() }),
                                        onchange: move |event| { draft.write().insert(key.into(), event.value()); notice.set(String::new()); },
                                        if key == "currency" { option { value: "", "Currency" } }
                                        for (value, name) in options {
                                            option { value,
                                                selected: draft.read().get(key).or_else(|| original.iter().find(|(k,_)| k == key).map(|(_,v)| v)).map(|v| v == value).unwrap_or(key == "language" && value == "en"),
                                                "{name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else if title == "Appearance" {
                div { class: "tax_discount_form appearance_preferences",
                    fieldset {
                        legend { "Display preferences" }
                        label { class: "appearance_theme",
                            span { "Theme" }
                            select {
                                disabled: saving(),
                                value: if draft.read().get("theme").or_else(|| original.iter().find(|(k,_)| k == "theme").map(|(_,v)| v)).is_some_and(|v| v.eq_ignore_ascii_case("Dark")) { "Dark" } else { "Light" },
                                onchange: move |event| { draft.write().insert("theme".into(), event.value()); notice.set(String::new()); },
                                for theme in ["Light", "Dark"] {
                                    option { value: theme,
                                        selected: (theme == "Dark") == draft.read().get("theme").or_else(|| original.iter().find(|(k,_)| k == "theme").map(|(_,v)| v)).is_some_and(|v| v.eq_ignore_ascii_case("Dark")),
                                        "{theme}"
                                    }
                                }
                            }
                        }
                        label { class: "tax_discount_toggle",
                            input { r#type: "checkbox", disabled: saving(),
                                checked: draft.read().get("follow_system_theme").or_else(|| original.iter().find(|(k,_)| k == "follow_system_theme").map(|(_,v)| v)).map(|v| v == "1").unwrap_or(false),
                                onchange: move |event| { draft.write().insert("follow_system_theme".into(), if event.checked() { "1" } else { "0" }.into()); notice.set(String::new()); }
                            }
                            span { "Follow system theme" }
                        }
                    }
                }
                div { class: "tax_discount_form",
                    appearance_colors::ColorPreferences { draft, original: original.clone(), saving: saving() }
                }
            } else if title == "Tax & Discount" {
                div { class: "tax_discount_form",
                    for (group,toggle,fields) in [("Tax","tax_enabled",vec![("tax_rate","Tax rate (%)")]),("Discount","discount_enabled",vec![("discount_type","Discount type"),("discount_value","Discount value")])] {
                        fieldset {
                            legend { "{group}" }
                            label { class: "tax_discount_toggle",
                                input { r#type:"checkbox",disabled:saving(),checked:draft.read().get(toggle).or_else(||original.iter().find(|(key,_)|key==toggle).map(|(_,v)|v)).map(|v|v=="1").unwrap_or(false),
                                    onchange:move |e|{draft.write().insert(toggle.into(),if e.checked(){"1"}else{"0"}.into());notice.set(String::new());}
                                }
                                span { "Enable {group.to_lowercase()}" }
                            }
                            div {class:"tax_discount_inputs",
                                for (key,label) in fields {
                                    label { span {"{label}"}
                                        if key=="discount_type" {
                                            select {disabled:saving(),value:draft.read().get(key).cloned().or_else(||original.iter().find(|(k,_)|k==key).map(|(_,v)|v.clone())).unwrap_or_default(),onchange:move |e|{draft.write().insert(key.into(),e.value());notice.set(String::new());},
                                                option {value:"","Discount type"}
                                                for option in ["percentage","fixed","manual"] {option {value:option,selected:draft.read().get(key).or_else(||original.iter().find(|(k,_)|k==key).map(|(_,v)|v)).map(|v|v==option).unwrap_or(false),"{option}"}}
                                            }
                                        } else {
                                            input {r#type:"number",min:"0",step:"0.01",max:if key=="tax_rate"{"100"}else{""},disabled:saving(),value:draft.read().get(key).cloned().or_else(||original.iter().find(|(k,_)|k==key).map(|(_,v)|v.clone())).unwrap_or_else(||"0.0".into()),oninput:move |e|{draft.write().insert(key.into(),e.value());notice.set(String::new());}}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
            div { class: "settings_edit_fields",
                for (key, stored) in rows {
                    {
                        let value = draft.read().get(&key).cloned().unwrap_or(stored);
                        let label = match key.as_str() {
                            "tax_enabled" => "Enable tax".to_string(),
                            "tax_rate" => "Tax rate (%)".to_string(),
                            "discount_enabled" => "Enable discount".to_string(),
                            "discount_value" => "Default discount value".to_string(),
                            "auto_backup_enabled" => "Enable automatic backup".to_string(),
                            "auto_backup_interval" => "Backup interval (hours)".to_string(),
                            "auto_backup_max" => "Backups to keep".to_string(),
                            _ => humanize_setting_key(&key),
                        };
                        let group = if organized { match key.as_str() {
                            "tax_enabled" => "Tax",
                            "discount_enabled" => "Discount",
                            "theme" => "Theme & Display",
                            "default_role" => "Default Role",
                            "allow_cashier_discount" => "Permissions",
                            "auto_backup_enabled" => "Automatic Backup",
                            _ => "",
                        }} else { "" };
                        let numeric = matches!(key.as_str(), "tax_rate" | "discount_value" | "auto_backup_interval" | "auto_backup_max");
                        let options: Vec<&str> = match key.as_str() {
                            "theme" => vec!["Light", "Dark"],
                            "discount_type" => vec!["percentage", "fixed", "manual"],
                            "language" => vec!["en", "my"],
                            "currency" => vec!["Kyats (Ks)", "Dollar ($)", "Baht (B)"],
                            _ => vec![],
                        };
                        let toggle = key.ends_with("_enabled") || key.starts_with("allow_") || key.starts_with("require_") || key == "follow_system_theme";
                        rsx! {
                            if !group.is_empty() { h3 { class: "settings_group_title", "{group}" } }
                            label { class: if organized && toggle { "settings_edit_field settings_toggle_field" } else { "settings_edit_field" },
                                span { "{label}" }
                                if toggle {
                                    input { r#type: "checkbox", checked: value == "1", disabled: saving(),
                                        onchange: move |event| { draft.write().insert(key.clone(), if event.checked() { "1" } else { "0" }.into()); notice.set(String::new()); }
                                    }
                                } else if !options.is_empty() {
                                    select { value: "{value}", disabled: saving(),
                                        onchange: move |event| { draft.write().insert(key.clone(), event.value()); notice.set(String::new()); },
                                        option { value: "", "Select" }
                                        for option in options { option { value: "{option}", "{option}" } }
                                    }
                                } else {
                                    input { r#type: if numeric { "number" } else { "text" },
                                        min: if numeric { "0" } else { "" },
                                        step: if key == "tax_rate" || key == "discount_value" { "0.01" } else { "1" },
                                        value: "{value}", disabled: saving(),
                                        oninput: move |event| { draft.write().insert(key.clone(), event.value()); notice.set(String::new()); }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            }
            div { class: "settings_page_actions",
                button { disabled: saving() || draft.read().is_empty(), onclick: move |_| { draft.write().clear(); notice.set(String::new()); }, "Reset Changes" }
                button { class: "settings_save", disabled: saving() || draft.read().is_empty(), onclick: move |_| {
                    let allowed = original.iter().map(|(key, _)| key).collect::<Vec<_>>();
                    let mut values = draft.read().iter().filter(|(key, _)| allowed.contains(key)).map(|(key, value)| (key.clone(), value.clone())).collect::<Vec<_>>();
                    if title == "Regional" {
                        if let Some(symbol) = values.iter().find(|(k,_)| k == "currency").and_then(|(_,v)| match v.as_str() { "Kyats (Ks)" => Some("Ks"), "Dollar ($)" => Some("$"), "Baht (B)" => Some("B"), _ => None }) {
                            values.push(("currency_symbol".into(), symbol.into()));
                        }
                    }
                    for (key, value) in &values {
                        if appearance_colors::FIELDS.iter().any(|(color_key,_,_)| *color_key == key.as_str()) && !appearance_colors::valid(value) {
                            error.set(true); notice.set("Choose a valid color.".into()); return;
                        }
                        if matches!(key.as_str(), "tax_rate" | "discount_value" | "auto_backup_interval" | "auto_backup_max") && !value.parse::<f64>().map(|n| n.is_finite() && n >= 0.0 && (key != "tax_rate" || n <= 100.0)).unwrap_or(false) {
                            error.set(true); notice.set(format!("Invalid value for {}", humanize_setting_key(key))); return;
                        }
                    }
                    let form = db_form.clone();
                    saving.set(true);
                    spawn(async move {
                        let result = async {
                            let pool = connect(&form.database_config()?).await?;
                            pos_core::db::save_settings(&pool, &values).await
                        }.await;
                        saving.set(false);
                        match result {
                            Ok(()) => { on_saved.call(values.into_iter().collect()); draft.write().clear(); error.set(false); notice.set("Settings saved.".into()); }
                            Err(err) => { error.set(true); notice.set(format!("Could not save settings: {err:#}")); }
                        }
                    });
                }, if saving() { "Saving..." } else { "Save Settings" } }
            }
            if !notice().is_empty() {
                if error() {
                    MessageBox { title: "Settings Error".to_string(), message: notice(),
                        on_close: move |_| { notice.set(String::new()); error.set(false); }
                    }
                } else {
                    div { role: "status", "{notice}" }
                }
            }
        }
    }
}

#[component]
fn SettingsCard(title: &'static str, value: String, action: String) -> Element {
    rsx! {
        div { class: "settings_card",
            span { "{title}" }
            strong { "{value}" }
            small { "{action}" }
        }
    }
}

fn settings_page_title(page: &str) -> &'static str {
    settings_nav_items()
        .into_iter()
        .find(|item| item.id == page)
        .map(|item| item.title)
        .unwrap_or("Settings")
}

fn settings_page_subtitle(page: &str) -> &'static str {
    match page {
        "overview" => "Quick summary for this Rust POS client.",
        "payments" => "Configure sale payment options.",
        "tax_discount" => "Discount and future tax behavior.",
        "appearance" => "Display and workspace behavior.",
        "receipt" => "Receipt output and print behavior.",
        "regional" => "Currency and language display.",
        "database" => "PostgreSQL connection used by this client.",
        "users" => "Account and permission planning.",
        _ => "Settings Center",
    }
}

fn setting_value(settings: &HashMap<String, String>, key: &str, fallback: &str) -> String {
    settings
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

fn settings_rows(settings: &HashMap<String, String>, keys: &[&str]) -> Vec<(String, String)> {
    keys.iter()
        .map(|key| (key.to_string(), setting_value(settings, key, "")))
        .collect()
}

fn humanize_setting_key(key: &str) -> String {
    key.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn checkout_manual_discount(settings: &HashMap<String, String>) -> bool {
    matches!(
        setting_value(settings, "discount_enabled", "0")
            .to_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    ) && setting_value(settings, "discount_type", "manual").eq_ignore_ascii_case("manual")
}

#[cfg(test)]
mod checkout_settings_tests {
    use super::*;
    #[test]
    fn discount_obeys_database_settings() {
        let mut settings = HashMap::from([
            ("discount_enabled".into(), "0".into()),
            ("discount_type".into(), "percentage".into()),
            ("discount_value".into(), "10".into()),
        ]);
        assert_eq!(checkout_effective_discount(&settings, 200.0, 50.0), 0.0);
        settings.insert("discount_enabled".into(), "1".into());
        assert_eq!(checkout_effective_discount(&settings, 200.0, 50.0), 20.0);
        settings.insert("discount_type".into(), "fixed".into());
        assert_eq!(checkout_effective_discount(&settings, 200.0, 50.0), 10.0);
        settings.insert("discount_type".into(), "manual".into());
        assert_eq!(checkout_effective_discount(&settings, 200.0, 50.0), 50.0);
        assert_eq!(checkout_effective_discount(&settings, 200.0, 300.0), 200.0);
    }
}

#[cfg(test)]
fn checkout_effective_discount(
    settings: &HashMap<String, String>,
    subtotal: f64,
    manual: f64,
) -> f64 {
    if checkout_manual_discount(settings) {
        manual.clamp(0.0, subtotal)
    } else {
        checkout_default_discount(settings, subtotal)
    }
}

fn checkout_default_discount(settings: &HashMap<String, String>, subtotal: f64) -> f64 {
    let enabled = setting_value(settings, "discount_enabled", "0");
    if !matches!(enabled.as_str(), "1" | "true" | "True" | "yes" | "on") {
        return 0.0;
    }

    let value = setting_value(settings, "discount_value", "0")
        .parse::<f64>()
        .unwrap_or(0.0)
        .max(0.0);
    match setting_value(settings, "discount_type", "manual")
        .to_lowercase()
        .as_str()
    {
        "percentage" => (subtotal * value / 100.0).clamp(0.0, subtotal),
        "fixed" => value.clamp(0.0, subtotal),
        _ => 0.0,
    }
}

fn default_payment_type(
    settings: &HashMap<String, String>,
    payment_types: &[PaymentType],
) -> String {
    let configured = setting_value(settings, "default_payment_type", "");
    if !configured.is_empty()
        && payment_types
            .iter()
            .any(|payment_type| payment_type.name.eq_ignore_ascii_case(&configured))
    {
        return configured;
    }

    payment_types
        .iter()
        .find(|payment_type| payment_type.name.eq_ignore_ascii_case("cash"))
        .or_else(|| payment_types.first())
        .map(|payment_type| payment_type.name.clone())
        .unwrap_or_else(|| "Cash".to_string())
}

#[component]
fn ReceiptsPage(
    receipts: Vec<SaleSummary>,
    selected: Option<ReceiptDetail>,
    status: String,
    search: String,
    active_tab: String,
    from_date: String,
    to_date: String,
    on_back: EventHandler<MouseEvent>,
    on_refresh: EventHandler<MouseEvent>,
    on_search: EventHandler<String>,
    on_from_date: EventHandler<String>,
    on_to_date: EventHandler<String>,
    on_tab: EventHandler<String>,
    on_quick_range: EventHandler<i64>,
    on_select: EventHandler<i32>,
    on_close_detail: EventHandler<MouseEvent>,
    on_print: EventHandler<ReceiptDetail>,
    on_refund: EventHandler<i32>,
) -> Element {
    let filtered_receipts = filter_receipts(&receipts, &search, &active_tab, &from_date, &to_date);
    let receipt_count = filtered_receipts.len();
    let sales_total = filtered_receipts
        .iter()
        .map(|receipt| receipt.total)
        .sum::<f64>();
    let discount_total = filtered_receipts
        .iter()
        .map(|receipt| receipt.discount_amount)
        .sum::<f64>();
    let refund_total = filtered_receipts
        .iter()
        .filter(|receipt| receipt.status.as_deref() == Some("refunded"))
        .map(|receipt| receipt.total)
        .sum::<f64>();
    let credit_total = filtered_receipts
        .iter()
        .filter(|receipt| receipt.payment_type.as_deref() == Some("Credit"))
        .map(|receipt| receipt.total)
        .sum::<f64>();
    let selected_id = selected.as_ref().map(|detail| detail.summary.id);
    let receipt_rows = filtered_receipts
        .into_iter()
        .map(|receipt| {
            let is_selected = selected_id == Some(receipt.id);
            (receipt, is_selected)
        })
        .collect::<Vec<_>>();

    rsx! {
        section { class: "receipts_page",
            div { class: "receipts_title panel",
                div {
                    strong { "Receipts" }
                    small { "Find a sale, review its items and payment details." }
                }
            }

            div { class: "receipt_stats",
                ReceiptStat { class_name: "blue", label: "Receipts", value: receipt_count.to_string() }
                ReceiptStat { class_name: "green", label: "Sales", value: format_ks(sales_total) }
                ReceiptStat { class_name: "orange", label: "Discount", value: format_ks(discount_total) }
                ReceiptStat { class_name: "red", label: "Refund", value: format_ks(refund_total) }
                ReceiptStat { class_name: "purple", label: "Credit", value: format_ks(credit_total) }
            }

            small { class: "receipts_note", "Summary for the selected date range, across all receipt tabs." }

            div { class: "receipt_filters panel",
                label {
                    span { "From" }
                    input {
                        r#type: "date",
                        value: "{from_date}",
                        oninput: move |event| on_from_date.call(event.value())
                    }
                }
                label {
                    span { "To" }
                    input {
                        r#type: "date",
                        value: "{to_date}",
                        oninput: move |event| on_to_date.call(event.value())
                    }
                }
                label { class: "receipt_search_box",
                    span { "Search" }
                    input {
                        placeholder: "Invoice or customer",
                        value: "{search}",
                        oninput: move |event| on_search.call(event.value())
                    }
                }
                button { class: "primary", onclick: move |event| on_refresh.call(event), "Apply filters" }
            }

            div { class: "receipts_body",
                div { class: "receipt_tabs",
                    button {
                        class: if active_tab == "receipts" { "active" } else { "" },
                        onclick: move |_| on_tab.call("receipts".to_string()),
                        "Receipts"
                    }
                    button {
                        class: if active_tab == "refunded" { "active" } else { "" },
                        onclick: move |_| on_tab.call("refunded".to_string()),
                        "Refunded"
                    }
                    button {
                        class: if active_tab == "discounted" { "active" } else { "" },
                        onclick: move |_| on_tab.call("discounted".to_string()),
                        "Discounted"
                    }
                    button {
                        class: if active_tab == "credit" { "active" } else { "" },
                        onclick: move |_| on_tab.call("credit".to_string()),
                        "Credit"
                    }
                }
                div { class: "receipt_quick_filters",
                    button { onclick: move |_| on_quick_range.call(1), "Today" }
                    button { onclick: move |_| on_quick_range.call(7), "Last 7 days" }
                    button { onclick: move |_| on_quick_range.call(30), "Last 30 days" }
                }
                div { class: "receipts_content",
                    section { class: "transactions_panel panel",
                        div { class: "transactions_head",
                            strong { "Transactions" }
                            small { "Select a receipt to view details" }
                        }
                        if !status.is_empty() {
                            small { class: "receipts_status", "{status}" }
                        }
                        div { class: "transactions_list",
                            if receipt_rows.is_empty() {
                                div { class: "empty receipts_empty",
                                    strong { "No receipts loaded" }
                                    span { "Open Receipts from the side menu or click Apply filters." }
                                }
                            } else {
                                for (receipt, is_selected) in receipt_rows {
                                    ReceiptRow {
                                        receipt,
                                        selected: is_selected,
                                        on_select
                                    }
                                }
                            }
                        }
                    }
                    ReceiptDetailPanel {
                        detail: selected,
                        on_close: on_close_detail,
                        on_print,
                        on_refund
                    }
                }
            }
        }
    }
}

#[component]
fn ReceiptStat(class_name: &'static str, label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "receipt_stat {class_name}",
            span { "{label}" }
            strong { "{value}" }
        }
    }
}

#[component]
fn ReceiptRow(receipt: SaleSummary, selected: bool, on_select: EventHandler<i32>) -> Element {
    let invoice = receipt
        .invoice_no
        .clone()
        .unwrap_or_else(|| format!("Sale #{}", receipt.id));
    let customer = receipt
        .customer_name
        .clone()
        .unwrap_or_else(|| "Walk-in Customer".to_string());
    let payment_type = receipt
        .payment_type
        .clone()
        .unwrap_or_else(|| "Cash".to_string());
    let status = receipt
        .status
        .clone()
        .unwrap_or_else(|| "completed".to_string());
    let created_at = receipt
        .created_at
        .map(format_receipt_datetime)
        .unwrap_or_else(|| "-".to_string());

    rsx! {
        button {
            class: if selected { "transaction_item active" } else { "transaction_item" },
            onclick: move |_| on_select.call(receipt.id),
            div {
                strong { "{invoice}" }
                small { "{created_at} · {customer}" }
            }
            div { class: "transaction_amount",
                strong { "{format_ks(receipt.total)}" }
                span { "{payment_type}" }
                em { "{status}" }
            }
        }
    }
}

#[component]
fn ReceiptDetailPanel(
    detail: Option<ReceiptDetail>,
    on_close: EventHandler<MouseEvent>,
    on_print: EventHandler<ReceiptDetail>,
    on_refund: EventHandler<i32>,
) -> Element {
    if let Some(detail) = detail {
        let summary = detail.summary.clone();
        let sale_id = summary.id;
        let invoice = summary
            .invoice_no
            .clone()
            .unwrap_or_else(|| format!("Sale #{}", summary.id));
        let customer = summary
            .customer_name
            .clone()
            .unwrap_or_else(|| "Walk-in Customer".to_string());
        let payment_type = summary
            .payment_type
            .clone()
            .unwrap_or_else(|| "Cash".to_string());
        let status = summary
            .status
            .clone()
            .unwrap_or_else(|| "completed".to_string());
        let created_at = summary
            .created_at
            .map(format_receipt_datetime)
            .unwrap_or_else(|| "-".to_string());
        let printable_detail = detail.clone();
        let item_rows = detail
            .items
            .clone()
            .into_iter()
            .map(|item| {
                (
                    item.product_name.unwrap_or_else(|| "Item".to_string()),
                    item.qty,
                    item.price,
                    item.total,
                )
            })
            .collect::<Vec<_>>();

        rsx! {
            section { class: "receipt_detail_panel panel",
                div { class: "receipt_detail_head",
                    div {
                        strong { "{invoice}" }
                        small { "{created_at} · {customer} · {status}" }
                    }
                    button { onclick: move |event| on_close.call(event), "Close" }
                }
                div { class: "receipt_items_table",
                    div { class: "receipt_item_header",
                        span { "Item" }
                        span { "Qty" }
                        span { "Price" }
                        span { "Total" }
                    }
                    for (item_name, qty, price, total) in item_rows {
                        div { class: "receipt_item_row",
                            span { "{item_name}" }
                            span { "{format_qty(qty)}" }
                            span { "{format_ks(price)}" }
                            span { "{format_ks(total)}" }
                        }
                    }
                }
                div { class: "receipt_payment_box",
                    div { span { "Payment method" } strong { "{payment_type}" } }
                    div { span { "Discount" } strong { "{format_ks(summary.discount_amount)}" } }
                    div { span { "Total" } strong { "{format_ks(summary.total)}" } }
                    div { span { "Paid" } strong { "{format_ks(summary.payment)}" } }
                    div { span { "Change" } strong { "{format_ks(summary.change_amount)}" } }
                }
                div { class: "receipt_detail_actions",
                    button {
                        class: "refund_button",
                        disabled: status == "refunded",
                        onclick: move |_| on_refund.call(sale_id),
                        "Refund"
                    }
                    button { onclick: move |_| on_print.call(printable_detail.clone()), "Print Receipt" }
                }
            }
        }
    } else {
        rsx! {
            section { class: "receipt_detail_panel panel",
                div { class: "empty receipts_empty",
                    strong { "Select a receipt" }
                    span { "Receipt details will appear here." }
                }
            }
        }
    }
}

#[component]
fn ServicePriceDialog(
    product: Product,
    price: String,
    on_price: EventHandler<String>,
    on_cancel: EventHandler<()>,
    on_add: EventHandler<()>,
) -> Element {
    let keypad_digits = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "00", "0"]
        .into_iter()
        .map(|digit| (digit.to_string(), price.clone()))
        .collect::<Vec<_>>();
    let backspace_price = price.clone();
    let can_add = price.parse::<f64>().unwrap_or(0.0) > 0.0;

    rsx! {
        div { class: "modal_backdrop service_backdrop",
            section { class: "service_dialog",
                div { class: "service_head",
                    div {
                        strong { "{product.name}" }
                        small { "Enter service price" }
                    }
                    button { onclick: move |_| on_cancel.call(()), "×" }
                }
                div { class: "service_body",
                    label { class: "service_price_field",
                        "Price"
                        input {
                            autofocus: true,
                            readonly: true,
                            value: "{price}",
                            onmounted: move |element| async move {
                                let _ = element.data().set_focus(true).await;
                            },
                            onkeydown: move |event| {
                                let key = event.data.key().to_string();
                                match key.as_str() {
                                    "Enter" => {
                                        if can_add {
                                            on_add.call(());
                                        }
                                    }
                                    "Escape" => on_cancel.call(()),
                                    "Backspace" => {
                                        let mut next = price.clone();
                                        next.pop();
                                        on_price.call(next);
                                    }
                                    "Delete" => on_price.call(String::new()),
                                    digit if digit.chars().all(|character| character.is_ascii_digit()) => {
                                        let mut next = price.clone();
                                        next.push_str(digit);
                                        on_price.call(only_digits(&next));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    div { class: "number_pad",
                        for (digit, current_price) in keypad_digits {
                            button {
                                onclick: move |_| {
                                    let mut next = current_price.clone();
                                    next.push_str(&digit);
                                    on_price.call(only_digits(&next));
                                },
                                "{digit}"
                            }
                        }
                        button {
                            onclick: move |_| {
                                let mut next = backspace_price.clone();
                                next.pop();
                                on_price.call(next);
                            },
                            "⌫"
                        }
                    }
                    button {
                        class: "service_clear",
                        onclick: move |_| on_price.call(String::new()),
                        "Clear"
                    }
                }
                div { class: "service_actions",
                    button { onclick: move |_| on_cancel.call(()), "⊗  Cancel" }
                    button {
                        class: "primary",
                        disabled: !can_add,
                        onclick: move |_| on_add.call(()),
                        "+  Add"
                    }
                }
            }
        }
    }
}

#[component]
fn VariantDialog(
    product: Product,
    selected_variant_id: Option<i32>,
    on_select: EventHandler<i32>,
    on_cancel: EventHandler<MouseEvent>,
    on_add: EventHandler<MouseEvent>,
) -> Element {
    let selected_in_stock = selected_variant_id
        .and_then(|id| {
            product
                .variants
                .iter()
                .find(|variant| variant.variant_id == id)
        })
        .map(|variant| variant.stock > 0.0)
        .unwrap_or(false);

    rsx! {
        div { class: "modal_backdrop service_backdrop",
            section { class: "variant_dialog",
                div { class: "service_head",
                    div {
                        strong { "{product.name}" }
                        small { "Choose one variant" }
                    }
                    button { onclick: move |event| on_cancel.call(event), "×" }
                }
                div { class: "variant_list",
                    for variant in product.variants.iter().cloned() {
                        VariantRow {
                            variant: variant.clone(),
                            selected: selected_variant_id == Some(variant.variant_id),
                            on_select: move |_| on_select.call(variant.variant_id)
                        }
                    }
                }
                div { class: "variant_actions",
                    button { onclick: move |event| on_cancel.call(event), "Cancel" }
                    button {
                        class: "primary",
                        disabled: !selected_in_stock,
                        onclick: move |event| on_add.call(event),
                        "+  Add"
                    }
                }
            }
        }
    }
}

#[component]
fn VariantRow(
    variant: ProductVariant,
    selected: bool,
    on_select: EventHandler<MouseEvent>,
) -> Element {
    let out_of_stock = variant.stock <= 0.0;
    let label = variant_label(&variant);
    let title = if label.is_empty() {
        variant
            .sku
            .clone()
            .unwrap_or_else(|| format!("Variant {}", variant.variant_id))
    } else {
        label
    };
    let subtitle = variant
        .sku
        .clone()
        .or(variant.barcode.clone())
        .unwrap_or_else(|| format!("ID {}", variant.variant_id));
    let row_class = if out_of_stock {
        "variant_row disabled"
    } else if selected {
        "variant_row selected"
    } else {
        "variant_row"
    };

    rsx! {
        button {
            class: row_class,
            disabled: out_of_stock,
            onclick: move |event| on_select.call(event),
            span { class: "variant_radio", aria_hidden: "true" }
            span { class: "variant_info",
                strong { "{title}" }
                small { "{subtitle}" }
            }
            span { class: "variant_meta",
                strong { "{format_ks(variant.price)}" }
                small { "Stock {format_qty(variant.stock)}" }
            }
        }
    }
}

#[component]
fn MessageBox(title: String, message: String, on_close: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { class: "modal_backdrop",
            section { class: "message_box",
                div { class: "message_icon", "!" }
                div { class: "message_body",
                    strong { "{title}" }
                    p { "{message}" }
                }
                div { class: "dialog_actions message_actions",
                    button {
                        class: "primary",
                        autofocus: true,
                        onclick: move |event| on_close.call(event),
                        "OK"
                    }
                }
            }
        }
    }
}

#[component]
fn ReceiptDialog(
    receipt: ReceiptData,
    settings: HashMap<String, String>,
    on_done: EventHandler<MouseEvent>,
) -> Element {
    let mut print_status = use_signal(String::new);

    rsx! {
        div { class: "modal_backdrop",
            section { class: "receipt_dialog",
                div { class: "dialog_head",
                    div {
                        strong { "Sale saved" }
                        small { "{receipt.invoice_no}" }
                    }
                    button { onclick: move |event| on_done.call(event), "x" }
                }
                div { class: "receipt_layout",
                    aside { class: "receipt_summary",
                        span { class: "receipt_badge", "Sale saved" }
                        h3 { "Payment summary" }
                        div { span { "Subtotal" } strong { "{format_ks(receipt.subtotal)}" } }
                        div { span { "Discount" } strong { "{format_ks(receipt.discount)}" } }
                        div { span { "Total" } strong { "{format_ks(receipt.total)}" } }
                        div { span { "Sale type" } strong { "{receipt.payment_type}" } }
                        div { span { "Payment" } strong { "{format_ks(receipt.payment)}" } }
                        div { span { "Change" } strong { "{format_ks(receipt.change)}" } }
                    }
                    div { class: "receipt_preview",
                        div { class: "receipt_paper",
                            h3 { "{setting_value(&settings, \"shop_name\", \"KAY POS\")}" }
                            small { "{receipt.invoice_no}" }
                            div { class: "receipt_rule" }
                            for line in receipt.lines.iter() {
                                div { class: "receipt_item",
                                    span { "{line.display_name()}" }
                                    small { "{line.qty} x {format_ks(line.unit_price())}" }
                                    strong { "{format_ks(line_total(line))}" }
                                }
                            }
                            div { class: "receipt_rule" }
                            if receipt.discount > 0.0 {
                                div { class: "receipt_total", span { "Discount" } strong { "{format_ks(receipt.discount)}" } }
                            }
                            div { class: "receipt_total", span { "Total" } strong { "{format_ks(receipt.total)}" } }
                        }
                    }
                }
                div { class: "dialog_actions",
                    if !print_status.read().is_empty() {
                        small { class: "print_status", "{print_status}" }
                    }
                    button { onclick: move |event| on_done.call(event), "Done" }
                    button {
                        class: "primary",
                        onclick: move |_| {
                            let receipt=receipt.clone();
                            let settings=settings.clone();
                            print_status.set("Sending receipt to printer...".into());
                            spawn(async move {
                                match receipt_printer::print(receipt, settings).await {
                                    Ok(()) => print_status.set("Receipt sent to printer".into()),
                                    Err(error) => print_status.set(format!("Print error: {error:#}")),
                                }
                            });
                        },
                        "Print Receipt"
                    }
                }
            }
        }
    }
}

fn quick_received_amounts(total: f64) -> Vec<f64> {
    if !total.is_finite() || total <= 0.0 {
        return Vec::new();
    }
    let step = if total < 5000.0 { 1000.0 } else { 5000.0 };
    let mut amounts = vec![
        total,
        (total / step).ceil() * step,
        (total / 10000.0).ceil() * 10000.0,
        500.0,
        1000.0,
        5000.0,
        10000.0,
    ];
    amounts.retain(|amount| *amount >= total);
    amounts.sort_by(f64::total_cmp);
    amounts.dedup();
    amounts
}

#[test]
fn quick_received_matches_cash_suggestions() {
    for (total, expected) in [
        (100.0, vec![100.0, 500.0, 1000.0, 5000.0, 10000.0]),
        (8000.0, vec![8000.0, 10000.0]),
        (14500.0, vec![14500.0, 15000.0, 20000.0]),
        (500.0, vec![500.0, 1000.0, 5000.0, 10000.0]),
        (2500.0, vec![2500.0, 3000.0, 5000.0, 10000.0]),
        (3000.0, vec![3000.0, 5000.0, 10000.0]),
        (4000.0, vec![4000.0, 5000.0, 10000.0]),
        (15000.0, vec![15000.0, 20000.0]),
        (20000.0, vec![20000.0]),
        (25100.0, vec![25100.0, 30000.0]),
        (2500.5, vec![2500.5, 3000.0, 5000.0, 10000.0]),
    ] {
        assert_eq!(quick_received_amounts(total), expected);
    }
    for total in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(quick_received_amounts(total).is_empty());
    }
}

fn normalize_scan(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|c| {
            if ('\u{1040}'..='\u{1049}').contains(&c) {
                char::from(b'0' + (c as u32 - 0x1040) as u8)
            } else {
                c
            }
        })
        .collect()
}

#[test]
fn scanner_normalizes_myanmar_digits() {
    assert_eq!(normalize_scan(" \u{1041}\u{1042}\u{1043} "), "123");
    assert_eq!(normalize_scan(" ITM-00467 "), "ITM-00467");
}

fn checkout_tax(settings: &HashMap<String, String>, amount: f64) -> f64 {
    pos_core::sales::receipt_tax(
        amount,
        settings.get("tax_enabled").is_some_and(|v| v == "1"),
        settings
            .get("tax_rate")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0),
    )
    .unwrap_or(0.0)
}

fn appearance_mode(settings: &HashMap<String, String>) -> &'static str {
    if settings
        .get("follow_system_theme")
        .is_some_and(|v| matches!(v.as_str(), "1" | "true"))
    {
        "system"
    } else if settings
        .get("theme")
        .is_some_and(|v| v.eq_ignore_ascii_case("dark"))
    {
        "dark"
    } else {
        "light"
    }
}

#[test]
fn appearance_theme_selection() {
    let mut settings = HashMap::new();
    assert_eq!(appearance_mode(&settings), "light");
    settings.insert("theme".into(), "Light Gray".into());
    assert_eq!(appearance_mode(&settings), "light");
    settings.insert("theme".into(), "Dark".into());
    assert_eq!(appearance_mode(&settings), "dark");
    settings.insert("follow_system_theme".into(), "1".into());
    assert_eq!(appearance_mode(&settings), "system");
}

fn current_receipt_date() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn parse_receipt_date(value: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

#[test]
fn receipt_picker_date_format() {
    assert_eq!(
        parse_receipt_date("2026-09-17"),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 17)
    );
    assert!(parse_receipt_date("").is_none());
    assert!(parse_receipt_date("2026-02-30").is_none());
    assert!(parse_receipt_date(&current_receipt_date()).is_some());
}

fn filter_receipts(
    receipts: &[SaleSummary],
    search: &str,
    active_tab: &str,
    from_date: &str,
    to_date: &str,
) -> Vec<SaleSummary> {
    let search = search.trim().to_lowercase();
    let from_date = parse_receipt_date(from_date);
    let to_date = parse_receipt_date(to_date);

    receipts
        .iter()
        .filter(|receipt| {
            let receipt_date = receipt.created_at.map(|value| value.date());
            let date_matches = receipt_date
                .map(|date| {
                    from_date.map(|from| date >= from).unwrap_or(true)
                        && to_date.map(|to| date <= to).unwrap_or(true)
                })
                .unwrap_or(true);
            let status = receipt.status.as_deref().unwrap_or_default().to_lowercase();
            let payment_type = receipt
                .payment_type
                .as_deref()
                .unwrap_or_default()
                .to_lowercase();
            let tab_matches = match active_tab {
                "refunded" => status == "refunded",
                "discounted" => receipt.discount_amount > 0.0,
                "credit" => payment_type == "credit",
                _ => status != "refunded",
            };
            let search_matches = search.is_empty()
                || receipt
                    .invoice_no
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&search)
                || receipt
                    .customer_name
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&search)
                || receipt
                    .payment_type
                    .as_deref()
                    .unwrap_or_default()
                    .to_lowercase()
                    .contains(&search);

            date_matches && tab_matches && search_matches
        })
        .cloned()
        .collect()
}


#[component]
fn DbSettingsDialog(
    form: DbForm,
    on_cancel: EventHandler<MouseEvent>,
    on_save: EventHandler<DbForm>,
) -> Element {
    let mut draft = use_signal(|| form);

    rsx! {
        div { class: "modal_backdrop",
            section { class: "db_dialog",
                div { class: "dialog_head",
                    div {
                        strong { "PostgreSQL Connection" }
                        small { "Connect this POS client to the Server PC database." }
                    }
                    button { onclick: move |event| on_cancel.call(event), "x" }
                }
                div { class: "dialog_grid",
                    label {
                        "Server IP / Host"
                        input {
                            value: "{draft.read().host}",
                            oninput: move |event| draft.write().host = event.value()
                        }
                    }
                    label {
                        "Port"
                        input {
                            value: "{draft.read().port}",
                            oninput: move |event| draft.write().port = event.value()
                        }
                    }
                    label {
                        "Database"
                        input {
                            value: "{draft.read().database}",
                            oninput: move |event| draft.write().database = event.value()
                        }
                    }
                    label {
                        "Username"
                        input {
                            value: "{draft.read().username}",
                            oninput: move |event| draft.write().username = event.value()
                        }
                    }
                    label { class: "full",
                        "Password"
                        input {
                            r#type: "password",
                            value: "{draft.read().password}",
                            oninput: move |event| draft.write().password = event.value()
                        }
                    }
                }
                div { class: "dialog_actions",
                    button { onclick: move |event| on_cancel.call(event), "Cancel" }
                    button {
                        class: "primary",
                        onclick: move |_| on_save.call(draft.read().clone()),
                        "Save & Test"
                    }
                }
            }
        }
    }
}

impl DbForm {
    fn database_config(&self) -> anyhow::Result<DatabaseConfig> {
        let port = self.port.trim().parse::<u16>()?;
        Ok(DatabaseConfig {
            database_url: format!(
                "postgresql://{}:{}@{}:{}/{}",
                url_encode(self.username.trim()),
                url_encode(&self.password),
                self.host.trim(),
                port,
                self.database.trim()
            ),
            max_connections: 5,
        })
    }
}

fn load_saved_db_form() -> DbForm {
    fs::read_to_string(db_config_path())
        .ok()
        .and_then(|content| serde_json::from_str::<DbForm>(&content).ok())
        .or_else(form_from_env)
        .unwrap_or_default()
}

fn save_db_form(form: &DbForm) -> anyhow::Result<()> {
    let path = db_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(form)?)?;
    Ok(())
}

fn db_config_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("kay-pos-db.json")
}

fn form_from_env() -> Option<DbForm> {
    let url = std::env::var("ZAY_POS_DATABASE_URL").ok()?;
    parse_database_url(&url)
}

fn parse_database_url(url: &str) -> Option<DbForm> {
    let rest = url
        .strip_prefix("postgresql://")
        .or_else(|| url.strip_prefix("postgres://"))?;
    let (credentials, host_part) = rest.split_once('@')?;
    let (username, password) = credentials.split_once(':')?;
    let (host_port, database_part) = host_part.split_once('/')?;
    let (host, port) = host_port
        .split_once(':')
        .map(|(host, port)| (host.to_string(), port.to_string()))
        .unwrap_or_else(|| (host_port.to_string(), "5432".to_string()));
    Some(DbForm {
        host,
        port,
        database: database_part
            .split('?')
            .next()
            .unwrap_or(database_part)
            .to_string(),
        username: username.to_string(),
        password: password.to_string(),
    })
}

fn url_encode(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('@', "%40")
        .replace(':', "%3A")
        .replace('/', "%2F")
}

async fn load_cached_product_image(form: DbForm, id: i32) -> anyhow::Result<Option<String>> {
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    type ImageCache = HashMap<(String, i32), (Instant, Option<String>)>;
    static CACHE: OnceLock<Mutex<ImageCache>> = OnceLock::new();
    let config = form.database_config()?;
    let key = (config.database_url.clone(), id);
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, image)) = cache
        .lock()
        .unwrap()
        .get(&key)
        .filter(|(time, _)| time.elapsed() < Duration::from_secs(60))
    {
        return Ok(image.clone());
    }
    let pool = connect(&config).await?;
    let image = pos_core::db::product_image(&pool, id).await?;
    let mut images = cache.lock().unwrap();
    images.retain(|_, (time, _)| time.elapsed() < Duration::from_secs(60));
    let bytes: usize = images
        .values()
        .map(|(_, image)| image.as_ref().map_or(0, String::len))
        .sum();
    if bytes + image.as_ref().map_or(0, String::len) > 32 * 1024 * 1024 {
        images.clear();
    }
    if image.as_ref().map_or(0, String::len) <= 32 * 1024 * 1024 {
        images.insert(key, (Instant::now(), image.clone()));
    }
    Ok(image)
}

#[component]
fn CatalogImage(id: i32, name: String) -> Element {
    let source = use_context::<Signal<DbForm>>();
    let image = use_resource(move || {
        let form = source.read().clone();
        async move { load_cached_product_image(form, id).await }
    });
    let state = image.read();
    match state.as_ref() {
        Some(Ok(Some(url))) => rsx! { img { src: "{url}", alt: "{name}", loading: "lazy" } },
        Some(Err(_)) => rsx! { span { title: "Image could not be loaded", "KAY" } },
        _ => rsx! { span { "KAY" } },
    }
}

#[component]
fn ProductCard(product: Product, on_add: EventHandler<MouseEvent>) -> Element {
    let source = use_context::<Signal<DbForm>>();
    let id = product.id;
    let image = use_resource(move || {
        let form = source.read().clone();
        async move { load_cached_product_image(form, id).await.ok().flatten() }
    });
    let image_url = product
        .image_data_url
        .clone()
        .or_else(|| image.read().as_ref().cloned().flatten());
    let category = product
        .category_name
        .clone()
        .unwrap_or_else(|| "No category".to_string());
    let service = is_service_product(&product);
    let variants = is_variant_product(&product);
    let available_stock = available_product_stock(&product);
    let stock = if service {
        0
    } else {
        available_stock.round() as i64
    };
    let wholesale = !product.price_tiers.is_empty()
        || product
            .variants
            .iter()
            .any(|variant| variant.wholesale_min_qty > 0 && variant.wholesale_price > 0.0);
    let low_stock =
        !service && pos_core::stock_alerts::is_low(available_stock, product.low_stock);
    let out_of_stock = !service && available_stock <= 0.0;
    let tone = category_tone(product.category_id, &category);

    rsx! {
        button {
            class: if out_of_stock { "product_card out_of_stock" } else { "product_card" },
            disabled: out_of_stock,
            onclick: move |event| on_add.call(event),
            div { class: "product_image",
                if let Some(url) = image_url {
                    img { src: "{url}", alt: "{product.name}", loading: "lazy" }
                } else {
                    span { "KAY" }
                }
                if service {
                    em { class: "product_badge", "Service" }
                } else if variants {
                    em { class: "product_badge", "Variant" }
                } else if wholesale {
                    em { class: "product_badge", "Wholesale" }
                } else if out_of_stock {
                    em { class: "product_badge out", "Out" }
                } else if low_stock {
                    em { class: "product_badge low", "Low stock" }
                }
            }
            div { class: "product_body",
                strong { title: "{product.name}", "{product.name}" }
                span { class: "product_chip tone{tone}", title: "{category}", "{category}" }
                em { "{format_ks(product.price)}" }
                if !service && available_stock > 0.0 {
                    small { class: "stock_pill", "{stock}" }
                }
            }
        }
    }
}

fn add_to_cart(cart: &mut Signal<Vec<CartLine>>, product: Product) {
    let variant = if is_variant_product(&product) {
        product
            .variants
            .iter()
            .find(|variant| variant.stock > 0.0)
            .or_else(|| product.variants.first())
            .cloned()
    } else {
        None
    };
    let mut lines = cart.write();
    if let Some(line) = lines.iter_mut().find(|line| {
        line.product.id == product.id
            && line.variant.as_ref().map(|variant| variant.variant_id)
                == variant.as_ref().map(|variant| variant.variant_id)
    }) {
        if can_increase_line(line) {
            line.qty += 1.0;
        }
    } else {
        let candidate = CartLine {
            product,
            variant,
            qty: 1.0,
            unit_price_override: None,
        };
        if !has_stock_for_line(&candidate) {
            return;
        }
        lines.push(CartLine { ..candidate });
    }
}

fn add_service_to_cart(cart: &mut Signal<Vec<CartLine>>, product: Product, price: f64) {
    cart.write().push(CartLine {
        product,
        variant: None,
        qty: 1.0,
        unit_price_override: Some(price),
    });
}

fn add_variant_to_cart(
    cart: &mut Signal<Vec<CartLine>>,
    product: Product,
    variant: ProductVariant,
) {
    let mut lines = cart.write();
    if let Some(line) = lines.iter_mut().find(|line| {
        line.product.id == product.id
            && line.variant.as_ref().map(|item| item.variant_id) == Some(variant.variant_id)
    }) {
        if can_increase_line(line) {
            line.qty += 1.0;
        }
    } else {
        let candidate = CartLine {
            product,
            variant: Some(variant),
            qty: 1.0,
            unit_price_override: None,
        };
        if has_stock_for_line(&candidate) {
            lines.push(candidate);
        }
    }
}

fn remove_from_cart(cart: &mut Signal<Vec<CartLine>>, index: usize) {
    let mut lines = cart.write();
    if index < lines.len() {
        lines.remove(index);
    }
}

fn increment_cart(cart: &mut Signal<Vec<CartLine>>, index: usize) {
    let mut lines = cart.write();
    if let Some(line) = lines.get_mut(index) {
        if can_increase_line(line) {
            line.qty += 1.0;
        }
    }
}

fn decrement_cart(cart: &mut Signal<Vec<CartLine>>, index: usize) {
    let mut lines = cart.write();
    if let Some(line) = lines.get_mut(index) {
        line.qty -= 1.0;
        if line.qty <= 0.0 {
            lines.remove(index);
        }
    }
}

fn available_product_stock(product: &Product) -> f64 {
    if is_service_product(product) {
        return f64::INFINITY;
    }

    if is_variant_product(product) && !product.variants.is_empty() {
        return product
            .variants
            .iter()
            .map(|variant| variant.stock.max(0.0))
            .sum();
    }

    product.stock.max(0.0)
}

fn available_line_stock(line: &CartLine) -> f64 {
    if is_service_product(&line.product) {
        return f64::INFINITY;
    }

    line.variant
        .as_ref()
        .map(|variant| variant.stock.max(0.0))
        .unwrap_or_else(|| line.product.stock.max(0.0))
}

fn has_stock_for_line(line: &CartLine) -> bool {
    line.qty <= available_line_stock(line)
}

fn can_increase_line(line: &CartLine) -> bool {
    line.qty + 1.0 <= available_line_stock(line)
}

impl CartLine {
    fn base_price(&self) -> f64 {
        self.variant
            .as_ref()
            .map(|variant| variant.price)
            .or(self.unit_price_override)
            .unwrap_or(self.product.price)
    }

    fn unit_price(&self) -> f64 {
        if let Some(price) = self.unit_price_override {
            return price;
        }

        if let Some(variant) = &self.variant {
            if variant.wholesale_min_qty > 0
                && variant.wholesale_price > 0.0
                && self.qty >= f64::from(variant.wholesale_min_qty)
            {
                return variant.wholesale_price;
            }
            return variant.price;
        }

        if let Some(tier) = best_price_tier(&self.product.price_tiers, self.qty) {
            return tier.unit_price;
        }

        self.product.price
    }

    fn wholesale_regular_price(&self) -> f64 {
        let base = self.base_price();
        if self.unit_price() < base {
            base
        } else {
            0.0
        }
    }

    fn wholesale_savings(&self) -> f64 {
        let base = self.base_price();
        ((base - self.unit_price()).max(0.0)) * self.qty
    }

    fn wholesale_tier_min_qty(&self) -> Option<i32> {
        if let Some(variant) = &self.variant {
            if variant.wholesale_min_qty > 0
                && variant.wholesale_price > 0.0
                && self.qty >= f64::from(variant.wholesale_min_qty)
            {
                return Some(variant.wholesale_min_qty);
            }
            return None;
        }

        best_price_tier(&self.product.price_tiers, self.qty).map(|tier| tier.min_qty)
    }

    fn wholesale_unit_label(&self) -> Option<String> {
        best_price_tier(&self.product.price_tiers, self.qty)
            .and_then(|tier| tier.unit_label.clone())
            .filter(|label| !label.trim().is_empty())
    }

    fn variant_label(&self) -> Option<String> {
        self.variant
            .as_ref()
            .map(variant_label)
            .filter(|label| !label.is_empty())
    }

    fn display_name(&self) -> String {
        if let Some(label) = self.variant_label() {
            format!("{} ({label})", self.product.name)
        } else if let Some(min_qty) = self.wholesale_tier_min_qty() {
            format!("{} (Wholesale {min_qty}+)", self.product.name)
        } else {
            self.product.name.clone()
        }
    }
}

fn line_total(line: &CartLine) -> f64 {
    line.unit_price() * line.qty
}

fn best_price_tier(tiers: &[ProductPriceTier], qty: f64) -> Option<&ProductPriceTier> {
    tiers
        .iter()
        .filter(|tier| tier.unit_price > 0.0 && qty >= f64::from(tier.min_qty))
        .max_by(|left, right| {
            left.min_qty
                .cmp(&right.min_qty)
                .then_with(|| right.unit_price.total_cmp(&left.unit_price))
        })
}

fn sold_by_mode(product: &Product) -> String {
    product
        .sold_by
        .as_deref()
        .unwrap_or("Each")
        .trim()
        .replace('_', " ")
        .to_lowercase()
}

fn is_service_product(product: &Product) -> bool {
    let mode = sold_by_mode(product);
    mode == "service" || mode == "services" || mode.ends_with(" service")
}

fn is_variant_product(product: &Product) -> bool {
    let mode = sold_by_mode(product);
    mode == "variant" || mode == "variants" || mode.ends_with(" variants")
}

fn variant_label(variant: &ProductVariant) -> String {
    [variant.color.as_deref(), variant.size.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn only_digits(value: &str) -> String {
    let cleaned = value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    let trimmed = cleaned.trim_start_matches('0').to_string();
    if cleaned.is_empty() || !trimmed.is_empty() {
        trimmed
    } else {
        "0".to_string()
    }
}

fn decimal_input(value: &str) -> String {
    let mut cleaned = String::new();
    let mut has_dot = false;
    for character in value.chars() {
        if character.is_ascii_digit() {
            cleaned.push(character);
        } else if character == '.' && !has_dot {
            cleaned.push(character);
            has_dot = true;
        }
    }
    if cleaned.starts_with('.') {
        cleaned.insert(0, '0');
    }
    cleaned
}

fn format_qty(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{:.0}", value)
    } else {
        format!("{value:.2}")
    }
}

fn format_receipt_datetime(value: chrono::NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_ks(value: f64) -> String {
    let rounded = value.round() as i64;
    let chars = rounded.abs().to_string().chars().rev().collect::<Vec<_>>();
    let mut grouped = String::new();
    for index in 0..chars.len() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(chars[index]);
    }
    let formatted = grouped.chars().rev().collect::<String>();
    if rounded < 0 {
        format!("-{formatted} Ks")
    } else {
        format!("{formatted} Ks")
    }
}

fn category_tone(id: Option<i32>, name: &str) -> usize {
    if let Some(id) = id {
        return id.unsigned_abs() as usize % 6;
    }
    let mut hash = 0usize;
    for byte in name.as_bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(*byte as usize);
    }
    hash % 6
}
