fn main() {
    // The manifest is embedded by the linker instead of by tauri-build, so
    // test binaries get it too; without it they fail to start
    // (STATUS_ENTRYPOINT_NOT_FOUND) once Windows notifications are linked.
    let mut windows = tauri_build::WindowsAttributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        let dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
        let manifest = std::path::Path::new(&dir).join("app-manifest.xml");
        println!("cargo:rerun-if-changed=app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri build step failed");
}
