fn main() {
    // The `<msix>` manifest element claims the dev publisher, so it is embedded
    // in debug builds only. Release builds get the real publisher's `<msix>`
    // when SignPath/Store lands (plan, Phase 5 next steps).
    if std::env::var("PROFILE").as_deref() != Ok("debug") {
        tauri_build::build();
        return;
    }
    println!("cargo:rerun-if-changed=qapsule.exe.manifest");
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("qapsule.exe.manifest"));
    if let Err(e) =
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
    {
        panic!("{e:#}");
    }
}
