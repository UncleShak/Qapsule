use tauri::Manager;

mod window;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let main = app
                .get_webview_window("main")
                .expect("main window is declared in tauri.conf.json");
            window::setup(&main)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        assert_eq!(env!("CARGO_PKG_NAME"), "qapsule-app");
    }
}
