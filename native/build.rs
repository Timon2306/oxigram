fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("Slint UI failed to compile");

    // The exe carries the app icon (Explorer, taskbar, Alt+Tab, the shortcut).
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=icons/icon.ico");
        let mut res = tauri_winres::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        res.set("ProductName", "Oxigram");
        res.set("FileDescription", "Oxigram");
        res.compile().expect("failed to embed the Windows icon");
    }
}
