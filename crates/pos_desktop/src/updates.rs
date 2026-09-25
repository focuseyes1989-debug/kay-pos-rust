use anyhow::{Context, Result};
use dioxus::prelude::*;
use std::{path::Path, time::Duration};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
enum InstallProgress {
    Preparing,
    Downloading(u64, Option<u64>),
    Verifying,
    Installing,
}
impl InstallProgress {
    fn label(&self) -> &'static str {
        match self {
            Self::Preparing => "Preparing update...",
            Self::Downloading(..) => "Downloading...",
            Self::Verifying => "Verifying update...",
            Self::Installing => "Installing...",
        }
    }
    fn percent(&self) -> Option<f64> {
        match self {
            Self::Downloading(bytes, Some(total)) if *total > 0 =>
                Some((*bytes as f64 / *total as f64 * 100.0).min(100.0)),
            _ => None,
        }
    }
    fn detail(&self) -> String {
        match self {
            Self::Downloading(bytes, total) => {
                let downloaded = *bytes as f64 / 1_048_576.0;
                match total.filter(|total| *total > 0) {
                    Some(total) => format!("{:.0}%  |  {downloaded:.1} / {:.1} MB", self.percent().unwrap_or(0.0), total as f64 / 1_048_576.0),
                    None => format!("{downloaded:.1} MB downloaded"),
                }
            }
            _ => self.label().into(),
        }
    }
}
type ProgressState = Arc<Mutex<InstallProgress>>;
fn report(progress: &ProgressState, value: InstallProgress) {
    *progress.lock().unwrap_or_else(|e| e.into_inner()) = value;
}

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

fn install(version: &str, progress: ProgressState) -> Result<std::path::PathBuf> {
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
    let download_progress = progress.clone();
    let archive_progress = progress.clone();
    let mut config = builder();
    config
        .release_tag(format!("v{version}"))
        .asset_matcher(move |assets| {
            assets
                .iter()
                .find(|asset| asset.name() == expected)
                .cloned()
        })
        .progress_callback(move |bytes, total| {
            report(&download_progress, if total.is_some_and(|size| size > 0 && bytes >= size) {
                InstallProgress::Verifying
            } else { InstallProgress::Downloading(bytes, total) });
        })
        .verify_archive(move |_| {
            report(&archive_progress, InstallProgress::Verifying);
            Ok(())
        })
        .verify_binary(move |path| {
            report(&progress, InstallProgress::Verifying);
            verify_binary(path, &version)
                .map_err(|e| self_update::Error::verification_rejected(format!("{e:#}")))?;
            report(&progress, InstallProgress::Installing);
            Ok(())
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
    let mut progress = use_signal(|| None::<InstallProgress>);
    let mut checking = use_signal(|| false);
    let mut restarting = use_signal(|| false);
    let button_label = if restarting() { "Restarting..." }
        else if installed().is_some() { "Restart KAY POS" }
        else if let Some(value) = progress() { value.label() }
        else if checking() { "Checking..." }
        else if allow_install && version().is_some() { "Download & Install" }
        else { "Check for Updates" };
    rsx! {
        section { class: "app_updates",
            strong { "KAY POS {VERSION}" }
            div { class: "app_update_actions",
                button { class: "primary update_check", r#type: "button",
                    disabled: restarting() || (busy() && installed().is_none()),
                    "aria-busy": checking() || progress().is_some() || restarting(),
                    onclick: move |_| {
                    if restarting() { return; }
                    if let Some(path) = installed() {
                        restarting.set(true);
                        match std::process::Command::new(&path).spawn() {
                            Ok(_) => dioxus_desktop::window().close(),
                            Err(e) => { restarting.set(false); status.set(format!("Restart failed: {e}. Close and reopen KAY POS.")); }
                        }
                        return;
                    }
                    if busy() { return; }
                    if let Some(next) = version().filter(|_| allow_install) {
                        busy.set(true); status.set(String::new()); progress.set(Some(InstallProgress::Preparing));
                        spawn(async move {
                            let shared = Arc::new(Mutex::new(InstallProgress::Preparing));
                            let worker_progress = shared.clone();
                            let task = tokio::task::spawn_blocking(move || install(&next, worker_progress));
                            // Coalesce download callbacks so fast transfers cannot flood the UI.
                            while !task.is_finished() {
                                let current = shared.lock().unwrap_or_else(|e| e.into_inner()).clone();
                                if progress.peek().as_ref() != Some(&current) { progress.set(Some(current)); }
                                tokio::time::sleep(Duration::from_millis(100)).await;
                            }
                            progress.set(None);
                            match task.await {
                                Ok(Ok(path)) => { installed.set(Some(path)); status.set("Update installed. Restart to continue.".into()); }
                                Ok(Err(e)) => { status.set(format!("Update failed: {e:#}")); busy.set(false); }
                                Err(e) => { status.set(format!("Update failed: {e}")); busy.set(false); }
                            }
                        });
                        return;
                    }
                    busy.set(true); checking.set(true); version.set(None); status.set(String::new());
                    spawn(async move {
                        let result = tokio::task::spawn_blocking(check).await;
                        match result {
                            Ok(Ok(Some(next))) => { status.set(format!("Version {next} available")); version.set(Some(next)); }
                            Ok(Ok(None)) => status.set("No newer release is available.".into()),
                            Ok(Err(e)) => status.set(format!("Update check failed: {e:#}. You can continue using this version.")),
                            Err(e) => status.set(format!("Update check failed: {e}")),
                        }
                        checking.set(false); busy.set(false);
                    });
                }, crate::icons::ActionLabel {label:button_label} }
            }
            if let Some(value) = progress() {
                div { class: "app_update_progress",
                    progress { max: "100", value: value.percent().map(|v| v.to_string()), aria_label: value.label() }
                    small { "{value.detail()}" }
                }
            }
            if !allow_install && version().is_some() { p { "Sign out to install this update." } }
            if !status().is_empty() { p { role: "status", "{status}" } }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_is_byte_based_and_unknown_sizes_are_indeterminate() {
        let progress = InstallProgress::Downloading(1_048_576, Some(4_194_304));
        assert_eq!(progress.percent(), Some(25.0));
        assert!(progress.detail().contains("1.0 / 4.0 MB"));
        assert_eq!(InstallProgress::Downloading(20, Some(10)).percent(), Some(100.0));
        assert_eq!(InstallProgress::Downloading(20, None).percent(), None);
        assert_eq!(InstallProgress::Downloading(20, Some(0)).percent(), None);
        for phase in [InstallProgress::Preparing, InstallProgress::Verifying, InstallProgress::Installing] {
            assert_eq!(phase.percent(), None);
        }
    }
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
