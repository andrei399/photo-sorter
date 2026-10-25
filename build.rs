// Windows only shows an .exe's icon in Explorer, the taskbar pin and installer shortcuts when it is embedded as a resource.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("embed the Windows icon resource");
    }
}
