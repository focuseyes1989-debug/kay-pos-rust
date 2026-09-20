use anyhow::{Context, Result};
use dioxus::prelude::*;
use std::{path::Path, time::Duration};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const OWNER: &str = "focuseyes1989-debug";
const REPO: &str = "kay-pos-updates";
const PUBLIC_KEY: [u8; 32] = *include_bytes!("../assets/update-public.key");

fn compatible_newer(current: &str, next: &str) -> Result<bool> {
    let current = semver::Version::parse(current)?;
    let next = semver::Version::parse(next)?;
    anyhow::ensure!(
        next.pre.is_empty() && next.build.is_empty(),
        "Only stable releases are supported"
    );
    anyhow::ensure!((current.major, current.minor) == (next.major, next.minor),
        "This release needs a managed upgrade. Contact the administrator before updating the database.");
    Ok(next > current)
}

fn asset_name(version: &str) -> String {
    format!("kay-pos-{version}-x86_64-pc-windows-msvc.zip")
}

fn builder() -> self_update::backends::github::UpdateBuilder {
    let mut builder = self_update::backends::github::Update::configure();
    builder
        .repo_owner(OWNER)
        .repo_name(REPO)
        .bin_name("pos_desktop.exe")
        .bin_path_in_archive("pos_desktop.exe")
        .target("x86_64-pc-windows-msvc")
        .current_version(VERSION)
        .verifying_keys([PUBLIC_KEY])
        .timeout(Duration::from_secs(120))
        .no_confirm(true)
        .show_output(false)
        .show_download_progress(false);
    builder
}

