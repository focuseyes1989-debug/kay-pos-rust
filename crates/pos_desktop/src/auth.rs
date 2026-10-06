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
    let remembered = use_hook(|| load_remembered_username().unwrap_or_default());
    let mut remember = use_signal(|| !remembered.is_empty());
    let mut username = use_signal(|| remembered.clone());
    let mut password = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut configure = use_signal(|| false);
    let mut appearance_revision = use_signal(|| 0u64);
    let initial_appearance = use_hook(|| local_appearance::load().unwrap_or_default());
    let appearance = use_resource(move || {
        let _ = appearance_revision();
        async move {
            let request = async {
                let pool = connect(&load_saved_db_form().database_config()?).await?;
                let rows = pos_core::db::list_settings(&pool).await?;
                Ok::<HashMap<String, String>, anyhow::Error>(
                    rows.into_iter()
                        .filter(|row| local_appearance::is_key(&row.key))
                        .map(|row| (row.key, row.value.unwrap_or_default()))
                        .collect(),
                )
            };
            match tokio::time::timeout(Duration::from_secs(4), request).await {
                Ok(Ok(settings)) => local_appearance::resolve(settings)
                    .unwrap_or_else(|_| local_appearance::load().unwrap_or_default()),
                _ => local_appearance::load().unwrap_or_default(),
            }
        }
    });
    let colors = appearance.read().as_ref().cloned().unwrap_or(initial_appearance);
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
                let keep_username = remember();
                let secret = password();
                password.set(String::new());
                spawn(async move {
                    let result = async {
                        let pool = connect(&load_saved_db_form().database_config()?).await?;
                        pos_core::auth::login(&pool, &name, &secret).await
                    }.await;
                    match result {
                        Ok(actor) => {
                            if let Err(e)=save_remembered_username(if keep_username {Some(actor.username())}else{None}) {
                                error.set(format!("Cannot save Remember me: {e}"));
                            } else {session.set(Some(actor));}
                        },
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
                label {class:"login_remember",
                    input {r#type:"checkbox",checked:remember(),disabled:busy(),onchange:move |e|{
                        let enabled=e.checked();remember.set(enabled);
                        if !enabled {if let Err(e)=save_remembered_username(None){error.set(format!("Cannot clear Remember me: {e}"));}}
                    }}
                    span {"Remember me"}
                }
                if !error().is_empty() { p { role: "alert", "{error}" } }
                button { class: "primary", r#type: "submit", disabled: busy(), if busy() { "Signing in..." } else { crate::icons::ActionLabel { label:"Sign in" } } }
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
        WorkspaceView::Sales | WorkspaceView::Receipts | WorkspaceView::ServiceOrders | WorkspaceView::Ai => Permission::Sell,
        WorkspaceView::Settings | WorkspaceView::ErpOperations => Permission::Admin,
        _ => Permission::Manage,
    })
}

fn remembered_username_path()->anyhow::Result<PathBuf> {
    Ok(PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or_else(||anyhow::anyhow!("LOCALAPPDATA unavailable"))?).join("KAY POS Rust").join("remembered-username.json"))
}
fn load_remembered_username()->anyhow::Result<String> {
    let path=remembered_username_path()?;
    if !path.exists(){return Ok(String::new());}
    Ok(serde_json::from_slice::<String>(&fs::read(path)?)?)
}
fn save_remembered_username(username:Option<&str>)->anyhow::Result<()> {
    save_remembered_username_at(&remembered_username_path()?,username)
}
fn save_remembered_username_at(path:&std::path::Path,username:Option<&str>)->anyhow::Result<()> {
    use std::io::Write;
    if let Some(username)=username {
        let parent=path.parent().ok_or_else(||anyhow::anyhow!("Invalid remember path"))?;
        fs::create_dir_all(parent)?;
        let mut file=tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(&serde_json::to_vec(username)?)?;
        file.as_file().sync_all()?;
        file.persist(path).map_err(|e|e.error)?;
    }else{match fs::remove_file(path){Ok(())=>{},Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(e.into())}}
    Ok(())
}
#[cfg(test)]
mod remember_tests {
    #[test]
    fn only_username_is_stored_and_uncheck_removes_it() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("remember.json");
        super::save_remembered_username_at(&path,Some("မောင်မောင်")).unwrap();
        let bytes=std::fs::read(&path).unwrap();assert_eq!(serde_json::from_slice::<String>(&bytes).unwrap(),"မောင်မောင်");
        super::save_remembered_username_at(&path,Some("cashier")).unwrap();
        assert_eq!(serde_json::from_slice::<String>(&std::fs::read(&path).unwrap()).unwrap(),"cashier");
        super::save_remembered_username_at(&path,None).unwrap();assert!(!path.exists());
        super::save_remembered_username_at(&path,None).unwrap();
    }
}
