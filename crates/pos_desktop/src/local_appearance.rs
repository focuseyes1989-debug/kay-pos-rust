use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
};

const KEYS: &[&str] = &[
    "customer_display_editor_v1",
    "ui_theme_editor_v1",
    "ui_theme_preset_library_v1",
    "language",
    "language_ui_version",
    "sidebar_hidden_pages",
    "barcode_label_width",
    "barcode_label_height",
    "barcode_label_dpi",
    "theme",
    "follow_system_theme",
    "window_resolution",
    "ui_header_color",
    "ui_status_color",
    "ui_button_color",
    "ui_hover_color",
    "ui_focus_color",
    "ui_category_color",
    "ui_product_hover_color",
    "ui_sidebar_color",
    "ui_icon_hover_color",
];
type Settings = HashMap<String, String>;

pub fn is_key(key: &str) -> bool {
    KEYS.contains(&key)
}

fn path() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is unavailable")?)
            .join("KAY POS Rust")
            .join("appearance.json"),
    )
}

fn read_at(path: &Path) -> Result<Option<Settings>> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut values: Settings =
                serde_json::from_slice(&bytes).context("Local appearance settings are damaged")?;
            values.retain(|key, _| KEYS.contains(&key.as_str()));
            Ok(Some(values))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("Cannot read local appearance settings"),
    }
}

fn write_at(path: &Path, values: &Settings, create_only: bool) -> Result<()> {
    let parent = path.parent().context("Invalid appearance path")?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec_pretty(values)?)?;
    file.as_file().sync_all()?;
    if create_only {
        match file.persist_noclobber(path) {
            Ok(_) => (),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.error.into()),
        }
    } else {
        file.persist(path).map_err(|e| e.error)?;
    }
    Ok(())
}

