fn main() {
    println!("cargo:rerun-if-changed=app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("app.ico")
            .compile()
            .expect("Failed to embed app.ico in the Windows executable");
    }
}
