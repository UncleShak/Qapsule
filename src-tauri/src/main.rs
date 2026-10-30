// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--identity-check") {
        println!("{}", qapsule_app_lib::identity::check());
        return;
    }
    qapsule_app_lib::run()
}
