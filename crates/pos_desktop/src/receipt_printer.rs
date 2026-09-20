use crate::{receipt_layout, setting_value, ReceiptData};
use std::collections::HashMap;


pub async fn print(receipt: ReceiptData, settings: HashMap<String, String>) -> anyhow::Result<()> {
    print_rows(receipt.invoice_no.clone(), receipt_layout::sale(&receipt, &settings), settings).await
}

pub async fn print_detail(detail: crate::ReceiptDetail, settings: HashMap<String,String>) -> anyhow::Result<()> {
    let invoice=detail.summary.invoice_no.clone().unwrap_or_else(||format!("Sale #{}",detail.summary.id));
    print_rows(invoice, receipt_layout::detail(&detail,&settings), settings).await
}

async fn print_rows(invoice: String, rows: Vec<receipt_layout::Row>, settings: HashMap<String,String>) -> anyhow::Result<()> {
    let printer = setting_value(&settings, "receipt_printer_name", "");
    anyhow::ensure!(!printer.trim().is_empty(), "Select a receipt printer in Settings > Printer first");
    static PRINT_SLOT: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> = std::sync::OnceLock::new();
    let permit=PRINT_SLOT.get_or_init(||std::sync::Arc::new(tokio::sync::Semaphore::new(1))).clone()
        .try_acquire_owned().map_err(|_|anyhow::anyhow!("A receipt is already being sent to the printer. Please wait."))?;
    let payload = serde_json::to_vec(
        &serde_json::json!({"printer":printer,"invoice":invoice,"rows":rows,"paper_mm":receipt_layout::paper_mm(&settings)}),
    )?;
    tokio::task::spawn_blocking(move || {let _permit=permit; send(payload)}).await?
}

#[cfg(windows)]
fn send(payload: Vec<u8>) -> anyhow::Result<()> {
    use std::{
        io::Write,
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            concat!(
                "[Console]::InputEncoding = [System.Text.Encoding]::UTF8;\n",
                include_str!("../assets/print-receipt.ps1")
            ),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let write_result = child.stdin.take().unwrap().write_all(&payload);
    let output = child.wait_with_output()?;
    write_result?;
    anyhow::ensure!(
        output.status.success(),
        "Printer did not confirm the job: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[cfg(not(windows))]
fn send(_: Vec<u8>) -> anyhow::Result<()> {
    anyhow::bail!("Automatic printing requires Windows")
}
