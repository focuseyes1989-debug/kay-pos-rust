use base64::{engine::general_purpose::STANDARD, Engine};
use dioxus::prelude::*;

fn source(name: &str) -> Option<&'static str> {
    macro_rules! assets {
        ($($name:literal),+ $(,)?) => { match name {
            $($name => Some(include_str!(concat!("../../../assets/icons/", $name, ".svg"))),)+
            _ => None,
        }};
    }
    assets!("add", "edit", "delete", "save", "cancel", "close", "refresh", "search",
        "print", "file_export", "history", "undo", "logout", "login", "settings",
        "dashboard", "point_of_sale", "receipt", "products", "inventory", "category",
        "location_on", "person", "groups", "supplier", "payments", "credit_card",
        "analytics", "orders", "folder_open", "download_done", "upload_file", "tv_displays",
        "calendar", "chevron_down", "check_circle", "visibility", "backup", "shopping_cart", "arrow_circle_left", "arrow_circle_right")
}

#[component]
pub fn Icon(name: String) -> Element {
    let Some(svg)=source(&name) else { return rsx! {}; };
    let url=format!("data:image/svg+xml;base64,{}",STANDARD.encode(svg));
    rsx! { span {class:"ui_svg_icon",aria_hidden:"true",style:"mask-image:url('{url}');-webkit-mask-image:url('{url}');"} }
}

fn for_label(label: &str) -> Option<&'static str> {
    let label=label.trim().trim_start_matches('+').trim();
    Some(match label {
        "OK"|"Apply"=>"check_circle", "Back"=>"arrow_circle_left",
        "Browse"=>"folder_open", "Settings"|"Printing Preferences"|"Open DB Settings"=>"settings",
        "Remove Photo"|"Remove Image"=>"delete", "Edit supplier"=>"edit",
        "Retry"|"Retry original request"|"Retry original sale"|"Retry same request"|"Refresh Printers"=>"refresh",
        "Check and cancel unsaved request"|"Check and cancel unsaved sale"=>"cancel",
        "Recover saved cart"=>"shopping_cart", "Return saved hold"=>"undo",
        "Stock Transfer"|"Transfer"=>"inventory", "Barcode Print"|"Test Print"=>"print",
        "Store template"=>"save", "Reset layout"|"Reset Changes"=>"undo",
        "AI"|"Kay Assistant"=>"analytics", "Dashboard"=>"dashboard", "Sales"=>"point_of_sale", "Receipts"=>"receipt",
        "Products"=>"products", "Inventory"=>"inventory", "Categories"=>"category",
        "Locations"=>"location_on", "Customers"=>"person", "Employees"=>"groups",
        "Suppliers"=>"supplier", "Expenses"=>"payments", "Sale Summary"=>"analytics",
        "Service Order"|"Service Orders"|"ERP Operations"=>"orders", "Settings Center"=>"settings",
        "Sign out"=>"logout", "Sign in"=>"login", "Refresh"=>"refresh",
        "Search"|"Apply filters"=>"search", "Reset"|"Reverse"=>"undo",
        "Edit"=>"edit", "Cancel"=>"cancel", "Close"|"×"|"✕"=>"close",
        "Print Receipt"|"Print"=>"print", "Export Excel"=>"file_export",
        "View Movements"|"View ledger"|"Ledger"=>"history",
        "Payment Collection"=>"payments", "Credit Sale"=>"credit_card",
        "Serial & Warranty"=>"inventory",
        "Reports"|"Outstanding Report"=>"analytics", "Choose File"=>"folder_open",
        "Check compatibility"=>"check_circle",
        "Purchases"=>"orders", "Add purchase order"|"Add item"=>"add",
        "Discounts"=>"payments", "Attachments"=>"folder_open", "Upload"=>"upload_file",
        "Hold sale"=>"save", "Held sales"|"Points history"|"Activity Log"=>"history", "Resume sale"=>"shopping_cart",
        "Category groups"=>"category", "Add group"=>"add",
        "Download"=>"download_done", "Mark read"=>"check_circle", "Check alerts"=>"check_circle",
        "Receive Order"=>"inventory", "Supplier payment"=>"payments",
        "Save order"=>"save", "View details"=>"visibility",
        "Open Cashdrawer"=>"payments", "Show Customer Display"=>"tv_displays",
        "Sync Attendance"=>"refresh", "Test Connection"|"Test TCP Connection"=>"check_circle",
        "Check for Updates"=>"refresh", "Download & Install"=>"download_done",
        "Restart KAY POS"=>"refresh", "Save"=>"save", "New"=>"add",
        s if s.starts_with("Add ")=>"add",
        s if s.starts_with("Save ")=>"save",
        s if s.starts_with("Delete")=>"delete",
        _=>return None,
    })
}