fn resolve_at(path: &Path, mut shared: Settings) -> Result<Settings> {
    if read_at(path)?.is_none() {
        // Import the existing appearance once, then ignore all server changes.
        let initial = shared
            .iter()
            .filter(|(k, _)| KEYS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        write_at(path, &initial, true)?;
    }
    let mut local = read_at(path)?.context("Local appearance settings disappeared")?;
    if !local.contains_key("language_ui_version") {
        // Preserve the current language once when upgrading an existing client.
        let language = if shared.get("language_ui_version").map(String::as_str) == Some("1")
            && shared.get("language").map(String::as_str) == Some("en") { "en" } else { "my" };
        local.insert("language".into(), language.into());
        local.insert("language_ui_version".into(), "1".into());
        write_at(path, &local, false)?;
    }
    shared.retain(|key, _| !KEYS.contains(&key.as_str()));
    shared.extend(local);
    Ok(shared)
}

pub fn resolve(shared: Settings) -> Result<Settings> {
    resolve_at(&path()?, shared)
}

pub fn load() -> Result<Settings> {
    Ok(read_at(&path()?)?.unwrap_or_default())
}

fn save_at(path: &Path, changes: &[(String, String)]) -> Result<()> {
    anyhow::ensure!(
        changes.iter().all(|(key, _)| KEYS.contains(&key.as_str())),
        "Only appearance settings can be saved locally"
    );
    let mut values = read_at(path)?.unwrap_or_default();
    values.extend(changes.iter().cloned());
    write_at(path, &values, false)
}

pub fn save(changes: &[(String, String)]) -> Result<()> {
    save_at(&path()?, changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_editor_and_presets_persist_without_affecting_other_clients() {
        let temp=tempfile::tempdir().unwrap();let a=temp.path().join("a.json");let b=temp.path().join("b.json");
        resolve_at(&a,shared("light")).unwrap();resolve_at(&b,shared("dark")).unwrap();
        let config=r##"{"light":{"header":{"color":"#123456","end":"#abcdef","kind":"linear","angle":90}},"dark":{},"presets":[{"name":"Ocean","light":{},"dark":{}}]}"##;
        assert!(crate::theme_editor::validate(config).is_ok());
        save_at(&a,&[(crate::theme_editor::KEY.into(),config.into())]).unwrap();
        assert_eq!(resolve_at(&a,shared("dark")).unwrap()[crate::theme_editor::KEY],config);
        assert!(!resolve_at(&b,shared("light")).unwrap().contains_key(crate::theme_editor::KEY));
    }
    fn shared(theme: &str) -> Settings {
        [
            ("theme".into(), theme.into()),
            ("shop_name".into(), "Shop".into()),
        ]
        .into()
    }
    #[test]
    fn language_is_imported_once_and_then_independent_for_each_client() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a/appearance.json");
        let b = temp.path().join("b/appearance.json");
        // Existing local appearance files from before language became local.
        write_at(&a, &shared("light"), false).unwrap();
        let mut server = shared("light");
        server.insert("language".into(), "en".into());
        server.insert("language_ui_version".into(), "1".into());
        assert_eq!(resolve_at(&a, server.clone()).unwrap()["language"], "en");
        assert_eq!(resolve_at(&b, server.clone()).unwrap()["language"], "en");
        save_at(&a, &[("language".into(), "my".into()), ("language_ui_version".into(), "1".into())]).unwrap();
        assert_eq!(resolve_at(&a, server.clone()).unwrap()["language"], "my");
        server.insert("language".into(), "my".into());
        let reloaded_b = resolve_at(&b, server).unwrap();
        assert_eq!(reloaded_b["language"], "en");
        assert_eq!(crate::regional::Language::from_settings(&reloaded_b), crate::regional::Language::English);
        assert!(is_key("language"));
        assert!(is_key("language_ui_version"));
        assert!(!is_key("currency"));
    }
    #[test]
    fn menu_visibility_survives_reload_and_stays_local() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a/appearance.json");
        let b = temp.path().join("b/appearance.json");
        resolve_at(&a, shared("light")).unwrap();
        resolve_at(&b, shared("light")).unwrap();
        save_at(&a, &[("sidebar_hidden_pages".into(), r#"["Reports"]"#.into())]).unwrap();
        assert_eq!(resolve_at(&a, shared("dark")).unwrap()["sidebar_hidden_pages"], r#"["Reports"]"#);
        assert!(!resolve_at(&b, shared("dark")).unwrap().contains_key("sidebar_hidden_pages"));
    }
    #[test]
    fn clients_are_independent_and_shared_settings_still_refresh() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a/appearance.json");
        let b = temp.path().join("b/appearance.json");
        resolve_at(&a, shared("light")).unwrap();
        resolve_at(&b, shared("light")).unwrap();
        save_at(&a, &[("theme".into(), "dark".into())]).unwrap();
        assert_eq!(resolve_at(&a, shared("light")).unwrap()["theme"], "dark");
        let mut server = shared("dark");
        server.insert("shop_name".into(), "New Shop".into());
        server.insert("ui_sidebar_color".into(), "#ffffff".into());
        let client_b = resolve_at(&b, server).unwrap();
        assert_eq!(client_b["theme"], "light");
        assert_eq!(client_b["shop_name"], "New Shop");
        assert!(!client_b.contains_key("ui_sidebar_color"));
        assert!(!read_at(&a).unwrap().unwrap().contains_key("shop_name"));
    }
    #[test]
    fn customer_display_design_persists_only_on_this_client() {
        let temp=tempfile::tempdir().unwrap();
        let a=temp.path().join("a/appearance.json");
        let b=temp.path().join("b/appearance.json");
        resolve_at(&a,shared("light")).unwrap();
        resolve_at(&b,shared("light")).unwrap();
        let mut design=crate::customer_display_editor::Design::default();
        design.font_size=24;
        let key=crate::customer_display_editor::KEY;
        let value=serde_json::to_string(&design).unwrap();
        save_at(&a,&[(key.into(),value.clone())]).unwrap();
        assert_eq!(resolve_at(&a,shared("light")).unwrap()[key],value);
        assert!(!resolve_at(&b,shared("light")).unwrap().contains_key(key));
    }
    #[test]
    fn invalid_files_and_nonappearance_writes_are_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("appearance.json");
        assert!(save_at(&file, &[("shop_name".into(), "No".into())]).is_err());
        std::fs::write(&file, b"damaged").unwrap();
        assert!(resolve_at(&file, shared("dark")).is_err());
        assert!(save_at(&file, &[("theme".into(), "dark".into())]).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"damaged");
    }
}
