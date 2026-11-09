pub mod commands;
pub mod core;

use crate::core::{AppDirs, Core};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// True when this build carries `plugins.updater` (merged in by tauri.updater.conf.json). The frontend shows the
/// header update icon only then; the plugin itself is registered under the same condition in `run`.
fn updater_configured(app: &AppHandle) -> bool {
    app.config().plugins.0.contains_key("updater")
}

#[tauri::command]
fn updater_enabled(app: AppHandle) -> bool {
    updater_configured(&app)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // The updater plugin fails to initialize without `plugins.updater` (it is only merged in by
            // tauri.updater.conf.json once a key is registered), so register it only when configured.
            // Without it `updater_enabled` is false and the frontend hides the update icon.
            if updater_configured(app.handle()) {
                app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            let dirs = AppDirs { config_dir: app.path().app_config_dir()?, data_dir: app.path().app_data_dir()? };
            app.manage(Arc::new(Core::new(dirs)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_path_settings,
            commands::save_path_settings,
            commands::get_settings,
            commands::update_settings,
            commands::reindex_sources,
            commands::list_entities,
            commands::entity_template,
            commands::get_entity,
            commands::create_entity,
            commands::update_entity,
            commands::delete_entity,
            commands::list_sources,
            commands::search_sources,
            commands::read_source,
            commands::read_source_bytes,
            commands::web_research,
            commands::sync_status,
            commands::commit_data,
            commands::pull_data,
            commands::push_data,
            commands::sync_data,
            commands::list_chat_sessions,
            commands::get_chat_session,
            commands::delete_chat_session,
            commands::patch_chat_session,
            commands::delete_chat_messages,
            updater_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod updater_config_tests {
    //! tauri.updater.conf.json is only merged in by the release workflow once a key is registered,
    //! so a typo would otherwise surface only in a release build.
    use serde_json::Value;

    #[test]
    fn updater_overlay_config_loads() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.updater.conf.json")).expect("valid JSON");
        let cfg: tauri_plugin_updater::Config =
            serde_json::from_value(conf["plugins"]["updater"].clone()).expect("plugins.updater deserializes into the plugin Config");
        assert_eq!(
            cfg.endpoints.iter().map(|u| u.as_str()).collect::<Vec<_>>(),
            ["https://github.com/luangpho/phrapedia/releases/latest/download/latest.json"]
        );
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], Value::Bool(true));
    }

    #[test]
    fn base_config_has_no_updater_and_is_ad_hoc_signed() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(conf["plugins"].get("updater").is_none(), "updater config must live in tauri.updater.conf.json");
        assert_eq!(conf["bundle"]["macOS"]["signingIdentity"], "-");
    }

    #[test]
    fn capabilities_allow_updater_and_relaunch() {
        let caps: Value = serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let perms: Vec<&str> = caps["permissions"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(perms.contains(&"updater:default"));
        assert!(perms.contains(&"process:allow-restart"));
        assert!(perms.contains(&"dialog:allow-message"), "ask() goes through dialog message");
        assert!(!perms.contains(&"process:default"), "process:default also grants exit");
    }

    #[test]
    fn bundle_overlay_no_longer_ships_a_node_server() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.bundle.conf.json")).unwrap();
        assert!(conf["bundle"].get("resources").is_none(), "no .output/server resource: the app runs without Node");
    }
}
