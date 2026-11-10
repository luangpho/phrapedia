//! Git sync of the data directory (port of server/utils/git.ts; same behaviour as the simple-git version).
//! Spawns the `git` CLI in the export root. Only the data directory is ever staged.

use super::app_settings::is_safe_git_name;
use super::cli::{run_capture, Captured, RunOpts};
use super::error::{AppError, Result};
use super::paths::to_posix;
use super::types::{LastCommit, SyncAction, SyncFileChange, SyncResult, SyncStatus};
use super::Core;
use std::time::Duration;

const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);
const NET_TIMEOUT: Duration = Duration::from_secs(180);

fn safe_name(value: &str, what: &str) -> Result<Option<String>> {
    if value.is_empty() {
        return Ok(None);
    }
    if !is_safe_git_name(value) {
        return Err(AppError::invalid(format!("{what} に使えない文字が含まれています: {value}")));
    }
    Ok(Some(value.to_string()))
}

/// Drop `user:password@` from a remote URL before it is shown in the UI.
pub fn redact_url(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else { return url.to_string() };
    let rest = &url[scheme_end + 3..];
    let auth_end = rest.find('/').unwrap_or(rest.len());
    match rest[..auth_end].rfind('@') {
        Some(at) => format!("{}{}", &url[..scheme_end + 3], &rest[at + 1..]),
        None => url.to_string(),
    }
}

fn is_missing_remote_ref(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("couldn't find remote ref") || m.contains("no such ref was fetched")
}

struct Porcelain {
    current: Option<String>,
    ahead: i64,
    behind: i64,
    files: Vec<(char, char, String)>,
}

