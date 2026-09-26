use crate::DbForm;
use pos_core::purchases::Command;
use serde::{Deserialize, Serialize};
use std::io::Write;

#[derive(Clone, Serialize, Deserialize)]
pub struct Pending {
    pub username: String,
    pub command: Command,
}
fn path(form: &DbForm) -> anyhow::Result<std::path::PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow::anyhow!("Local app data unavailable"))?;
    let identity = format!(
        "{}:{}:{}",
        form.host.trim().to_lowercase(),
        form.port.trim(),
        form.database.trim()
    );
    Ok(std::path::PathBuf::from(root)
        .join("KAY POS Rust")
        .join("purchase-pending")
        .join(format!(
            "{}.json",
            pos_core::auth::fingerprint(identity.as_bytes())
        )))
}
pub fn load(form: &DbForm) -> anyhow::Result<Option<Pending>> {
    match std::fs::read(path(form)?) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub fn save(form: &DbForm, value: &Pending) -> anyhow::Result<()> {
    if let Some(old) = load(form)? {
        anyhow::ensure!(
            old.username == value.username && old.command == value.command,
            "Resolve the pending purchase request first"
        );
        return Ok(());
    }
    let path = path(form)?;
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&serde_json::to_vec(value)?)?;
    temp.as_file().sync_all()?;
    temp.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn clear(form: &DbForm, value: &Pending) -> anyhow::Result<()> {
    let old = load(form)?.ok_or_else(|| anyhow::anyhow!("Pending request missing"))?;
    anyhow::ensure!(
        old.username == value.username && old.command == value.command,
        "Pending purchase request changed"
    );
    std::fs::remove_file(path(form)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn purchase_journal_keeps_identity_and_exact_payload() {
        let form = DbForm {
            database: pos_core::auth::new_request_id(),
            ..Default::default()
        };
        let pending = Pending {
            username: "test".into(),
            command: Command {
                request_id: pos_core::auth::new_request_id(),
                action: pos_core::purchases::Action::Cancel {
                    id: 1,
                    revision: 1,
                    reason: "test".into(),
                },
            },
        };
        save(&form, &pending).unwrap();
        save(&form, &pending).unwrap();
        assert_eq!(load(&form).unwrap().unwrap().command, pending.command);
        let mut changed = pending.clone();
        changed.command.request_id = pos_core::auth::new_request_id();
        assert!(save(&form, &changed).is_err());
        assert!(clear(&form, &changed).is_err());
        clear(&form, &pending).unwrap();
        assert!(load(&form).unwrap().is_none());
    }
}
