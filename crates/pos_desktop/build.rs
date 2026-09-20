fn main() {
    let icon = "assets/kay-simple.ico";
    println!("cargo:rerun-if-changed={icon}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winres::WindowsResource::new()
            .set_icon(icon)
            .set("ProductName", "KAY POS")
            .set("FileDescription", "KAY POS")
            .compile()
            .expect("Failed to embed the KAY POS Windows icon");
    }
}
