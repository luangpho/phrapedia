//! Path helpers: traversal-safe joins and root-relative resolution (port of server/utils/paths.ts).

use super::error::{AppError, Result};
use std::path::{Path, PathBuf};

/// True when `child` is inside `parent` (or equal). Both must be absolute (lexical check).
pub fn is_inside(parent: &Path, child: &Path) -> bool {
    child.starts_with(parent)
}

/// Absolute stays; relative is taken from `root`. Empty = `root`.
pub fn resolve_from_root(root: &Path, p: &str) -> PathBuf {
    let p = p.trim();
    if p.is_empty() {
        return root.to_path_buf();
    }
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Join a user-supplied relative path under `base`. Rejects `..` components, absolute escapes, NUL bytes and
/// symlinks whose target leaves `base`. A leading `/` or `\` is tolerated (treated as relative), like the old safeJoin.
pub fn safe_join(base: &Path, rel: &str) -> Result<PathBuf> {
    let bad = || AppError::invalid(format!("パスが基準フォルダの外を指しています: {rel}"));
    if rel.contains('\0') {
        return Err(bad());
    }
    let normalised = rel.replace('\\', "/");
    let mut full = base.to_path_buf();
    for comp in normalised.split('/') {
        match comp {
            "" | "." => {}
            ".." => return Err(bad()),
            c => full.push(c),
        }
    }
    // Resolve symlinks of the deepest existing ancestor and make sure we are still inside the (resolved) base.
    let base_real = base.canonicalize().map_err(|e| AppError::io("基準フォルダを確認できません", e))?;
    let mut probe = full.clone();
    let real = loop {
        match probe.canonicalize() {
            Ok(p) => break p,
            Err(_) => {
                if !probe.pop() || probe.as_os_str().is_empty() {
                    return Err(bad());
                }
            }
        }
    };
    if !real.starts_with(&base_real) {
        return Err(bad());
    }
    Ok(full)
}

pub fn to_posix(p: &Path) -> String {
    p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn joins_inside_and_tolerates_leading_slash() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("notes")).unwrap();
        assert_eq!(safe_join(d.path(), "notes/a.md").unwrap(), d.path().join("notes/a.md"));
        assert_eq!(safe_join(d.path(), "/notes\\a.md").unwrap(), d.path().join("notes/a.md"));
        assert_eq!(safe_join(d.path(), "").unwrap(), d.path());
        assert_eq!(safe_join(d.path(), "not/yet/there.md").unwrap(), d.path().join("not/yet/there.md"));
    }

    #[test]
    fn rejects_traversal() {
        let d = tempfile::tempdir().unwrap();
        for bad in ["../x", "a/../../x", "..", "a/..", "a\\..\\..\\x", "x\0y"] {
            let e = safe_join(d.path(), bad).unwrap_err();
            assert_eq!(e.code, super::super::error::ErrorCode::InvalidArgument, "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_but_allows_inside_links() {
        let d = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "s").unwrap();
        let base = d.path().join("sources");
        fs::create_dir_all(base.join("real")).unwrap();
        std::os::unix::fs::symlink(outside.path(), base.join("escape")).unwrap();
        std::os::unix::fs::symlink(base.join("real"), base.join("inside")).unwrap();
        assert!(safe_join(&base, "escape/secret.md").is_err());
        assert!(safe_join(&base, "escape").is_err());
        assert!(safe_join(&base, "inside/x.md").is_ok());
    }

    #[test]
    fn resolve_from_root_behaviour() {
        let root = Path::new("/r");
        assert_eq!(resolve_from_root(root, ""), PathBuf::from("/r"));
        assert_eq!(resolve_from_root(root, "data"), PathBuf::from("/r/data"));
        assert_eq!(resolve_from_root(root, "/abs"), PathBuf::from("/abs"));
        assert!(is_inside(root, Path::new("/r/a")));
        assert!(!is_inside(root, Path::new("/x")));
    }
}
