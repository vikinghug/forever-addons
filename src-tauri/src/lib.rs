//! An addon manager for World of Warcraft: Forever clients.
//!
//! The crate is layered: [`domain`] holds the vocabulary, [`source`] talks to
//! addon providers, [`wow`] reads the local client, [`install`] writes to it,
//! and [`commands`] is the only surface the frontend can reach.

pub mod catalog;
pub mod commands;
pub mod config;
pub mod domain;
pub mod error;
pub mod install;
pub mod source;
pub mod state;
pub mod view;
pub mod wow;

use tauri::Manager;

use crate::state::AppState;

/// Builds and runs the desktop application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState::load(data_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::detect_wow_install,
            commands::set_client_path,
            commands::set_enabled_sources,
            commands::set_curseforge_api_key,
            commands::set_wago_token,
            commands::add_github_repo,
            commands::remove_github_repo,
            commands::refresh_source,
            commands::search_catalog,
            commands::list_categories,
            commands::get_addon_detail,
            commands::list_installed,
            commands::install_addon,
            commands::uninstall_addon,
            commands::open_url,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("forever-addons failed to start: {error}");
        std::process::exit(1);
    }
}
