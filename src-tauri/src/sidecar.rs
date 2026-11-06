//! Sidecar = the built Nitro server (`.output/server/index.mjs`) run by the `node` found on PATH.
//! It listens on 127.0.0.1 on a free port. The whole process group is killed on stop / drop.

use std::fs::File;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Origins of the Tauri WebView that the sidecar must accept cross-origin (see server/middleware/cors.ts).
pub const WEBVIEW_ORIGINS: &str = "tauri://localhost,http://tauri.localhost,https://tauri.localhost,http://localhost:1420";

#[derive(Debug, Clone)]
pub struct SidecarConfig {
    pub node: PathBuf,
    /// Path of index.mjs
    pub entry: PathBuf,
    pub dataset_path: String,
    pub export_path: String,
    pub state_dir: PathBuf,
    pub cwd: PathBuf,
    pub log_file: PathBuf,
    pub allowed_origins: String,
}

pub struct Sidecar {
    child: Child,
    pub port: u16,
}

/// Find `node`: PATH first, then the usual Homebrew / system locations (GUI apps on macOS get a minimal PATH).
pub fn find_node() -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { "node.exe" } else { "node" };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(extra));
    }
    find_in(&dirs, exe).ok_or_else(|| "node が見つかりません。Node.js 22 以上をインストールしてください（macOS: brew install node）".to_string())
}

fn find_in(dirs: &[PathBuf], exe: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(exe)).find(|c| c.is_file())
}

pub fn free_port() -> std::io::Result<u16> {
    let l = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(l.local_addr()?.port())
}

