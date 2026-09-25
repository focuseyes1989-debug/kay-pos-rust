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
        "calendar", "check_circle", "visibility", "backup", "shopping_cart")
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
        "Dashboard"=>"dashboard", "Sales"=>"point_of_sale", "Receipts"=>"receipt",
        "Products"=>"products", "Inventory"=>"inventory", "Categories"=>"category",
        "Locations"=>"location_on", "Customers"=>"person", "Employees"=>"groups",
        "Suppliers"=>"supplier", "Expenses"=>"payments", "Sale Summary"=>"analytics",
        "Service Order"|"Service Orders"=>"orders", "Settings Center"=>"settings",
        "Sign out"=>"logout", "Sign in"=>"login", "Refresh"=>"refresh",
        "Search"|"Apply filters"=>"search", "Reset"|"Reverse"=>"undo",
        "Edit"=>"edit", "Cancel"=>"cancel", "Close"|"×"|"✕"=>"close",
        "Print Receipt"|"Print"=>"print", "Export Excel"=>"file_export",
        "View Movements"|"View ledger"|"Ledger"=>"history",
        "Payment Collection"=>"payments", "Credit Sale"=>"credit_card",
        "Outstanding Report"=>"analytics", "Choose File"=>"folder_open",
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