fn check() -> Result<Option<String>> {
    // GitHub's latest endpoint excludes drafts and releases marked prerelease,
    // even when their tags happen to look like stable semver.
    let releases = match builder()
        .timeout(Duration::from_secs(15))
        .build()?
        .get_latest_release()
    {
        Ok(releases) => releases,
        Err(e) if e.http_status() == Some(404) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let Some(release) = releases.all().first() else {
        return Ok(None);
    };
    if !compatible_newer(VERSION, release.version())? {
        return Ok(None);
    }
    let expected = asset_name(release.version());
    anyhow::ensure!(
        release.assets().iter().any(|a| a.name() == expected),
        "The release has no compatible Windows package yet"
    );
    Ok(Some(release.version().to_string()))
}

fn verify_binary(path: &Path, version: &str) -> Result<()> {
    let mut command = std::process::Command::new(path);
    command.arg("--update-self-check").arg(version);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .context("Could not start the downloaded executable")?;
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            anyhow::ensure!(
                status.success(),
                "Downloaded executable version check failed"
            );
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Downloaded executable did not respond");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn install(version: &str) -> Result<std::path::PathBuf> {
    anyhow::ensure!(
        !cfg!(debug_assertions),
        "Install updates from a release build only"
    );
    anyhow::ensure!(
        compatible_newer(VERSION, version)?,
        "No newer version selected"
    );
    let form = crate::load_saved_db_form();
    let _instance = crate::pending_checkout::acquire_instance(&form)?;
    anyhow::ensure!(
        crate::pending_checkout::load(&form)?.is_none(),
        "Sign in and resolve the pending checkout before updating"
    );
    let executable = std::env::current_exe()?;
    let backup = executable.with_extension("previous.exe");
    std::fs::copy(&executable, &backup)
        .context("Cannot create an executable backup. Use a writable installation folder.")?;
    let expected = asset_name(version);
    let version = version.to_string();
    let mut config = builder();
    config
        .release_tag(format!("v{version}"))
        .asset_matcher(move |assets| {
            assets
                .iter()
                .find(|asset| asset.name() == expected)
                .cloned()
        })
        .verify_binary(move |path| {
            verify_binary(path, &version)
                .map_err(|e| self_update::Error::verification_rejected(format!("{e:#}")))
        });
    config.build()?.update()?;
    Ok(executable)
}

#[component]
pub fn SettingsUpdates() -> Element {
    let busy = use_signal(|| false);
    rsx! { UpdatePanel { allow_install: false, busy } }
}

#[component]
pub fn UpdatePanel(allow_install: bool, mut busy: Signal<bool>) -> Element {
    let mut version = use_signal(|| None::<String>);
    let mut status = use_signal(String::new);
    let mut installed = use_signal(|| None::<std::path::PathBuf>);
    rsx! {
        section { class: "app_updates",
            strong { "KAY POS {VERSION}" }
            div { class: "app_update_actions",
                button { r#type: "button", disabled: busy(), onclick: move |_| {
                    if busy() { return; }
                    busy.set(true); version.set(None); status.set("Checking for updates...".into());
                    spawn(async move {
                        let result = tokio::task::spawn_blocking(check).await;
                        match result {
                            Ok(Ok(Some(next))) => { status.set(format!("Version {next} available")); version.set(Some(next)); }
                            Ok(Ok(None)) => status.set("No newer release is available.".into()),
                            Ok(Err(e)) => status.set(format!("Update check failed: {e:#}. You can continue using this version.")),
                            Err(e) => status.set(format!("Update check failed: {e}")),
                        }
                        busy.set(false);
                    });
                }, "Check for Updates" }
                if let Some(next) = version() {
                    if allow_install {
                        button { r#type: "button", disabled: busy(), onclick: move |_| {
                            if busy() { return; }
                            let next=next.clone(); busy.set(true); status.set("Downloading and verifying update...".into());
                            spawn(async move {
                                match tokio::task::spawn_blocking(move || install(&next)).await {
                                    Ok(Ok(path)) => { installed.set(Some(path)); status.set("Update installed. Restart to continue.".into()); }
                                    Ok(Err(e)) => { status.set(format!("Update failed: {e:#}")); busy.set(false); }
                                    Err(e) => { status.set(format!("Update failed: {e}")); busy.set(false); }
                                }
                            });
                        }, "Download & Install" }
                    } else { span { "Sign out to install this update." } }
                }
                if let Some(path) = installed() {
                    button { r#type: "button", onclick: move |_| {
                        let result = std::process::Command::new(&path).spawn();
                        match result {
                            Ok(_) => dioxus_desktop::window().close(),
                            Err(e) => status.set(format!("Restart failed: {e}. Close and reopen KAY POS.")),
                        }
                    }, "Restart KAY POS" }
                }
            }
            if !status().is_empty() { p { role: "status", "{status}" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires a signed release package; uses only a local mock server and temporary executable"]
    fn staged_install_preserves_files_and_rejects_bad_signature() {
        use std::io::{Read, Write};
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
        let zip = std::path::PathBuf::from(std::env::var_os("KAY_UPDATE_TEST_PACKAGE").unwrap());
        let bytes = std::fs::read(&zip).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let release = serde_json::json!({"tag_name":format!("v{VERSION}"), "name":"Test", "created_at":"2026-09-20T00:00:00Z", "assets":[{"name":asset_name(VERSION), "url":format!("{base}/asset") }]}).to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let server_stop = stop.clone();
        let server = std::thread::spawn(move || {
            while !server_stop.load(Ordering::Relaxed) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(10)); continue;
                };
                socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut request = [0;8192];
                let count = socket.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..count]);
                let body = if request.starts_with("GET /asset ") { bytes.as_slice() } else { release.as_bytes() };
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                socket.write_all(body).unwrap();
            }
        });
        let temp = tempfile::tempdir().unwrap();
        let installed = temp.path().join("pos_desktop.exe");
        let config = temp.path().join("kay-pos-db.json");
        std::fs::write(&installed, b"old executable").unwrap();
        std::fs::write(&config, b"untouched configuration").unwrap();
        let mut update = builder();
        update.api_base_url(&base).current_version("0.1.9").release_tag(format!("v{VERSION}"))
            .bin_install_path(&installed).timeout(Duration::from_secs(10))
            .asset_matcher(|assets| assets.iter().find(|a| a.name() == asset_name(VERSION)).cloned())
            .verifying_keys([[1;32]]);
        let rejected = update.build().unwrap().update().is_err();
        let unchanged = std::fs::read(&installed).unwrap() == b"old executable";
        update.verifying_keys([PUBLIC_KEY]).verify_binary(|p| verify_binary(p, VERSION)
            .map_err(|e| self_update::Error::verification_rejected(e.to_string())));
        let result = update.build().unwrap().update();
        stop.store(true, Ordering::Relaxed);
        server.join().unwrap();
        assert!(rejected && unchanged);
        result.unwrap();
        assert_eq!(std::fs::read(&config).unwrap(), b"untouched configuration");
        verify_binary(&installed, VERSION).unwrap();
    }
    #[test]
    #[ignore = "Makes a read-only GitHub API request"]
    fn public_release_check() {
        check().unwrap();
    }

    #[test]
    #[ignore = "Requires KAY_UPDATE_TEST_PACKAGE pointing to a signed release ZIP"]
    fn signed_package_and_tamper() {
        let package = std::path::PathBuf::from(std::env::var_os("KAY_UPDATE_TEST_PACKAGE").unwrap());
        self_update::verify_signature(&package, &[PUBLIC_KEY]).unwrap();
        assert!(self_update::verify_signature(&package, &[[1; 32]]).is_err());
        let folder = std::env::temp_dir().join(format!("kay-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let copy = folder.join(package.file_name().unwrap());
        let mut bytes = std::fs::read(&package).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 1;
        std::fs::write(&copy, bytes).unwrap();
        let rejected = self_update::verify_signature(&copy, &[PUBLIC_KEY]).is_err();
        std::fs::remove_file(copy).unwrap();
        std::fs::remove_dir(folder).unwrap();
        assert!(rejected);
    }
    #[test]
    fn update_version_policy() {
        assert!(compatible_newer("0.2.0", "0.2.1").unwrap());
        assert!(!compatible_newer("0.2.2", "0.2.1").unwrap());
        assert!(!compatible_newer("0.2.0", "0.2.0").unwrap());
        assert!(compatible_newer("0.2.0", "0.3.0").is_err());
        assert!(compatible_newer("0.2.0", "0.2.1-beta.1").is_err());
        assert!(compatible_newer("0.2.0", "../evil").is_err());
        assert_eq!(
            asset_name("0.2.1"),
            "kay-pos-0.2.1-x86_64-pc-windows-msvc.zip"
        );
        assert_ne!(PUBLIC_KEY, [0; 32]);
    }
}
