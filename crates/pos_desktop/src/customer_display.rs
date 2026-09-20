use dioxus::prelude::*;
use dioxus_desktop::{Config, DesktopContext, WindowBuilder};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Default, PartialEq)]
pub struct DisplayLine {
    pub name: String,
    pub quantity: String,
    pub amount: String,
}

#[derive(Clone, Default, PartialEq)]
pub struct DisplaySnapshot {
    pub shop: String,
    pub lines: Vec<DisplayLine>,
    pub total: String,
    pub received: Option<String>,
    pub change: Option<String>,
    pub completed: bool,
}

type SharedDisplay = Arc<Mutex<DisplaySnapshot>>;

pub fn use_customer_display(snapshot: DisplaySnapshot, enabled: Signal<bool>) -> Signal<bool> {
    let mut visible = use_signal(|| false);
    let shared = use_hook(|| Arc::new(Mutex::new(DisplaySnapshot::default())));
    if let Ok(mut state) = shared.lock() {
        *state = snapshot;
    }
    let child = use_hook(|| Rc::new(RefCell::new(None::<DesktopContext>)));
    let cleanup = child.clone();
    use_drop(move || {
        if let Some(window) = cleanup.borrow_mut().take() {
            window.close();
        }
    });
    use_future(move || {
        let shared = shared.clone();
        let child = child.clone();
        async move {
            let cashier = dioxus_desktop::window();
            let mut target = None;
            loop {
                let primary = cashier
                    .current_monitor()
                    .or_else(|| cashier.primary_monitor());
                let secondary = cashier.available_monitors().find(|monitor| {
                    *enabled.peek()
                        && primary
                            .as_ref()
                            .is_some_and(|current| monitor.position() != current.position())
                });
                let next = secondary
                    .as_ref()
                    .map(|monitor| (monitor.position(), monitor.size()));
                if next != target {
                    if let Some(window) = child.borrow_mut().take() {
                        window.close();
                    }
                    if let Some(monitor) = secondary {
                        let mut dom = VirtualDom::new(CustomerDisplay);
                        dom.insert_any_root_context(Box::new(shared.clone()));
                        let window = WindowBuilder::new()
                            .with_window_icon(Some(crate::app_icon::icon()))
                            .with_title("KAY POS - Customer Display")
                            .with_decorations(false)
                            .with_position(monitor.position())
                            .with_inner_size(monitor.size())
                            .with_fullscreen(Some(
                                dioxus_desktop::tao::window::Fullscreen::Borderless(Some(monitor)),
                            ))
                            .with_focused(false);
                        let display = cashier
                            .new_window(dom, Config::new().with_window(window).with_menu(None))
                            .await;
                        *child.borrow_mut() = Some(display);
                    }
                    target = next;
                    visible.set(target.is_some());
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    });
    visible
}

#[component]
fn CustomerDisplay() -> Element {
    let shared = use_context::<SharedDisplay>();
    let mut snapshot = use_signal(DisplaySnapshot::default);
    use_future(move || {
        let shared = shared.clone();
        async move {
            loop {
                let next = shared.lock().ok().map(|state| state.clone());
                if let Some(next) = next {
                    if *snapshot.peek() != next {
                        snapshot.set(next);
                    }
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    });
    let state = snapshot.read();
    rsx! {
        style { {include_str!("../assets/customer-display.css")} }
        main {
            header { "{state.shop}" }
            if state.lines.is_empty() {
                section { class: "welcome", h1 { "Welcome" } }
            } else {
                section { class: "sale",
                    div { class: "items",
                        table {
                            thead { tr { th { "Item" } th { "Qty" } th { "Amount" } } }
                            tbody { for line in &state.lines {
                                tr { td { "{line.name}" } td { "{line.quantity}" } td { "{line.amount}" } }
                            } }
                        }
                    }
                    footer {
                        div { class: "total", span { "Total" } strong { "{state.total}" } }
                        if let Some(received) = &state.received { div { span { "Received" } strong { "{received}" } } }
                        if let Some(change) = &state.change { div { span { "Change" } strong { "{change}" } } }
                        if state.completed { p { "Thank you" } }
                    }
                }
            }
        }
    }
}
