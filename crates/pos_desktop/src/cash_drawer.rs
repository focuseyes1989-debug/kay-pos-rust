pub async fn open(printer: String) -> anyhow::Result<()> {
    anyhow::ensure!(
        !printer.trim().is_empty(),
        "Select a Windows receipt printer in Settings > Printer first"
    );
    anyhow::ensure!(!printer.contains('\0'), "Invalid printer name");
    tokio::task::spawn_blocking(move || send(&printer)).await?
}

#[cfg(not(windows))]
fn send(_: &str) -> anyhow::Result<()> {
    anyhow::bail!("Cash drawer requires Windows")
}

#[cfg(windows)]
fn send(printer: &str) -> anyhow::Result<()> {
    use std::{ffi::c_void, ptr};
    type Handle = *mut c_void;
    #[repr(C)]
    struct Document {
        name: *const u16,
        output: *const u16,
        datatype: *const u16,
    }
    #[link(name = "winspool")]
    extern "system" {
        fn OpenPrinterW(name: *const u16, handle: *mut Handle, defaults: *const c_void) -> i32;
        fn StartDocPrinterW(handle: Handle, level: u32, doc: *const Document) -> u32;
        fn StartPagePrinter(handle: Handle) -> i32;
        fn WritePrinter(handle: Handle, data: *const u8, count: u32, written: *mut u32) -> i32;
        fn EndPagePrinter(handle: Handle) -> i32;
        fn EndDocPrinter(handle: Handle) -> i32;
        fn ClosePrinter(handle: Handle) -> i32;
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn check(ok: bool) -> anyhow::Result<()> {
        anyhow::ensure!(
            ok,
            "Windows did not confirm the drawer command: {}. Check the printer before retrying.",
            std::io::Error::last_os_error()
        );
        Ok(())
    }
    let name = wide(printer);
    let title = wide("KAY POS - Cash Drawer");
    let raw = wide("RAW");
    let mut handle = ptr::null_mut();
    // The buffers remain alive throughout the synchronous Windows spooler calls.
    unsafe {
        check(OpenPrinterW(name.as_ptr(), &mut handle, ptr::null()) != 0)?;
        let result = (|| {
            let doc = Document {
                name: title.as_ptr(),
                output: ptr::null(),
                datatype: raw.as_ptr(),
            };
            check(StartDocPrinterW(handle, 1, &doc) != 0)?;
            let page_result = (|| {
                check(StartPagePrinter(handle) != 0)?;
                let command = [0x1b, 0x70, 0x00, 0x19, 0xfa];
                let mut written = 0;
                let write_result = check(
                    WritePrinter(handle, command.as_ptr(), 5, &mut written) != 0 && written == 5,
                );
                let end_result = check(EndPagePrinter(handle) != 0);
                write_result.and(end_result)
            })();
            let end_result = check(EndDocPrinter(handle) != 0);
            page_result.and(end_result)
        })();
        ClosePrinter(handle);
        result
    }
}
