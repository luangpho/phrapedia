//! External CLI helpers (claude / agy / gemini / git): command resolution for GUI launches (minimal PATH) and a
//! blocking capture runner with timeout.

use super::error::{AppError, ErrorCode, Result};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Directories that usually hold user-installed CLIs but are missing from a Finder / Dock launch PATH.
pub fn fixed_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/opt/homebrew/sbin"), PathBuf::from("/usr/local/bin")];
    if let Some(h) = home {
        v.push(h.join(".local/bin"));
        v.push(h.join(".claude/local"));
        v.push(h.join(".npm-global/bin"));
        v.push(h.join(".cargo/bin"));
        v.push(h.join(".bun/bin"));
    }
    v
}

/// PATH of the user's login shell (`$SHELL -lc`), cached for the process lifetime. None when it cannot be read.
pub fn login_shell_path() -> Option<String> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            if cfg!(windows) {
                return None;
            }
            let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/sh".into());
            // Must not go through child_path(): that would re-enter this OnceLock and deadlock.
            let inherited = std::env::var_os("PATH").unwrap_or_default();
            let out = run_capture_with_path(
                &shell,
                &["-lc".into(), "printf '__PH_PATH__%s__PH_END__' \"$PATH\"".into()],
                &RunOpts { timeout: Duration::from_secs(4), ..Default::default() },
                PathBuf::from(&shell),
                &inherited,
            )
            .ok()?;
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let start = text.find("__PH_PATH__")? + "__PH_PATH__".len();
            let end = text[start..].find("__PH_END__")? + start;
            let p = text[start..end].trim().to_string();
            (!p.is_empty()).then_some(p)
        })
        .clone()
}

/// Inherited PATH entries first, then the login-shell ones, then the fixed candidates (each directory once).
pub fn merged_path(inherited: Option<OsString>, login: Option<&str>, home: Option<&Path>) -> OsString {
    let mut dirs: Vec<PathBuf> = vec![];
    let mut push = |p: PathBuf| {
        if !p.as_os_str().is_empty() && !dirs.contains(&p) {
            dirs.push(p);
        }
    };
    if let Some(inh) = &inherited {
        for p in std::env::split_paths(inh) {
            push(p);
        }
    }
    if let Some(l) = login {
        for p in std::env::split_paths(l) {
            push(p);
        }
    }
    for p in fixed_candidates(home) {
        push(p);
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| inherited.unwrap_or_default())
}

/// PATH to give to child processes.
pub fn child_path() -> OsString {
    merged_path(std::env::var_os("PATH"), login_shell_path().as_deref(), home_dir().as_deref())
}

fn find_in(dirs: &[PathBuf], exe: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(exe)).find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// A bare command name is looked up in `child_path()`; anything with a path separator is used as given.
/// Falls back to the bare name (spawn then reports "not found").
pub fn resolve_command(command: &str) -> PathBuf {
    let c = command.trim();
    if c.contains('/') || c.contains('\\') {
        return PathBuf::from(c);
    }
    let dirs: Vec<PathBuf> = std::env::split_paths(&child_path()).collect();
    find_in(&dirs, c).unwrap_or_else(|| PathBuf::from(c))
}

#[derive(Default)]
pub struct RunOpts {
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    pub stdin: Option<String>,
    pub timeout: Duration,
}

#[derive(Debug)]
pub struct Captured {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: Option<i32>,
}