fn parse_porcelain(out: &str) -> Porcelain {
    let mut p = Porcelain { current: None, ahead: 0, behind: 0, files: vec![] };
    let mut entries = out.split('\0');
    while let Some(e) = entries.next() {
        if e.is_empty() {
            continue;
        }
        if let Some(head) = e.strip_prefix("## ") {
            let (names, tracking) = match head.find(" [") {
                Some(i) => (&head[..i], Some(head[i + 2..].trim_end_matches(']'))),
                None => (head, None),
            };
            let name = names.strip_prefix("No commits yet on ").or_else(|| names.strip_prefix("Initial commit on ")).unwrap_or(names);
            let local = name.split("...").next().unwrap_or(name);
            p.current = Some(local.to_string());
            if let Some(t) = tracking {
                for part in t.split(',') {
                    let part = part.trim();
                    if let Some(n) = part.strip_prefix("ahead ") {
                        p.ahead = n.parse().unwrap_or(0);
                    } else if let Some(n) = part.strip_prefix("behind ") {
                        p.behind = n.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        let mut chars = e.chars();
        let (x, y) = (chars.next().unwrap_or(' '), chars.next().unwrap_or(' '));
        let path: String = e.chars().skip(3).collect();
        if x == 'R' || x == 'C' {
            entries.next(); // original path of a rename / copy
        }
        p.files.push((x, y, path));
    }
    p
}

impl Core {
    fn git_run(&self, args: &[&str], timeout: Duration) -> Result<Captured> {
        let root = self.export_root()?;
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        run_capture(
            "git",
            &args,
            &RunOpts {
                cwd: Some(root),
                env: vec![("GIT_TERMINAL_PROMPT".into(), "0".into()), ("LC_ALL".into(), "C".into()), ("GIT_OPTIONAL_LOCKS".into(), "0".into())],
                timeout,
                ..Default::default()
            },
        )
    }

    /// Run git and require exit code 0; the error message is git's own output.
    fn git_ok(&self, args: &[&str], timeout: Duration) -> std::result::Result<String, String> {
        match self.git_run(args, timeout) {
            Ok(o) if o.code == Some(0) => Ok(o.stdout_text()),
            Ok(o) => {
                let e = o.stderr_text();
                Err(if e.trim().is_empty() { o.stdout_text().trim().to_string() } else { e.trim().to_string() })
            }
            Err(e) => Err(e.message + &e.detail.map(|d| format!(" ({d})")).unwrap_or_default()),
        }
    }

    fn is_repo(&self) -> Result<bool> {
        let o = self.git_run(&["rev-parse", "--is-inside-work-tree"], LOCAL_TIMEOUT)?;
        Ok(o.code == Some(0) && o.stdout_text().trim() == "true")
    }

    /// The data dir relative to the export root (POSIX), as git sees it.
    fn data_rel_dir(&self) -> Result<String> {
        let abs = self.export_data_dir()?;
        let root = self.export_root()?;
        let rel = abs.strip_prefix(&root).map_err(|_| {
            AppError::invalid(format!("{} は {} の外にあります。git 同期は dataDir が書き出し先（リポジトリ）の中にある時だけ使えます", abs.display(), root.display()))
        })?;
        let s = to_posix(rel);
        Ok(if s.is_empty() { ".".into() } else { s })
    }

    fn data_status(&self, rel: &str) -> std::result::Result<Porcelain, String> {
        Ok(parse_porcelain(&self.git_ok(&["status", "--porcelain=v1", "-b", "-u", "-z", "--", rel], LOCAL_TIMEOUT)?))
    }

    pub fn sync_status(&self) -> Result<SyncStatus> {
        // unusable paths surface as errors (like the 409 of the REST version); everything else lands in `error`
        self.export_root()?;
        let r = (|| -> std::result::Result<SyncStatus, String> {
            if !self.is_repo().map_err(|e| e.message)? {
                return Ok(SyncStatus::default());
            }
            let settings = self.settings();
            let rel = self.data_rel_dir().map_err(|e| e.message)?;
            let st = self.data_status(&rel)?;
            let remotes_out = self.git_ok(&["remote", "-v"], LOCAL_TIMEOUT).unwrap_or_default();
            let remotes: Vec<(String, String)> = remotes_out
                .lines()
                .filter_map(|l| {
                    let (name, rest) = l.split_once('\t')?;
                    let url = rest.strip_suffix(" (fetch)")?;
                    Some((name.to_string(), url.to_string()))
                })
                .collect();
            let want = safe_name(&settings.git.remote, "リモート名").map_err(|e| e.message)?.unwrap_or_else(|| "origin".into());
            let remote = remotes.iter().find(|(n, _)| *n == want).or_else(|| remotes.first());
            let branch = safe_name(&settings.git.branch, "ブランチ名").map_err(|e| e.message)?.or(st.current.clone());
            let (mut ahead, mut behind) = (st.ahead, st.behind);
            if let (Some((rname, _)), Some(b)) = (remote, &branch) {
                if let Ok(counts) = self.git_ok(&["rev-list", "--left-right", "--count", &format!("{b}...{rname}/{b}")], LOCAL_TIMEOUT) {
                    let nums: Vec<i64> = counts.split_whitespace().filter_map(|n| n.parse().ok()).collect();
                    if nums.len() == 2 {
                        ahead = nums[0];
                        behind = nums[1];
                    }
                }
            }
            let last_commit = self.git_ok(&["log", "-1", "--format=%H%x1f%s%x1f%ai%x1f%an"], LOCAL_TIMEOUT).ok().and_then(|o| {
                let f: Vec<&str> = o.trim_end().split('\u{1f}').collect();
                (f.len() == 4 && !f[0].is_empty()).then(|| LastCommit { hash: f[0].chars().take(7).collect(), message: f[1].into(), date: f[2].into(), author: f[3].into() })
            });
            Ok(SyncStatus {
                is_repo: true,
                branch,
                remote: remote.map(|(n, _)| n.clone()),
                remote_url: remote.map(|(_, u)| redact_url(u)),
                ahead,
                behind,
                changes: st
                    .files
                    .iter()
                    .map(|(x, y, p)| {
                        let s = format!("{x}{y}").trim().to_string();
                        SyncFileChange { path: p.clone(), status: if s.is_empty() { "??".into() } else { s } }
                    })
                    .collect(),
                last_commit,
                error: None,
            })
        })();
        Ok(r.unwrap_or_else(|e| SyncStatus { error: Some(e), ..Default::default() }))
    }

    fn configure_author(&self) {
        let s = self.settings();
        if !s.git.author_name.is_empty() {
            let _ = self.git_ok(&["config", "--local", "user.name", &s.git.author_name], LOCAL_TIMEOUT);
        }
        if !s.git.author_email.is_empty() {
            let _ = self.git_ok(&["config", "--local", "user.email", &s.git.author_email], LOCAL_TIMEOUT);
        }
    }

    fn result(&self, ok: bool, action: SyncAction, message: impl Into<String>, status: Option<SyncStatus>) -> SyncResult {
        SyncResult { ok, action, message: message.into(), commit_hash: None, status }
    }

    /// Stage the data dir and commit. `ok: false` (not an error) when there is nothing to commit.
    pub fn commit_data(&self, message: Option<&str>) -> Result<SyncResult> {
        self.export_root()?;
        if !self.is_repo()? {
            return Ok(self.result(false, SyncAction::Commit, "Git 管理外です（Gitリポジトリではありません）", None));
        }
        let rel = self.data_rel_dir()?;
        self.configure_author();
        self.git_ok(&["add", "--", &rel], LOCAL_TIMEOUT).map_err(|e| AppError::new(super::error::ErrorCode::Git, "git add に失敗しました").with_detail(e))?;
        let st = self.data_status(&rel).map_err(|e| AppError::new(super::error::ErrorCode::Git, "git status に失敗しました").with_detail(e))?;
        let staged = st.files.iter().filter(|(x, _, _)| *x != ' ' && *x != '?').count();
        if staged == 0 {
            let status = self.sync_status()?;
            return Ok(self.result(false, SyncAction::Commit, "コミットする変更はありません", Some(status)));
        }
        let msg = message.map(str::trim).filter(|m| !m.is_empty()).map(str::to_string).unwrap_or_else(|| format!("data: update {staged} file(s) ({})", super::entities::now_iso()));
        match self.git_ok(&["commit", "-m", &msg], LOCAL_TIMEOUT) {
            Ok(out) => {
                let hash = out.lines().next().and_then(|l| l.strip_prefix('[')).and_then(|l| l.split(']').next()).and_then(|l| l.split_whitespace().last()).map(str::to_string);
                let hash = hash.or_else(|| self.git_ok(&["rev-parse", "--short", "HEAD"], LOCAL_TIMEOUT).ok().map(|s| s.trim().to_string()));
                let status = self.sync_status()?;
                let mut r = self.result(true, SyncAction::Commit, format!("コミットしました: {}", hash.clone().unwrap_or_default()), Some(status));
                r.commit_hash = hash;
                Ok(r)
            }
            Err(e) => Err(AppError::new(super::error::ErrorCode::Git, "コミットに失敗しました").with_detail(e)),
        }
    }

    pub fn pull_data(&self) -> Result<SyncResult> {
        self.export_root()?;
        let settings = self.settings();
        let st = self.sync_status()?;
        if !st.is_repo {
            return Ok(self.result(false, SyncAction::Pull, "Gitリポジトリではありません", None));
        }
        let Some(remote) = st.remote.clone() else {
            return Ok(self.result(false, SyncAction::Pull, "リモートが設定されていません", Some(st)));
        };
        let Some(branch) = safe_name(&settings.git.branch, "ブランチ名")?.or(st.branch.clone()) else {
            return Ok(self.result(false, SyncAction::Pull, "ブランチを特定できません", Some(st)));
        };
        // --autostash: files outside data/ may be modified (the app can live in the same repo); keep them out of the way.
        match self.git_ok(&["pull", "--rebase=true", "--autostash", &remote, &branch], NET_TIMEOUT) {
            Ok(_) => Ok(self.result(true, SyncAction::Pull, format!("{remote}/{branch} から取得しました"), Some(self.sync_status()?))),
            Err(m) if is_missing_remote_ref(&m) => {
                // First sync: the branch does not exist on the remote yet. Nothing to pull; push will create it.
                Ok(self.result(true, SyncAction::Pull, format!("リモートに {branch} はまだありません（push で作成されます）"), Some(st)))
            }
            Err(m) => {
                let _ = self.git_ok(&["rebase", "--abort"], LOCAL_TIMEOUT);
                Ok(self.result(false, SyncAction::Pull, format!("取得に失敗しました: {m}"), Some(self.sync_status()?)))
            }
        }
    }

    pub fn push_data(&self) -> Result<SyncResult> {
        self.export_root()?;
        let settings = self.settings();
        let st = self.sync_status()?;
        if !st.is_repo {
            return Ok(self.result(false, SyncAction::Push, "Gitリポジトリではありません", None));
        }
        let Some(remote) = st.remote.clone() else {
            return Ok(self.result(false, SyncAction::Push, "リモートが設定されていません", Some(st)));
        };
        let Some(branch) = safe_name(&settings.git.branch, "ブランチ名")?.or(st.branch.clone()) else {
            return Ok(self.result(false, SyncAction::Push, "ブランチを特定できません", Some(st)));
        };
        match self.git_ok(&["push", "--set-upstream", &remote, &branch], NET_TIMEOUT) {
            Ok(_) => Ok(self.result(true, SyncAction::Push, format!("{remote}/{branch} へ送信しました"), Some(self.sync_status()?))),
            Err(m) => Ok(self.result(false, SyncAction::Push, format!("送信に失敗しました: {m}"), Some(self.sync_status()?))),
        }
    }

    /// commit -> pull --rebase -> push
    pub fn sync_data(&self, message: Option<&str>) -> Result<SyncResult> {
        let c = self.commit_data(message)?;
        let p = self.pull_data()?;
        if !p.ok && p.status.as_ref().is_some_and(|s| s.remote.is_some()) {
            return Ok(SyncResult { action: SyncAction::Sync, ..p });
        }
        let u = self.push_data()?;
        let msg = [c.message.as_str(), p.message.as_str(), u.message.as_str()].into_iter().filter(|m| !m.is_empty()).collect::<Vec<_>>().join(" / ");
        Ok(SyncResult { ok: u.ok, action: SyncAction::Sync, message: msg, commit_hash: c.commit_hash, status: u.status })
    }

    /// Called after a record save when auto-commit is on. Never fails the caller.
    pub fn auto_commit_if_enabled(&self, message: &str) -> Option<SyncResult> {
        let s = self.settings();
        if !s.git.auto_commit {
            return None;
        }
        match self.commit_data(Some(message)) {
            Ok(c) if c.ok && s.git.auto_push => self.push_data().ok(),
            Ok(c) => Some(c),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::core_with_dataset;
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let o = Command::new("git").args(args).current_dir(dir).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_SYSTEM", "/dev/null").output().unwrap();
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    fn configure(core: &Core) {
        core.update_settings(&serde_json::json!({ "git": { "authorName": "Tester", "authorEmail": "t@example.com", "branch": "main" } })).unwrap();
    }

    #[test]
    fn redacts_credentials() {
        assert_eq!(redact_url("https://user:pw@github.com/a/b.git"), "https://github.com/a/b.git");
        assert_eq!(redact_url("https://github.com/a/b.git"), "https://github.com/a/b.git");
        assert_eq!(redact_url("git@github.com:a/b.git"), "git@github.com:a/b.git");
        assert_eq!(redact_url("ssh://git@host/x"), "ssh://host/x");
    }

    #[test]
    fn porcelain_parsing() {
        let p = parse_porcelain("## main...origin/main [ahead 2, behind 1]\0 M data/a.md\0?? data/b.md\0R  data/n.md\0data/o.md\0A  data/c.md\0");
        assert_eq!((p.current.as_deref(), p.ahead, p.behind), (Some("main"), 2, 1));
        let f: Vec<_> = p.files.iter().map(|(x, y, p)| format!("{x}{y}:{p}")).collect();
        assert_eq!(f, [" M:data/a.md", "??:data/b.md", "R :data/n.md", "A :data/c.md"]);
        assert_eq!(parse_porcelain("## No commits yet on main\0").current.as_deref(), Some("main"));
        assert_eq!(parse_porcelain("## Initial commit on dev\0").current.as_deref(), Some("dev"));
        assert_eq!(parse_porcelain("## feature\0").current.as_deref(), Some("feature"));
    }

    #[test]
    fn not_a_repo() {
        let (_t, core) = core_with_dataset();
        let st = core.sync_status().unwrap();
        assert!(!st.is_repo && st.error.is_none());
        let r = core.commit_data(None).unwrap();
        assert!(!r.ok && r.message.contains("Git 管理外"));
        assert!(!core.pull_data().unwrap().ok);
        assert!(!core.push_data().unwrap().ok);
    }

    #[test]
    fn commit_stages_only_the_data_dir() {
        let (_t, core) = core_with_dataset();
        configure(&core);
        let root = core.export_root().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        fs::create_dir_all(root.join("data/monks")).unwrap();
        fs::write(root.join("data/monks/a.md"), "---\nname: A\n---\n").unwrap();
        fs::write(root.join("outside.txt"), "not data").unwrap();

        let st = core.sync_status().unwrap();
        assert!(st.is_repo);
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.changes, [SyncFileChange { path: "data/monks/a.md".into(), status: "??".into() }], "files outside data/ are not listed");

        let r = core.commit_data(Some("data(monks): add A")).unwrap();
        assert!(r.ok, "{r:?}");
        assert!(r.commit_hash.as_deref().is_some_and(|h| h.len() >= 7));
        assert_eq!(git(&root, &["log", "-1", "--format=%s|%an|%ae"]).trim(), "data(monks): add A|Tester|t@example.com");
        assert!(git(&root, &["status", "--porcelain"]).contains("?? outside.txt"), "outside.txt stays untracked");
        assert!(core.sync_status().unwrap().changes.is_empty());
        assert_eq!(core.sync_status().unwrap().last_commit.unwrap().message, "data(monks): add A");

        let again = core.commit_data(None).unwrap();
        assert!(!again.ok && again.message == "コミットする変更はありません");

        // modify + delete are reported with porcelain codes
        fs::write(root.join("data/monks/a.md"), "---\nname: A2\n---\n").unwrap();
        fs::write(root.join("data/monks/b.md"), "x").unwrap();
        let st = core.sync_status().unwrap();
        let mut ch: Vec<_> = st.changes.iter().map(|c| format!("{}:{}", c.status, c.path)).collect();
        ch.sort();
        assert_eq!(ch, ["??:data/monks/b.md", "M:data/monks/a.md"]);
        let r = core.commit_data(None).unwrap();
        assert!(r.ok);
        assert!(git(&root, &["log", "-1", "--format=%s"]).starts_with("data: update 2 file(s)"));
    }

    #[test]
    fn sync_against_a_remote_creates_the_branch_then_round_trips() {
        let (t, core) = core_with_dataset();
        configure(&core);
        let remote = t.path().join("remote.git");
        git(t.path(), &["init", "-q", "--bare", "-b", "main", remote.to_str().unwrap()]);
        let root = core.export_root().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["remote", "add", "origin", remote.to_str().unwrap()]);
        fs::create_dir_all(root.join("data/temples")).unwrap();
        fs::write(root.join("data/temples/w.md"), "---\nname: W\n---\n").unwrap();

        // first sync: remote has no `main` yet -> pull is a no-op success, push creates it
        let r = core.sync_data(Some("first")).unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.action, SyncAction::Sync);
        assert!(r.message.contains("リモートに main はまだありません"), "{}", r.message);
        assert!(r.message.contains("origin/main へ送信しました"));
        assert_eq!(git(&remote, &["log", "-1", "--format=%s", "main"]).trim(), "first");
        let st = core.sync_status().unwrap();
        assert_eq!((st.ahead, st.behind), (0, 0));
        assert_eq!(st.remote.as_deref(), Some("origin"));
        assert!(st.remote_url.unwrap().ends_with("remote.git"));

        // someone else pushes; a local commit + sync rebases on top and pushes
        let other = t.path().join("other");
        git(t.path(), &["clone", "-q", remote.to_str().unwrap(), other.to_str().unwrap()]);
        git(&other, &["config", "user.name", "O"]);
        git(&other, &["config", "user.email", "o@example.com"]);
        fs::write(other.join("data/temples/o.md"), "---\nname: O\n---\n").unwrap();
        git(&other, &["add", "."]);
        git(&other, &["commit", "-q", "-m", "other"]);
        git(&other, &["push", "-q", "origin", "main"]);
        fs::write(root.join("data/temples/w2.md"), "---\nname: W2\n---\n").unwrap();
        let st = core.sync_status().unwrap();
        assert_eq!(st.behind, 0, "behind is only known after a fetch");
        let r = core.sync_data(None).unwrap();
        assert!(r.ok, "{r:?}");
        assert!(root.join("data/temples/o.md").is_file(), "remote change pulled");
        assert_eq!(git(&remote, &["rev-list", "--count", "main"]).trim(), "3");
        let st = core.sync_status().unwrap();
        assert_eq!((st.ahead, st.behind), (0, 0));
    }

    #[test]
    fn push_failure_is_reported_not_thrown() {
        let (_t, core) = core_with_dataset();
        configure(&core);
        let root = core.export_root().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["remote", "add", "origin", "/nonexistent/remote.git"]);
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/x.md"), "x").unwrap();
        core.commit_data(None).unwrap();
        let r = core.push_data().unwrap();
        assert!(!r.ok && r.message.starts_with("送信に失敗しました"), "{}", r.message);
        let r = core.pull_data().unwrap();
        assert!(!r.ok && r.message.starts_with("取得に失敗しました"), "{}", r.message);
    }

    #[test]
    fn no_remote_is_reported() {
        let (_t, core) = core_with_dataset();
        configure(&core);
        let root = core.export_root().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        assert_eq!(core.pull_data().unwrap().message, "リモートが設定されていません");
        assert_eq!(core.push_data().unwrap().message, "リモートが設定されていません");
    }

    #[test]
    fn unsafe_branch_name_is_refused() {
        let (_t, core) = core_with_dataset();
        let root = core.export_root().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        // bypass update_settings validation to simulate a hand-edited file
        fs::create_dir_all(&core.dirs.config_dir).unwrap();
        fs::write(core.dirs.config_dir.join("app-settings.json"), r#"{"git":{"branch":"--upload-pack=evil"}}"#).unwrap();
        let st = core.sync_status().unwrap();
        assert!(st.error.unwrap().contains("ブランチ名"));
    }

    #[test]
    fn auto_commit_follows_the_setting() {
        let (_t, core) = core_with_dataset();
        configure(&core);
        let root = core.export_root().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/a.md"), "a").unwrap();
        assert!(core.auto_commit_if_enabled("m").is_none(), "off by default");
        core.update_settings(&serde_json::json!({ "git": { "autoCommit": true } })).unwrap();
        let r = core.auto_commit_if_enabled("auto").unwrap();
        assert!(r.ok);
        assert_eq!(git(&root, &["log", "-1", "--format=%s"]).trim(), "auto");
    }

    #[test]
    fn data_dir_outside_the_repo_is_rejected() {
        let (_t, core) = core_with_dataset();
        let root = core.export_root().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        core.update_settings(&serde_json::json!({ "dataDir": "/somewhere/else" })).unwrap();
        // same as the TS version: git sync refuses a dataDir outside the repository
        let st = core.sync_status().unwrap();
        assert!(st.error.unwrap().contains("外にあります"));
    }
}
