/// Commands that only the settings window may call (see capabilities/settings.json).
/// Listing them here makes Tauri require a permission for each; the capsule's
/// capability doesn't grant them.
fn attributes() -> tauri_build::Attributes {
    tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["get_appearance", "set_appearance"]),
    )
}

fn main() {
    // The `<msix>` manifest element claims the dev publisher, so it is embedded
    // in debug builds only. Release builds embed it only when
    // QAPSULE_DEV_IDENTITY=1, for local measurement (scripts/measure-idle.ps1);
    // never for builds that are shipped. Those get the real publisher's `<msix>`
    // when SignPath/Store lands (plan, Phase 5 next steps).
    println!("cargo:rerun-if-env-changed=QAPSULE_DEV_IDENTITY");
    let dev_identity = std::env::var("QAPSULE_DEV_IDENTITY").as_deref() == Ok("1");
    if std::env::var("PROFILE").as_deref() != Ok("debug") && !dev_identity {
        if let Err(e) = tauri_build::try_build(attributes()) {
            panic!("{e:#}");
        }
        return;
    }
    println!("cargo:rerun-if-changed=qapsule.exe.manifest");
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("qapsule.exe.manifest"));
    if let Err(e) = tauri_build::try_build(attributes().windows_attributes(windows)) {
        panic!("{e:#}");
    }
}
