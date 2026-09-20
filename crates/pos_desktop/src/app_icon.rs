pub fn icon() -> dioxus_desktop::tao::window::Icon {
    let image = image::load_from_memory_with_format(
        include_bytes!("../assets/kay-simple.ico"),
        image::ImageFormat::Ico,
    )
    .expect("Bundled KAY POS icon must decode")
    .into_rgba8();
    let (width, height) = image.dimensions();
    dioxus_desktop::tao::window::Icon::from_rgba(image.into_raw(), width, height)
        .expect("Bundled KAY POS icon must have valid RGBA dimensions")
}

#[test]
fn bundled_icon_decodes() {
    let _ = icon();
}
