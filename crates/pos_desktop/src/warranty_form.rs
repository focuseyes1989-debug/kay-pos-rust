use crate::DbForm;
use pos_core::auth::Session;
use std::{collections::HashMap, path::PathBuf};

pub async fn output(
    form: DbForm,
    actor: Session,
    unit: String,
    pdf: bool,
) -> anyhow::Result<String> {
    let pool = pos_core::connect(&form.database_config()?).await?;
    let record = pos_core::warranty::certificate(&pool, &actor, &unit).await?;
    let settings: HashMap<String, String> = pos_core::db::list_settings(&pool)
        .await?
        .into_iter()
        .map(|s| (s.key, s.value.unwrap_or_default()))
        .collect();
    let destination = if pdf {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_file_name("Warranty.pdf")
            .add_filter("PDF", &["pdf"])
            .save_file()
            .await
        else {
            return Ok("PDF export cancelled.".into());
        };
        Some(file.path().to_path_buf())
    } else {
        None
    };
    let payload = serde_json::json!({"record":record,"shop":crate::setting_value(&settings,"shop_name","KAY POS"),"phone":crate::setting_value(&settings,"shop_phone",""),"address":crate::setting_value(&settings,"shop_address",""),"logo":crate::receipt_layout::logo(&settings),"issued":chrono::Local::now().format("%Y-%m-%d").to_string(),"mode":if pdf {"pdf"}else{"print"}});
    let path = destination.clone();
    let printed = tokio::task::spawn_blocking(move || send(payload, path)).await??;
    Ok(if !printed {
        "Print cancelled.".into()
    } else if let Some(path) = destination {
        format!("Warranty PDF saved: {}", path.display())
    } else {
        "A4 warranty form sent to printer.".into()
    })
}

#[cfg(windows)]
fn send(mut payload: serde_json::Value, destination: Option<PathBuf>) -> anyhow::Result<bool> {
    use std::{
        io::Write,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    // A private temporary PDF protects an existing destination when rendering fails.
    let temporary = if let Some(path) = &destination {
        Some(
            tempfile::Builder::new()
                .prefix(".kay-warranty-")
                .suffix(".pdf")
                .tempfile_in(path.parent().unwrap_or(std::path::Path::new(".")))?
                .into_temp_path(),
        )
    } else {
        None
    };
    if let Some(file) = &temporary {
        payload["path"] = file.to_string_lossy().to_string().into();
    }
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-STA",
            "-Command",
            concat!(
                "[Console]::InputEncoding=[System.Text.Encoding]::UTF8;\n",
                include_str!("../assets/warranty-form.ps1")
            ),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&payload)?)?;
    let result = child.wait_with_output()?;
    anyhow::ensure!(
        result.status.success(),
        "Warranty form failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    if String::from_utf8_lossy(&result.stdout).contains("CANCELLED") {
        return Ok(false);
    }
    if let (Some(file), Some(path)) = (temporary, destination) {
        use std::io::Read;
        let mut signature = [0u8; 5];
        std::fs::File::open(&file)?.read_exact(&mut signature)?;
        anyhow::ensure!(
            &signature == b"%PDF-",
            "PDF printer did not produce a valid PDF"
        );
        file.persist(path).map_err(|e| e.error)?;
    }
    Ok(true)
}
#[cfg(not(windows))]
fn send(_: serde_json::Value, _: Option<PathBuf>) -> anyhow::Result<bool> {
    anyhow::bail!("A4 warranty output requires Windows")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires Windows Microsoft Print to PDF"]
    fn exports_real_a4_pdf_using_private_temporary_file() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("Warranty-ဦးအောင်သန်း.pdf");
        let payload = serde_json::json!({"record":{"id":"TEST-UNIT","serial":"SN-001","invoice":"INV001","customer":"ဦးအောင်သန်း","phone":"09 123456789","address":"Yangon","product":"Lithium Battery 51.2V 100Ah","starts":"2026-10-14","ends":"2028-10-14","sale_status":"completed"},"shop":"KAY SOLAR","phone":"09 777123456","address":"Yangon","issued":"2026-10-14","mode":"pdf"});
        assert!(send(payload.clone(), Some(path.clone()))?);
        assert!(std::fs::metadata(&path)?.len() > 1000);
        assert!(send(payload, Some(path.clone()))?);
        assert_eq!(&std::fs::read(path)?[..5], b"%PDF-");
        assert_eq!(std::fs::read_dir(dir.path())?.count(), 1);
        Ok(())
    }
}
