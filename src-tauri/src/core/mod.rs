//! Tauri-independent core: settings, records, primary sources, SQLite store, git sync, web research.
//! Used by the Tauri command layer and by the `phrapedia mcp` stdio subcommand.

pub mod app_settings;
pub mod cli;
pub mod entities;
pub mod error;
pub mod frontmatter;
pub mod git;
pub mod path_settings;
pub mod paths;
pub mod research;
pub mod slug;
pub mod sources;
pub mod store;
pub mod types;

use app_settings::{AppSettings, ResearchEngine};
use error::{AppError, Result};
use path_settings::PathSettings;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const IDENTIFIER: &str = "jp.grrow.phrapedia";

/// Where the app keeps its own files (not the dataset).
#[derive(Debug, Clone)]
pub struct AppDirs {
    /// settings.json (2 paths) + app-settings.json
    pub config_dir: PathBuf,
    /// state/ (SQLite, CLI work dirs) and the default dataset
    pub data_dir: PathBuf,
}

impl AppDirs {
    /// OS-standard locations for the bundle identifier (same as Tauri's `app_config_dir` / `app_data_dir`).
    /// Used by `phrapedia mcp` when the dirs are not passed explicitly.
    pub fn from_os() -> AppDirs {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
        let env_dir = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let (config, data) = if cfg!(target_os = "macos") {
            let base = home.join("Library/Application Support");
            (base.clone(), base)
        } else if cfg!(windows) {
            let base = env_dir("APPDATA").unwrap_or_else(|| home.join("AppData/Roaming"));
            (base.clone(), base)
        } else {
            (env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")), env_dir("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")))
        };
        AppDirs { config_dir: config.join(IDENTIFIER), data_dir: data.join(IDENTIFIER) }
    }

    pub fn state_dir(&self) -> PathBuf {
        self.data_dir.join("state")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PathCheckView {
    pub ok: bool,
    pub error: Option<String>,
}

impl From<std::result::Result<(), String>> for PathCheckView {
    fn from(r: std::result::Result<(), String>) -> Self {
        match r {
            Ok(()) => PathCheckView { ok: true, error: None },
            Err(e) => PathCheckView { ok: false, error: Some(e) },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub settings_warning: Option<String>,
    pub dataset_path: String,
    pub export_path: String,
    pub dataset_check: PathCheckView,
    pub export_check: PathCheckView,
    pub state_dir: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathSettingsView {
    pub dataset_path: String,
    pub export_path: String,
    pub effective_export: String,
    pub dataset_check: PathCheckView,
    pub export_check: PathCheckView,
    pub settings_file: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathsOutcome {
    pub ok: bool,
    pub dataset_error: Option<String>,
    pub export_error: Option<String>,
    pub settings: PathSettingsView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPaths {
    pub sources_dir: String,
    pub data_dir: String,
    pub dataset_root: String,
    pub export_root: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub claude_found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_version: Option<String>,
    pub gemini_found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gemini_version: Option<String>,
    pub gemini_engine: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: AppSettings,
    pub resolved: ResolvedPaths,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone)]
pub struct CliProbe {
    pub found: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

pub struct Core {
    pub dirs: AppDirs,
    db: Mutex<Option<rusqlite::Connection>>,
    claude_probe: Mutex<Option<(Instant, String, CliProbe)>>,
}

impl Core {
    pub fn new(dirs: AppDirs) -> Core {
        Core { dirs, db: Mutex::new(None), claude_probe: Mutex::new(None) }
    }

    // ---- paths ----

    pub fn path_settings(&self) -> (PathSettings, Option<String>) {
        path_settings::load(&self.dirs.config_dir)
    }

    /// (dataset, export) as plain strings, without validation. The default dataset dir is created on first use.
    pub fn effective_paths(&self) -> (String, String) {
        let (stored, _) = self.path_settings();
        let dataset = match stored.dataset_path.as_deref().map(str::trim) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => {
                let d = self.dirs.data_dir.join("dataset");
                let _ = std::fs::create_dir_all(&d);
                d.to_string_lossy().into_owned()
            }
        };
        let export = stored.effective_export(&dataset);
        (dataset, export)
    }

    /// Dataset root (read side). Errors with `path_unusable` when the folder is missing / unreadable.
    pub fn dataset_root(&self) -> Result<PathBuf> {
        let (dataset, _) = self.effective_paths();
        path_settings::check_dataset(&dataset).map_err(|e| AppError::path_unusable(format!("データセットパスが使えません: {e}")))?;
        Ok(PathBuf::from(dataset))
    }

    /// Export root (write side + git working tree).
    pub fn export_root(&self) -> Result<PathBuf> {
        let (_, export) = self.effective_paths();
        path_settings::check_export(&export).map_err(|e| AppError::path_unusable(format!("書き出し先パスが使えません: {e}")))?;
        Ok(PathBuf::from(export))
    }

    pub fn same_roots(&self) -> bool {
        let (d, e) = self.effective_paths();
        d == e
    }

    pub fn path_settings_view(&self) -> PathSettingsView {
        let (stored, _) = self.path_settings();
        let (dataset, export) = self.effective_paths();
        PathSettingsView {
            dataset_check: path_settings::check_dataset(&dataset).into(),
            export_check: path_settings::check_export(&export).into(),
            dataset_path: dataset,
            export_path: stored.export_path.clone().unwrap_or_default(),
            effective_export: export,
            settings_file: self.dirs.config_dir.join(path_settings::SETTINGS_FILE).to_string_lossy().into_owned(),
        }
    }

    /// Validate, then persist only when both paths are usable.
    pub fn save_path_settings(&self, dataset_path: &str, export_path: &str) -> Result<SavePathsOutcome> {
        let dataset = dataset_path.trim().to_string();
        let export = export_path.trim().to_string();
        let effective_export = if export.is_empty() { dataset.clone() } else { export.clone() };
        let dataset_error = path_settings::check_dataset(&dataset).err();
        let export_error = path_settings::check_export(&effective_export).err();
        if dataset_error.is_some() || export_error.is_some() {
            return Ok(SavePathsOutcome { ok: false, dataset_error, export_error, settings: self.path_settings_view() });
        }
        let new = PathSettings { dataset_path: Some(dataset), export_path: if export.is_empty() { None } else { Some(export) } };
        path_settings::save(&self.dirs.config_dir, &new).map_err(|e| AppError::io("パス設定を保存できません", e))?;
        Ok(SavePathsOutcome { ok: true, dataset_error: None, export_error: None, settings: self.path_settings_view() })
    }

    pub fn app_info(&self) -> AppInfo {
        let (_, w1) = self.path_settings();
        let (_, w2) = app_settings::load_overrides(&self.dirs.config_dir);
        let warning = match (w1, w2) {
            (None, None) => None,
            (a, b) => Some([a, b].into_iter().flatten().collect::<Vec<_>>().join(" / ")),
        };
        let v = self.path_settings_view();
        AppInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            settings_warning: warning,
            dataset_path: v.dataset_path,
            export_path: v.effective_export,
            dataset_check: v.dataset_check,
            export_check: v.export_check,
            state_dir: self.dirs.state_dir().to_string_lossy().into_owned(),
        }
    }

    // ---- AppSettings ----

    pub fn settings(&self) -> AppSettings {
        let (overrides, _) = app_settings::load_overrides(&self.dirs.config_dir);
        app_settings::effective(&overrides)
    }

    /// Validate and persist a settings patch (only the user's overrides are stored). Returns the effective settings.
    pub fn update_settings(&self, patch: &Value) -> Result<AppSettings> {
        let normalised = app_settings::validate_patch(patch)?;
        let (stored, _) = app_settings::load_overrides(&self.dirs.config_dir);
        let merged = app_settings::deep_merge(&stored, &normalised);
        app_settings::save_overrides(&self.dirs.config_dir, &merged)?;
        Ok(app_settings::effective(&merged))
    }

    /// Absolute path of the primary-sources directory.
    pub fn sources_dir(&self) -> Result<PathBuf> {
        let root = self.dataset_root()?;
        Ok(paths::resolve_from_root(&root, &self.settings().sources_dir))
    }

    /// Records directory to read from (inside the dataset path).
    pub fn data_dir(&self) -> Result<PathBuf> {
        let root = self.dataset_root()?;
        Ok(paths::resolve_from_root(&root, &self.settings().data_dir))
    }

    /// Records directory to write to (inside the export path), mirroring `data_dir()`'s location relative to the dataset.
    pub fn export_data_dir(&self) -> Result<PathBuf> {
        let read = self.data_dir()?;
        if self.same_roots() {
            return Ok(read);
        }
        let export = self.export_root()?;
        let dataset = self.dataset_root()?;
        let rel = if paths::is_inside(&dataset, &read) { read.strip_prefix(&dataset).map(Path::to_path_buf).unwrap_or_default() } else { PathBuf::from("data") };
        Ok(if rel.as_os_str().is_empty() { export.join("data") } else { export.join(rel) })
    }

    // ---- CLI probes / capabilities ----

    fn probe(&self, command: &str) -> CliProbe {
        match cli::run_capture(command, &["--version".into()], &cli::RunOpts { timeout: Duration::from_secs(15), ..Default::default() }) {
            Ok(o) if o.code == Some(0) => CliProbe { found: true, version: o.stdout_text().lines().next().map(|s| s.trim().to_string()), error: None },
            Ok(o) => {
                let text = if o.stderr.is_empty() { o.stdout_text() } else { o.stderr_text() };
                CliProbe { found: false, version: None, error: Some(text.trim().chars().take(400).collect()) }
            }
            Err(e) => CliProbe { found: false, version: None, error: Some(e.to_string()) },
        }
    }

    /// `claude --version`, cached for 60 seconds per command.
    pub fn probe_claude(&self, command: &str) -> CliProbe {
        if let Some((at, cmd, p)) = self.claude_probe.lock().unwrap().as_ref() {
            if cmd == command && at.elapsed() < Duration::from_secs(60) {
                return p.clone();
            }
        }
        let p = self.probe(command);
        *self.claude_probe.lock().unwrap() = Some((Instant::now(), command.to_string(), p.clone()));
        p
    }

    pub fn probe_research(&self) -> (CliProbe, ResearchEngine, String) {
        let s = self.settings();
        let engine = s.gemini.engine;
        let command = if s.gemini.command.trim().is_empty() { research::default_command(engine).to_string() } else { s.gemini.command.clone() };
        (self.probe(&command), engine, command)
    }

    pub fn settings_view(&self) -> Result<SettingsView> {
        let s = self.settings();
        let ((gemini, engine, _), claude) = std::thread::scope(|sc| {
            let g = sc.spawn(|| self.probe_research());
            let c = self.probe_claude(&s.ai.command);
            (g.join().expect("probe thread"), c)
        });
        let dataset = self.dataset_root().ok();
        let (ds, ex) = self.effective_paths();
        let base = dataset.unwrap_or_else(|| PathBuf::from(&ds));
        Ok(SettingsView {
            resolved: ResolvedPaths {
                sources_dir: paths::resolve_from_root(&base, &s.sources_dir).to_string_lossy().into_owned(),
                data_dir: paths::resolve_from_root(&base, &s.data_dir).to_string_lossy().into_owned(),
                dataset_root: ds,
                export_root: ex,
            },
            capabilities: Capabilities {
                claude_found: claude.found,
                claude_version: claude.version,
                gemini_found: gemini.found,
                gemini_version: gemini.version,
                gemini_engine: research::engine_label(engine).to_string(),
            },
            settings: s,
        })
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    /// A Core whose config / data dirs and dataset live in temp dirs. Hold the returned TempDir.
    pub fn core_with_dataset() -> (tempfile::TempDir, Core) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = AppDirs { config_dir: tmp.path().join("config"), data_dir: tmp.path().join("appdata") };
        let dataset = tmp.path().join("dataset");
        std::fs::create_dir_all(&dataset).unwrap();
        let core = Core::new(dirs);
        let out = core.save_path_settings(dataset.to_str().unwrap(), "").unwrap();
        assert!(out.ok, "{out:?}");
        (tmp, core)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dataset_is_created_under_app_data() {
        let tmp = tempfile::tempdir().unwrap();
        let core = Core::new(AppDirs { config_dir: tmp.path().join("c"), data_dir: tmp.path().join("d") });
        let root = core.dataset_root().unwrap();
        assert_eq!(root, tmp.path().join("d").join("dataset"));
        assert!(root.is_dir());
        assert!(core.same_roots());
    }

    #[test]
    fn unusable_paths_are_path_unusable() {
        let tmp = tempfile::tempdir().unwrap();
        let core = Core::new(AppDirs { config_dir: tmp.path().join("c"), data_dir: tmp.path().join("d") });
        std::fs::create_dir_all(tmp.path().join("c")).unwrap();
        std::fs::write(tmp.path().join("c/settings.json"), format!(r#"{{"datasetPath":"{}"}}"#, tmp.path().join("missing").display())).unwrap();
        let e = core.dataset_root().unwrap_err();
        assert_eq!(e.code, error::ErrorCode::PathUnusable);
        assert!(e.message.contains("データセットパスが使えません"));
        assert!(!core.app_info().dataset_check.ok);
    }

    #[test]
    fn save_path_settings_rejects_without_persisting() {
        let tmp = tempfile::tempdir().unwrap();
        let core = Core::new(AppDirs { config_dir: tmp.path().join("c"), data_dir: tmp.path().join("d") });
        let out = core.save_path_settings("relative", "").unwrap();
        assert!(!out.ok);
        assert!(out.dataset_error.unwrap().contains("絶対パス"));
        assert!(!tmp.path().join("c/settings.json").exists());
    }

    #[test]
    fn export_data_dir_mirrors_relative_location() {
        let tmp = tempfile::tempdir().unwrap();
        let ds = tmp.path().join("ds");
        let ex = tmp.path().join("ex");
        std::fs::create_dir_all(&ds).unwrap();
        std::fs::create_dir_all(&ex).unwrap();
        let core = Core::new(AppDirs { config_dir: tmp.path().join("c"), data_dir: tmp.path().join("d") });
        assert!(core.save_path_settings(ds.to_str().unwrap(), ex.to_str().unwrap()).unwrap().ok);
        assert_eq!(core.data_dir().unwrap(), ds.join("data"));
        assert_eq!(core.export_data_dir().unwrap(), ex.join("data"));
        core.update_settings(&serde_json::json!({ "dataDir": "records" })).unwrap();
        assert_eq!(core.export_data_dir().unwrap(), ex.join("records"));
        // a data dir outside the dataset falls back to data/ under the export path
        core.update_settings(&serde_json::json!({ "dataDir": "/somewhere/else" })).unwrap();
        assert_eq!(core.export_data_dir().unwrap(), ex.join("data"));
    }

    #[test]
    fn update_settings_stores_only_overrides() {
        let (_t, core) = testutil::core_with_dataset();
        let s = core.update_settings(&serde_json::json!({ "git": { "autoCommit": true } })).unwrap();
        assert!(s.git.auto_commit);
        let text = std::fs::read_to_string(core.dirs.config_dir.join("app-settings.json")).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v, serde_json::json!({ "git": { "autoCommit": true } }));
        // a later patch merges into the stored overrides
        core.update_settings(&serde_json::json!({ "ai": { "model": "m" } })).unwrap();
        let s = core.settings();
        assert!(s.git.auto_commit);
        assert_eq!(s.ai.model, "m");
    }
}
