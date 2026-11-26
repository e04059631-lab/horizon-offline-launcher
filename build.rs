fn main() {
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent-light".into()),
    )
    .unwrap();

    #[cfg(windows)]
    {
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
            let mut res = winresource::WindowsResource::new();
            res.set_icon("assets/icon.ico");
            res.set("ProductName", "Horizon Launcher");
            res.set("FileDescription", "Horizon Launcher");
            res.set("FileVersion", "1.0.0");
            res.set("ProductVersion", "1.0.0");
            if let Err(e) = res.compile() {
                eprintln!("winresource compile note: {:?}", e);
            }
        }
    }
}
