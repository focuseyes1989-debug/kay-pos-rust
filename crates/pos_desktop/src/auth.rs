use super::*;
use base64::Engine;
use pos_core::auth::{Permission, Session};

#[component]
pub fn Root() -> Element {
    let mut session = use_signal(|| None::<Session>);
    use_context_provider(|| session);
    use_future(move || async move {
        let mut listener = document::eval(include_str!("../assets/fullscreen.js"));
        while let Ok(action) = listener.recv::<String>().await {
            if action == "fullscreen" {
                let window = dioxus_desktop::window();
                window.set_fullscreen(window.fullscreen().is_none());
            }
        }
    });
    use_future(move || async move {
        loop {
            sleep(Duration::from_secs(30)).await;
            let current = session.peek().clone();
            if let Some(actor) = current {
                let result = async {
                    let pool = connect(&load_saved_db_form().database_config()?).await?;
                    let mut connection = pool.acquire().await?;
                    actor.authorize(&mut connection, Permission::Sell).await
                }
                .await;
                if let Err(error) = result {
                    // A network outage is not evidence of revocation; sale/refund still
                    // revalidate on the server before writing.
                    if !pos_core::auth::is_database_error(&error)
                        && session.peek().as_ref() == Some(&actor)
                    {
                        session.set(None);
                    }
                }
            }
        }
    });
    if session.read().is_some() {
        rsx! { App {} }
    } else {
        rsx! { Login {} }
    }
}

#[component]
fn Login() -> Element {
    let mut session = use_context::<Signal<Option<Session>>>();
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut configure = use_signal(|| false);
    let mut appearance_revision = use_signal(|| 0u64);
    let appearance = use_resource(move || {
        let _ = appearance_revision();
        async move {
            let request = async {
                let pool = connect(&load_saved_db_form().database_config()?).await?;
                let rows = pos_core::db::list_settings(&pool).await?;
                Ok::<HashMap<String, String>, anyhow::Error>(
                    rows.into_iter()
                        .filter(|row| {
                            row.key == "theme"
                                || row.key == "follow_system_theme"
                                || appearance_colors::FIELDS
                                    .iter()
                                    .any(|(key, _, _)| *key == row.key)
                        })
                        .map(|row| (row.key, row.value.unwrap_or_default()))
                        .collect(),
                )
            };
            match tokio::time::timeout(Duration::from_secs(4), request).await {
                Ok(Ok(settings)) => settings,
                _ => HashMap::new(),
            }
        }
    });
    let colors = appearance.read().as_ref().cloned().unwrap_or_default();
    let background = use_hook(|| {
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(
            include_bytes!("../assets/market.png")
        ))
    });
    rsx! {
        style { "{APP_CSS}" }
        main { class: "shell login_screen",
            style: appearance_colors::style(&colors),
            "data-theme": appearance_mode(&colors),
            img { class: "login_background", src: background, alt: "", aria_hidden: "true" }
            form { class: "login_form", onsubmit: move |event| {
                event.prevent_default();
                if busy() { return; }
                busy.set(true);
                error.set(String::new());
                let name = username();
                let secret = password();
                password.set(String::new());
                spawn(async move {
                    let result = async {
                        let pool = connect(&load_saved_db_form().database_config()?).await?;
                        pos_core::auth::login(&pool, &name, &secret).await
                    }.await;
                    match result {
                        Ok(actor) => session.set(Some(actor)),
                        Err(e) => {
                            error.set(format!("Sign in failed: {e:#}"));
                            sleep(Duration::from_secs(2)).await;
                        }
                    }
                    busy.set(false);
                });
            },
                div { class: "login_brand",
                    div { class: "brand_mark", aria_hidden: "true", "K" }
                    h1 { "KAY POS" }
                }
                label { "Username" input { autofocus: true, autocomplete: "username", disabled: busy(), value: username(), oninput: move |e| username.set(e.value()) } }
                label { "Password" input { r#type: "password", autocomplete: "current-password", disabled: busy(), value: password(), oninput: move |e| password.set(e.value()) } }
                if !error().is_empty() { p { role: "alert", "{error}" } }
                button { class: "primary", r#type: "submit", disabled: busy(), if busy() { "Signing in..." } else { "Sign in" } }
                button { class: "login_connection", r#type: "button", disabled: busy(), onclick: move |_| configure.set(true), "Database connection" }
                updates::UpdatePanel { allow_install: true, busy }
            }
            if configure() {
                DbSettingsDialog { form: load_saved_db_form(), on_cancel: move |_| configure.set(false), on_save: move |form| {
                    match save_db_form(&form) {
                        Ok(()) => { configure.set(false); appearance_revision += 1; }
                        Err(e) => error.set(format!("{e:#}")),
                    }
                } }
            }
        }
    }
}

pub fn can_open(actor: &Session, view: WorkspaceView) -> bool {
    actor.allows(match view {
        WorkspaceView::Sales | WorkspaceView::Receipts | WorkspaceView::ServiceOrders => Permission::Sell,
        WorkspaceView::Settings => Permission::Admin,
        _ => Permission::Manage,
    })
}
