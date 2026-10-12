// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // started as the Steam list helper (see pzmm_core::pz::steam_client): list and exit, no window
    if let Some(code) = pzmm_core::pz::steam_client::child() {
        std::process::exit(code);
    }
    pzmm_app_lib::run()
}