impl Sidecar {
    /// Spawn the server and wait (up to `timeout`) until it accepts connections.
    pub fn start(cfg: &SidecarConfig, timeout: Duration) -> Result<Sidecar, String> {
        if !cfg.entry.is_file() {
            return Err(format!("サーバーが見つかりません: {}（npm run build が必要です）", cfg.entry.display()));
        }
        let port = free_port().map_err(|e| format!("空きポートを確保できません: {e}"))?;
        std::fs::create_dir_all(&cfg.state_dir).map_err(|e| format!("状態フォルダを作れません: {e}"))?;
        std::fs::create_dir_all(&cfg.cwd).map_err(|e| format!("作業フォルダを作れません: {e}"))?;
        if let Some(parent) = cfg.log_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let log = File::create(&cfg.log_file).map_err(|e| format!("ログを作れません: {e}"))?;
        let log_err = log.try_clone().map_err(|e| e.to_string())?;

        let mut cmd = Command::new(&cfg.node);
        cmd.arg(&cfg.entry)
            .current_dir(&cfg.cwd)
            .env("NITRO_HOST", "127.0.0.1")
            .env("HOST", "127.0.0.1")
            .env("NITRO_PORT", port.to_string())
            .env("PORT", port.to_string())
            .env("PHRAPEDIA_DATASET_PATH", &cfg.dataset_path)
            .env("PHRAPEDIA_EXPORT_PATH", &cfg.export_path)
            .env("PHRAPEDIA_ALLOWED_ORIGINS", &cfg.allowed_origins)
            .env("PHRAPEDIA_PARENT_PID", std::process::id().to_string())
            .env("NUXT_STATE_DIR", &cfg.state_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err));
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0); // own group: stop() can kill node and anything it spawned
        }
        let child = cmd.spawn().map_err(|e| format!("node を起動できません ({}): {e}", cfg.node.display()))?;
        let mut sc = Sidecar { child, port };
        if let Err(e) = sc.wait_ready(timeout, &cfg.log_file) {
            sc.stop();
            return Err(e);
        }
        Ok(sc)
    }

    fn wait_ready(&mut self, timeout: Duration, log_file: &Path) -> Result<(), String> {
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(format!("サーバーが起動直後に終了しました ({status})\n{}", log_tail(log_file)));
            }
            if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("サーバーの起動がタイムアウトしました\n{}", log_tail(log_file)));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[cfg(test)]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Whether the node process is still alive.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// SIGTERM the process group, escalate to SIGKILL after a grace period, then reap. Safe to call twice.
    pub fn stop(&mut self) {
        #[cfg(unix)]
        {
            let pgid = self.child.id() as libc::pid_t;
            // SAFETY: plain signal delivery to the process group we created in start().
            unsafe { libc::killpg(pgid, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if matches!(self.child.try_wait(), Ok(Some(_))) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            // SIGKILL also reaches children that outlived node
            unsafe { libc::killpg(pgid, libc::SIGKILL) };
            let _ = self.child.wait();
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        self.stop();
    }
}

fn log_tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(8)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> Option<PathBuf> {
        find_node().ok()
    }

    fn cfg(dir: &Path, script: &str) -> SidecarConfig {
        let entry = dir.join("index.mjs");
        std::fs::write(&entry, script).unwrap();
        SidecarConfig {
            node: find_node().unwrap(),
            entry,
            dataset_path: dir.to_string_lossy().into(),
            export_path: dir.to_string_lossy().into(),
            state_dir: dir.join("state"),
            cwd: dir.to_path_buf(),
            log_file: dir.join("logs/sidecar.log"),
            allowed_origins: WEBVIEW_ORIGINS.into(),
        }
    }

    /// Server that also spawns a long-lived grandchild; prints env so tests can check what was passed.
    const SERVER: &str = r#"
import http from 'node:http'
import { spawn } from 'node:child_process'
const child = spawn('sleep', ['300'], { stdio: 'ignore' })
console.log('GRANDCHILD', child.pid)
http.createServer((req, res) => res.end(JSON.stringify({
  dataset: process.env.PHRAPEDIA_DATASET_PATH, exportPath: process.env.PHRAPEDIA_EXPORT_PATH,
  host: process.env.NITRO_HOST, ppid: process.env.PHRAPEDIA_PARENT_PID,
}))).listen(Number(process.env.NITRO_PORT), process.env.NITRO_HOST)
"#;

    #[cfg(unix)]
    fn alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[cfg(unix)]
    #[test]
    fn start_serves_and_stop_kills_whole_group() {
        if node().is_none() {
            eprintln!("node not found; skipping");
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let c = cfg(d.path(), SERVER);
        let mut sc = Sidecar::start(&c, Duration::from_secs(15)).expect("start");
        let port = sc.port;
        let pid = sc.pid() as i32;
        // env reached the server
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        use std::io::{Read, Write};
        s.write_all(b"GET / HTTP/1.0\r\n\r\n").unwrap();
        let mut body = String::new();
        s.read_to_string(&mut body).unwrap();
        assert!(body.contains(&format!("\"dataset\":{}", serde_json::to_string(&c.dataset_path).unwrap())), "{body}");
        assert!(body.contains("\"host\":\"127.0.0.1\""));
        // grandchild pid from the log
        let log = std::fs::read_to_string(&c.log_file).unwrap();
        let gc: i32 = log.lines().find_map(|l| l.strip_prefix("GRANDCHILD ")).unwrap().trim().parse().unwrap();
        assert!(alive(pid) && alive(gc));

        sc.stop();
        std::thread::sleep(Duration::from_millis(200));
        assert!(!alive(pid), "node still alive");
        assert!(!alive(gc), "grandchild still alive (process group not killed)");
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err(), "port still open");
        sc.stop(); // idempotent
    }

    #[cfg(unix)]
    #[test]
    fn drop_stops_the_process() {
        if node().is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let c = cfg(d.path(), SERVER);
        let sc = Sidecar::start(&c, Duration::from_secs(15)).unwrap();
        let pid = sc.pid() as i32;
        drop(sc);
        assert!(!alive(pid));
    }

    #[test]
    fn early_exit_is_reported_with_log_tail() {
        if node().is_none() {
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let c = cfg(d.path(), "console.error('boom-xyz'); process.exit(3)");
        let err = Sidecar::start(&c, Duration::from_secs(15)).err().expect("must fail");
        assert!(err.contains("終了"), "{err}");
        assert!(err.contains("boom-xyz"), "{err}");
    }

    #[test]
    fn missing_entry_is_an_error_not_a_panic() {
        let d = tempfile::tempdir().unwrap();
        let c = SidecarConfig {
            node: PathBuf::from("node"),
            entry: d.path().join("nope.mjs"),
            dataset_path: String::new(),
            export_path: String::new(),
            state_dir: d.path().join("s"),
            cwd: d.path().to_path_buf(),
            log_file: d.path().join("l.log"),
            allowed_origins: String::new(),
        };
        assert!(Sidecar::start(&c, Duration::from_secs(1)).err().unwrap().contains("見つかりません"));
    }

    #[test]
    fn find_in_picks_first_existing() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a");
        let b = d.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("node"), "").unwrap();
        assert_eq!(find_in(&[a, b.clone()], "node"), Some(b.join("node")));
        assert_eq!(find_in(&[], "node"), None);
    }

    #[test]
    fn free_port_is_usable() {
        let p = free_port().unwrap();
        assert!(TcpListener::bind(("127.0.0.1", p)).is_ok());
    }
}
