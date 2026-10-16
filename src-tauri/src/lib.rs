mod archive;
mod game;
mod liveries;
mod loaders;
mod mods;
mod nexus;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    pub game_dir: Option<String>,
    pub nexus_key: Option<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("settings.json"))
}

pub(crate) fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub(crate) fn save_settings(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let sp = settings_path(app)?;
    std::fs::create_dir_all(sp.parent().unwrap()).map_err(|e| e.to_string())?;
    let json = serde_json::to_vec(s).map_err(|e| e.to_string())?;
    std::fs::write(sp, json).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // deve ser o primeiro: repassa links nxm:// da 2ª instância para a 1ª
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            game::get_game,
            game::set_game_dir,
            game::game_art,
            loaders::loaders_status,
            loaders::install_loader,
            loaders::set_active_loader,
            loaders::uninstall_loader,
            mods::list_mods,
            mods::set_mod_enabled,
            mods::uninstall_mod,
            mods::install_archive,
            liveries::list_liveries,
            liveries::set_livery_enabled,
            liveries::remove_livery,
            nexus::nexus_mods,
            nexus::nexus_account,
            nexus::nexus_connect,
            nexus::nexus_disconnect,
            nexus::nexus_install,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o app");
}
