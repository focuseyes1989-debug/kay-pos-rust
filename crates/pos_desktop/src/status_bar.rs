use super::*;

#[component]
pub fn StatusBar(form: DbForm, operator: String, activity: String) -> Element {
    let mut refresh = use_signal(|| 0u64);
    let mut clock = use_signal(chrono::Local::now);
    use_future(move || async move {
        loop {
            sleep(Duration::from_secs(1)).await;
            clock.set(chrono::Local::now());
        }
    });
    use_future(move || async move {
        loop {
            sleep(Duration::from_secs(15)).await;
            refresh += 1;
        }
    });
    let source = form.clone();
    let health = use_resource(move || {
        let _ = refresh();
        let form = source.clone();
        async move {
            let start = std::time::Instant::now();
            let result = async { pos_core::db::ping(&form.database_config()?).await };
            match tokio::time::timeout(Duration::from_secs(4), result).await {
                Ok(Ok(())) => Ok(start.elapsed().as_millis()),
                Ok(Err(_)) => Err("Database unavailable. Check the server or network.".to_string()),
                Err(_) => Err("Database check timed out.".to_string()),
            }
        }
    });
    let checking = !health.finished();
    let state = health.read();
    let (kind, label, detail) = match state.as_ref() {
        _ if checking => (
            "checking",
            "Checking database".to_string(),
            "Checking PostgreSQL connectivity".to_string(),
        ),
        Some(Ok(ms)) => (
            "online",
            format!("Database connected · {ms} ms"),
            format!("{}:{} / {}", form.host, form.port, form.database),
        ),
        Some(Err(message)) => (
            "offline",
            "Database unavailable".to_string(),
            message.clone(),
        ),
        None => ("checking", "Checking database".to_string(), String::new()),
    };
    let clock_text = clock().format("%d %b %Y  %H:%M:%S").to_string();
    rsx! {
        footer { class: "app_status_bar",
            div { class: "status_connection", "data-state": kind, title: detail, role: "status", aria_live: "polite",
                span { class: "status_dot", aria_hidden: "true" }
                span { "{label}" }
            }
            button { hidden:true, "data-refresh-health":"true", class: "status_refresh", title: "Check database connection", aria_label: "Check database connection", disabled: checking,
                onclick: move |_| refresh += 1, "↻" }
            span { class: "status_activity", title: activity.clone(), "{activity}" }
            crate::stock_alerts::StockAlerts { form: form.clone() }
            span { class: "status_operator", title: operator.clone(), "{operator}" }
            time { class: "status_clock", "{clock_text}" }
        }
    }
}
