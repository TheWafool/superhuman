// Embeds the app icon in the Windows .exe.
fn main() {
    println!("cargo:rerun-if-changed=dist/superhuman.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut r = winresource::WindowsResource::new();
        r.set_icon("dist/superhuman.ico");
        r.compile().expect("embedding the icon");
    }
}