impl Captured {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Spawn `command` (resolved through `resolve_command`), feed `stdin`, collect output. Kills the process group on timeout.
pub fn run_capture(command: &str, args: &[String], opts: &RunOpts) -> Result<Captured> {
    run_capture_with_path(command, args, opts, resolve_command(command), &child_path())
}

fn run_capture_with_path(command: &str, args: &[String], opts: &RunOpts, exe: PathBuf, path: &std::ffi::OsStr) -> Result<Captured> {
    let mut cmd = Command::new(&exe);
    cmd.args(args);
    cmd.env("PATH", path);
    for (k, v) in &opts.env {
        cmd.env(k, v);
    }
    for k in &opts.env_remove {
        cmd.env_remove(k);
    }
    if let Some(cwd) = &opts.cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdin(if opts.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| {
        AppError::new(ErrorCode::CliNotFound, format!("「{command}」を起動できません"))
            .with_detail(format!("{e}（{}）", exe.display()))
    })?;
    if let (Some(text), Some(mut stdin)) = (opts.stdin.clone(), child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut b = vec![];
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut b);
        }
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = vec![];
        if let Some(o) = err.as_mut() {
            let _ = o.read_to_end(&mut b);
        }
        b
    });
    let deadline = Instant::now() + opts.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(e) => return Err(AppError::io("プロセスの状態を取得できません", e)),
        }
        if opts.timeout > Duration::ZERO && Instant::now() >= deadline {
            kill_group(&mut child);
            let _ = child.wait();
            let _ = t_out.join();
            let _ = t_err.join();
            return Err(AppError::new(ErrorCode::Timeout, format!("{command} が {} ms 以内に終了しませんでした", opts.timeout.as_millis())));
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    Ok(Captured { stdout, stderr, code: status.and_then(|s| s.code()) })
}

/// SIGTERM the process group, SIGKILL after a short grace period.
pub fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        // SAFETY: plain signal syscalls on the process group we created with process_group(0).
        unsafe {
            libc::killpg(pid, libc::SIGTERM);
        }
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        unsafe {
            libc::killpg(pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merged_path_keeps_order_and_dedups() {
        let inherited = std::env::join_paths(["/usr/bin", "/bin", "/usr/local/bin"]).unwrap();
        let p = merged_path(Some(inherited), Some("/custom/bin:/usr/bin"), Some(Path::new("/home/u")));
        let dirs: Vec<_> = std::env::split_paths(&p).collect();
        assert_eq!(&dirs[..4], [PathBuf::from("/usr/bin"), "/bin".into(), "/usr/local/bin".into(), "/custom/bin".into()]);
        assert_eq!(dirs.iter().filter(|d| d.as_path() == std::path::Path::new("/usr/bin")).count(), 1);
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(dirs.contains(&PathBuf::from("/home/u/.local/bin")));
        assert!(dirs.contains(&PathBuf::from("/home/u/.claude/local")));
    }

    #[test]
    fn merged_path_works_without_inherited_path() {
        let p = merged_path(None, None, None);
        assert!(std::env::split_paths(&p).any(|d| d.as_path() == std::path::Path::new("/opt/homebrew/bin")));
    }

    #[test]
    fn resolve_keeps_explicit_paths_and_finds_sh() {
        assert_eq!(resolve_command("/opt/x/claude"), PathBuf::from("/opt/x/claude"));
        #[cfg(unix)]
        assert!(resolve_command("sh").is_absolute());
        assert_eq!(resolve_command("definitely-not-a-command-xyz"), PathBuf::from("definitely-not-a-command-xyz"));
    }

    #[cfg(unix)]
    #[test]
    fn find_in_picks_first_executable() {
        use std::os::unix::fs::PermissionsExt;
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::write(a.path().join("tool"), "x").unwrap(); // not executable
        std::fs::write(b.path().join("tool"), "x").unwrap();
        std::fs::set_permissions(b.path().join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let got = find_in(&[a.path().into(), b.path().into()], "tool").unwrap();
        assert_eq!(got, b.path().join("tool"));
    }

    #[cfg(unix)]
    #[test]
    fn capture_stdout_stdin_and_exit_code() {
        let o = run_capture("sh", &["-c".into(), "cat; echo err >&2; exit 3".into()], &RunOpts { stdin: Some("hello".into()), timeout: Duration::from_secs(5), ..Default::default() }).unwrap();
        assert_eq!(o.stdout_text(), "hello");
        assert_eq!(o.stderr_text().trim(), "err");
        assert_eq!(o.code, Some(3));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_and_reports() {
        let t = Instant::now();
        let e = run_capture("sh", &["-c".into(), "sleep 30".into()], &RunOpts { timeout: Duration::from_millis(200), ..Default::default() }).unwrap_err();
        assert_eq!(e.code, ErrorCode::Timeout);
        assert!(t.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn missing_command_is_cli_not_found() {
        let e = run_capture("definitely-not-a-command-xyz", &[], &RunOpts { timeout: Duration::from_secs(2), ..Default::default() }).unwrap_err();
        assert_eq!(e.code, ErrorCode::CliNotFound);
    }
}
