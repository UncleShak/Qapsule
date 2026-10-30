fn main() {
    // The `<msix>` manifest element claims the dev publisher, so it is embedded
    // in debug builds only. Release builds embed it only when
    // QAPSULE_DEV_IDENTITY=1, for local measurement (scripts/measure-idle.ps1);
    // never for builds that are shipped. Those get the real publisher's `<msix>`
    // when SignPath/Store lands (plan, Phase 5 next steps).
    println!("cargo:rerun-if-env-changed=QAPSULE_DEV_IDENTITY");
    let dev_identity = std::env::var("QAPSULE_DEV_IDENTITY").as_deref() == Ok("1");
    if std::env::var("PROFILE").as_deref() != Ok("debug") && !dev_identity {
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