#[component]
pub fn ActionLabel(label: String) -> Element {
    if label.trim() == "Close" {
        return rsx! { span { class:"ui_action_label ui_close_label", role:"img", aria_label:"Close", Icon {name:"close"} } };
    }
    let text = display_label(&label);
    rsx! { span {class:"ui_action_label",
        if let Some(name)=for_label(&label) { Icon {name} }
        span {"{text}"}
    } }
}

fn display_label(label: &str) -> &str {
    if for_label(label) == Some("add") {
        label.trim().trim_start_matches('+').trim_start()
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn add_icon_replaces_only_the_redundant_plus() {
        for label in ["+ Add Item", "+ Add customer", "+ Add expense", "+ Add variant", "+ Add tier"] {
            assert_eq!(super::display_label(label), label.trim_start_matches("+ "));
            assert_eq!(super::for_label(label), Some("add"));
        }
        assert_eq!(super::display_label("Add Item"), "Add Item");
        assert_eq!(super::display_label("New"), "New");
        assert_eq!(super::display_label("+ Unknown"), "+ Unknown");
    }
    #[test]
    fn action_assets_are_embedded_and_unknown_labels_stay_plain() {
        for label in ["Dashboard","Save Item","Delete","+ Add customer","Sign out","Export Excel","Sync Attendance","Print Receipt"] {
            let name=super::for_label(label).unwrap();
            assert!(super::source(name).unwrap().contains("<svg"));
        }
        assert!(super::for_label("A customer's product name").is_none());
        assert!(super::source("../../unknown").is_none());
    }
}

/// Shared semantic icons for card headings; text remains accessible.
#[component]
pub fn CardLabel(label: String) -> Element {
    let lower=label.to_lowercase();
    let (name,tone)=if lower.contains("refund") {("undo","pink")}
    else if lower.contains("credit") || lower.contains("outstanding") || lower.contains("advance") {("credit_card","purple")}
    else if lower.contains("expense") || lower.contains("spen") {("payments","amber")}
    else if lower.contains("profit") || lower.contains("received") || lower.contains("paid") || lower.contains("delivered") {("payments","teal")}
    else if lower.contains("discount") {("payments","amber")}
    else if lower.contains("receipt") {("receipt","blue")}
    else if lower.contains("employee") || lower.contains("customer") {("groups","teal")}
    else if lower.contains("stock") || lower.contains("cost") {("inventory","amber")}
    else if lower.contains("categor") {("category","purple")}
    else if lower.contains("budget") {("payments","blue")}
    else if lower.contains("alert") || lower.contains("unread") || lower.contains("expir") || lower.contains("dead") {("check_circle","pink")}
    else if lower.contains("leave") {("calendar","purple")}
    else if lower.contains("sale") {("analytics","blue")}
    else {("dashboard","blue")};
    rsx! {span {class:"card_label","data-tone":tone,Icon {name} span {"{label}"}}}
}


fn tab_icon(label: &str) -> &'static str {
    match label {
        "Receipts" => "receipt",
        "Refunded" => "undo",
        "Discounted" | "Discount" | "Wholesale" => "payments",
        "Credit" | "Receivables" | "Advances & Commission" => "credit_card",
        "Expenses" | "Budgets" | "Payments" | "Payables" | "Payroll" | "Cash Sessions" => "payments",
        "Alerts" => "check_circle",
        "Comparison" | "Summary" | "Sales" | "Profit & Loss" | "Financial Summary" | "Performance" => "analytics",
        "Categories" | "Categories (Sales)" | "Categories (Expense)" | "Spending by Category" | "Parent" => "category",
        "Expense History" | "Purchase History" | "Supplier Ledger" => "history",
        "Products & stock" | "Stock value" | "Stock In" | "Stock Out" => "inventory",
        "Adjustment" => "edit",
        "Location" | "Stock by location" | "Manage locations" => "location_on",
        "Products" | "Items" => "products",
        "Daily" | "Attendance" | "Shifts" | "Shift Assignments" | "Leave" => "calendar",
        "Employees" => "groups",
        "Documents" => "folder_open",
        "Commission Rules" => "settings",
        "Purchase Orders" | "Jobs" => "orders",
        "Design Prompts" => "products",
        _ => "dashboard",
    }
}

/// Decorative icons keep the existing visible tab text and accessible name.
#[component]
pub fn TabLabel(label: String) -> Element {
    rsx! {span {class:"ui_tab_label",Icon {name:tab_icon(&label)} span {"{label}"}}}
}
