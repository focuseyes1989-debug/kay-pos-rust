use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
};

const KEYS: &[&str] = &[
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
    let local = read_at(path)?.context("Local appearance settings disappeared")?;
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
    fn shared(theme: &str) -> Settings {
        [
            ("theme".into(), theme.into()),
            ("shop_name".into(), "Shop".into()),
        ]
        .into()
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
