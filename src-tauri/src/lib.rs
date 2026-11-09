mod settings;
mod sidecar;

use serde::Serialize;
use settings::PathSettings;
use sidecar::{Sidecar, SidecarConfig};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, RunEvent};

const START_TIMEOUT: Duration = Duration::from_secs(40);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Starting,
    Ready,
    Error,
}

#[derive(Default)]
struct Inner {
    sidecar: Option<Sidecar>,
    phase: Option<Phase>,
    base_url: Option<String>,
    error: Option<String>,
    /// Warning from loading settings.json (corrupt file etc.)
    settings_warning: Option<String>,
}

struct AppState(Mutex<Inner>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PathCheckView {
    ok: bool,
    error: Option<String>,
}

impl From<Result<(), String>> for PathCheckView {
    fn from(r: Result<(), String>) -> Self {
        match r {
            Ok(()) => PathCheckView { ok: true, error: None },
            Err(e) => PathCheckView { ok: false, error: Some(e) },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SidecarInfo {
    phase: Phase,
    base_url: Option<String>,
    error: Option<String>,
    settings_warning: Option<String>,
    dataset_check: PathCheckView,
    export_check: PathCheckView,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PathSettingsView {
    dataset_path: String,
    /// What the user typed (may be empty = same as dataset)
    export_path: String,
    effective_export: String,
    dataset_check: PathCheckView,
    export_check: PathCheckView,
    settings_file: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveOutcome {
    ok: bool,
    dataset_error: Option<String>,
    export_error: Option<String>,
    /// Set when saving succeeded but the server could not be restarted
    restart_error: Option<String>,
    settings: PathSettingsView,
}

fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_config_dir().map_err(|e| e.to_string())
}

fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Effective (dataset, export) strings for the stored settings.
fn resolve_paths(app: &AppHandle, s: &PathSettings) -> Result<(String, String), String> {
    let dataset = match s.dataset_path.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            let d = data_dir(app)?.join("dataset");
            // first run: make the default location usable; a user-chosen path is never created for them
            let _ = std::fs::create_dir_all(&d);
            d.to_string_lossy().into_owned()
        }
    };
    let export = s.effective_export(&dataset);
    Ok((dataset, export))
}

/// Locate index.mjs of the built server: bundled resource first, then the repo's .output (dev).
fn server_entry(app: &AppHandle) -> PathBuf {
    if let Ok(p) = std::env::var("PHRAPEDIA_SERVER_ENTRY") {
        return PathBuf::from(p);
    }
    if let Ok(res) = app.path().resource_dir() {
        let p = res.join("server").join("index.mjs");
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.output/server/index.mjs")
}

fn build_config(app: &AppHandle, dataset: String, export: String) -> Result<SidecarConfig, String> {
    let data = data_dir(app)?;
    Ok(SidecarConfig {
        node: sidecar::find_node()?,
        entry: server_entry(app),
        dataset_path: dataset,
        export_path: export,
        state_dir: data.join("state"),
        cwd: data.clone(),
        log_file: data.join("logs").join("sidecar.log"),
        allowed_origins: sidecar::WEBVIEW_ORIGINS.to_string(),
    })
}

/// (Re)start the sidecar with the stored settings. Never panics; failure ends up in `error`.
fn start_sidecar(app: &AppHandle) {
    let state = app.state::<AppState>();
    {
        let mut g = state.0.lock().unwrap();
        // stop first: the old server may hold the PGlite directory
        if let Some(mut old) = g.sidecar.take() {
            old.stop();
        }
        g.phase = Some(Phase::Starting);
        g.base_url = None;
        g.error = None;
    }
    let result = (|| -> Result<Sidecar, String> {
        let dir = config_dir(app)?;
        let (stored, warning) = settings::load(&dir);
        state.0.lock().unwrap().settings_warning = warning;
        let (dataset, export) = resolve_paths(app, &stored)?;
        let cfg = build_config(app, dataset, export)?;
        Sidecar::start(&cfg, START_TIMEOUT)
    })();
    let mut g = state.0.lock().unwrap();
    match result {
        Ok(sc) => {
            g.base_url = Some(format!("http://127.0.0.1:{}", sc.port));
            g.sidecar = Some(sc);
            g.phase = Some(Phase::Ready);
        }
        Err(e) => {
            g.error = Some(e);
            g.phase = Some(Phase::Error);
        }
    }
}

fn path_settings_view(app: &AppHandle) -> Result<PathSettingsView, String> {
    let dir = config_dir(app)?;
    let (stored, _) = settings::load(&dir);
    let (dataset, export) = resolve_paths(app, &stored)?;
    Ok(PathSettingsView {
        dataset_check: settings::check_dataset(&dataset).into(),
        export_check: settings::check_export(&export).into(),
        dataset_path: dataset,
        export_path: stored.export_path.clone().unwrap_or_default(),
        effective_export: export,
        settings_file: dir.join(settings::SETTINGS_FILE).to_string_lossy().into_owned(),
    })
}

/// Waits until the sidecar finished starting (or failed), then reports its state. The UI never has to poll.
#[tauri::command]
async fn sidecar_info(app: AppHandle) -> Result<SidecarInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let deadline = std::time::Instant::now() + START_TIMEOUT + Duration::from_secs(5);
        loop {
            let phase = state.0.lock().unwrap().phase.clone();
            if (phase.is_some() && phase != Some(Phase::Starting)) || std::time::Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let view = path_settings_view(&app)?;
        let mut g = state.0.lock().unwrap();
        // a crashed server must show as an error, not as a stale "ready"
        let dead = g.sidecar.as_mut().is_some_and(|sc| !sc.is_running());
        if dead {
            g.sidecar = None;
            g.base_url = None;
            g.phase = Some(Phase::Error);
            g.error = Some("サーバープロセスが終了しました。設定画面から再起動してください".into());
        }
        Ok(SidecarInfo {
            phase: g.phase.clone().unwrap_or(Phase::Starting),
            base_url: g.base_url.clone(),
            error: g.error.clone(),
            settings_warning: g.settings_warning.clone(),
            dataset_check: view.dataset_check,
            export_check: view.export_check,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_path_settings(app: AppHandle) -> Result<PathSettingsView, String> {
    path_settings_view(&app)
}

/// Validate, persist (only when valid) and restart the server so the new paths take effect.
#[tauri::command]
async fn save_path_settings(app: AppHandle, dataset_path: String, export_path: String) -> Result<SaveOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dataset = dataset_path.trim().to_string();
        let export = export_path.trim().to_string();
        let effective_export = if export.is_empty() { dataset.clone() } else { export.clone() };
        let dataset_error = settings::check_dataset(&dataset).err();
        let export_error = settings::check_export(&effective_export).err();
        if dataset_error.is_some() || export_error.is_some() {
            return Ok(SaveOutcome { ok: false, dataset_error, export_error, restart_error: None, settings: path_settings_view(&app)? });
        }
        let new = PathSettings { dataset_path: Some(dataset), export_path: if export.is_empty() { None } else { Some(export) } };
        settings::save(&config_dir(&app)?, &new)?;
        start_sidecar(&app);
        let restart_error = app.state::<AppState>().0.lock().unwrap().error.clone();
        Ok(SaveOutcome { ok: true, dataset_error: None, export_error: None, restart_error, settings: path_settings_view(&app)? })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Restart the server with the stored settings (used by the "retry" button).
#[tauri::command]
async fn restart_sidecar(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start_sidecar(&app)).await.map_err(|e| e.to_string())
}

/// True when this build carries `plugins.updater` (merged in by tauri.updater.conf.json). The frontend shows the
/// header update icon only then; the plugin itself is registered under the same condition in `run`.
fn updater_configured(app: &AppHandle) -> bool {
    app.config().plugins.0.contains_key("updater")
}

#[tauri::command]
fn updater_enabled(app: AppHandle) -> bool {
    updater_configured(&app)
}

fn stop_sidecar(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut g) = state.0.lock() {
            if let Some(mut sc) = g.sidecar.take() {
                sc.stop();
            }
        }
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage(AppState(Mutex::new(Inner::default())))
        .setup(|app| {
            // The updater plugin fails to initialize without `plugins.updater` (it is only merged in by
            // tauri.updater.conf.json once a key is registered), so register it only when configured.
            // Without it `updater_enabled` is false and the frontend hides the update icon.
            if updater_configured(app.handle()) {
                app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            // Start in the background so the window appears immediately; a failure never aborts startup.
            let handle = app.handle().clone();
            std::thread::spawn(move || start_sidecar(&handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![sidecar_info, get_path_settings, save_path_settings, restart_sidecar, updater_enabled])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
            stop_sidecar(handle);
        }
    });
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
}
