/// Every app command. Once an app manifest exists, Tauri refuses any app command that
/// no capability grants, so the capsule's commands must be listed here and granted in
/// capabilities/default.json, and the settings window's in capabilities/settings.json.
/// A command missing from here fails at runtime with "not allowed. Command not found".
const COMMANDS: &[&str] = &[
    // The capsule (capabilities/default.json).
    "get_snapshot",
    "press_action",
    "press_history",
    "dismiss_pop",
    "undo",
    "swap_peek",
    "set_pointer",
    "pointer_slowed",
    "expand_click",
    "media_click",
    "close_expanded",
    "set_hit_rects",
    "focus_toggle",
    "focus_reset",
    "focus_length",
    "focus_module",
    "set_screen",
    "step_screen",
    "set_volume",
    "toggle_mute",
    "open_system_page",
    "toggle_dnd",
    "open_settings",
    // The settings window only (capabilities/settings.json).
    "get_appearance",
    "set_appearance",
];

fn attributes() -> tauri_build::Attributes {
    tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS))
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
