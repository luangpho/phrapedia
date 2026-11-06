//! Path settings (dataset path / export path) persisted as settings.json in the Tauri app config dir,
//! plus path validation shared by the Tauri commands.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathSettings {
    /// Directory holding data/<kind>/*.md and sources/ (read side).
    #[serde(default)]
    pub dataset_path: Option<String>,
    /// Directory records are written to (and the git working tree). None / empty = same as dataset_path.
    #[serde(default)]
    pub export_path: Option<String>,
}

impl PathSettings {
    /// Export path actually used: the explicit one, else the dataset path.
    pub fn effective_export(&self, dataset: &str) -> String {
        match self.export_path.as_deref().map(str::trim) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => dataset.to_string(),
        }
    }
}

/// Load settings.json. A missing file gives defaults; an unreadable / corrupt file also gives defaults plus a
/// warning, so the app still starts.
pub fn load(config_dir: &Path) -> (PathSettings, Option<String>) {
    let file = config_dir.join(SETTINGS_FILE);
    match fs::read_to_string(&file) {
        Ok(text) => match serde_json::from_str::<PathSettings>(&text) {
            Ok(s) => (s, None),
            Err(e) => (PathSettings::default(), Some(format!("{} を読めません（既定値を使用）: {e}", file.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (PathSettings::default(), None),
        Err(e) => (PathSettings::default(), Some(format!("{} を開けません（既定値を使用）: {e}", file.display()))),
    }
}

/// Write settings.json atomically (temp file + rename).
pub fn save(config_dir: &Path, settings: &PathSettings) -> Result<(), String> {
    fs::create_dir_all(config_dir).map_err(|e| format!("設定フォルダを作れません: {e}"))?;
    let file = config_dir.join(SETTINGS_FILE);
    let tmp: PathBuf = config_dir.join(format!("{SETTINGS_FILE}.tmp"));
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&tmp, text).map_err(|e| format!("設定を書き込めません: {e}"))?;
    fs::rename(&tmp, &file).map_err(|e| format!("設定を保存できません: {e}"))
}

/// Dataset path: an existing, readable directory.
pub fn check_dataset(path: &str) -> Result<(), String> {
    check_dir(path, false)
}

/// Export path: an existing directory we can create files in.
pub fn check_export(path: &str) -> Result<(), String> {
    check_dir(path, true)
}

fn check_dir(path: &str, need_write: bool) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("パスが指定されていません".into());
    }
    let p = Path::new(path);
    // The sidecar runs with its own cwd, so a relative path would mean something different there than here.
    if !p.is_absolute() {
        return Err(format!("絶対パスを指定してください: {path}"));
    }
    let meta = match fs::metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(format!("フォルダが存在しません: {path}")),
        Err(e) => return Err(format!("フォルダを確認できません ({e}): {path}")),
    };
    if !meta.is_dir() {
        return Err(format!("フォルダではありません: {path}"));
    }
    if fs::read_dir(p).is_err() {
        return Err(format!("読み取り権限がありません: {path}"));
    }
    if need_write {
        // A real write probe: permission bits alone miss read-only mounts and ACLs.
        let probe = p.join(format!(".phrapedia-write-test-{}", std::process::id()));
        match fs::write(&probe, b"") {
            Ok(()) => {
                let _ = fs::remove_file(&probe);
            }
            Err(_) => return Err(format!("書き込み権限がありません: {path}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let d = tempfile::tempdir().unwrap();
        let (s, w) = load(d.path());
        assert_eq!(s, PathSettings::default());
        assert!(w.is_none());
        let want = PathSettings { dataset_path: Some("/a".into()), export_path: Some("/b".into()) };
        save(d.path(), &want).unwrap();
        let (got, w) = load(d.path());
        assert_eq!(got, want);
        assert!(w.is_none());
        assert!(!d.path().join("settings.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_falls_back_with_warning() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join(SETTINGS_FILE), "{ not json").unwrap();
        let (s, w) = load(d.path());
        assert_eq!(s, PathSettings::default());
        assert!(w.is_some());
    }

    #[test]
    fn export_defaults_to_dataset() {
        let s = PathSettings { dataset_path: Some("/d".into()), export_path: None };
        assert_eq!(s.effective_export("/d"), "/d");
        let s = PathSettings { dataset_path: Some("/d".into()), export_path: Some("  ".into()) };
        assert_eq!(s.effective_export("/d"), "/d");
        let s = PathSettings { dataset_path: Some("/d".into()), export_path: Some("/e".into()) };
        assert_eq!(s.effective_export("/d"), "/e");
    }

    #[test]
    fn validation() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().to_str().unwrap();
        assert!(check_dataset(p).is_ok());
        assert!(check_export(p).is_ok());
        assert!(check_dataset("").unwrap_err().contains("指定"));
        assert!(check_dataset("relative/dir").unwrap_err().contains("絶対パス"));
        assert!(check_export("~/data").unwrap_err().contains("絶対パス"));
        let missing = d.path().join("missing");
        assert!(check_dataset(missing.to_str().unwrap()).unwrap_err().contains("存在しません"));
        let f = d.path().join("f.txt");
        fs::write(&f, "x").unwrap();
        assert!(check_export(f.to_str().unwrap()).unwrap_err().contains("フォルダではありません"));
    }

    #[cfg(unix)]
    #[test]
    fn read_only_export_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let ro = d.path().join("ro");
        fs::create_dir(&ro).unwrap();
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o500)).unwrap();
        // root ignores permission bits, so only assert when the probe really cannot write
        let can_write = fs::write(ro.join("x"), b"").is_ok();
        if !can_write {
            assert!(check_export(ro.to_str().unwrap()).unwrap_err().contains("書き込み"));
            assert!(check_dataset(ro.to_str().unwrap()).is_ok());
        }
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o700)).unwrap();
    }
}
