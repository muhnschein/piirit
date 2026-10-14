//! Test's own `PulseAudio` daemon and recording streams.
//!
//! Own socket dir, `auth-anonymous`, one null sink; machine's sound untouched.

// Not all tests use all helpers. Setup failure fails asking test (clippy: `expect_used` only in
// test fns).
#![allow(dead_code, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Whether `pulseaudio` and `parec` exist. Missing: skip locally, fail on CI.
pub fn tools_present() -> bool {
    let missing: Vec<&str> = ["pulseaudio", "parec"]
        .into_iter()
        .filter(|tool| {
            Command::new(tool)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_err()
        })
        .collect();
    if missing.is_empty() {
        return true;
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "{} not installed, so the microphone watch cannot be tested against \
         a real sound server; install pulseaudio and pulseaudio-utils",
        missing.join(" and ")
    );
    eprintln!(
        "skipping: {} not found (install pulseaudio and pulseaudio-utils)",
        missing.join(" and ")
    );
    false
}

/// Own dir, short for socket path (108 bytes on Linux). Removed on drop.
pub struct Scratch {
    pub dir: PathBuf,
}

impl Scratch {
    pub fn new() -> Self {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "piirit-pa-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("scratch directory");
        Self { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

/// Running daemon. Killed on drop.
pub struct Daemon {
    dir: PathBuf,
    child: Option<Child>,
}

impl Daemon {
    /// Start daemon in `dir`; wait for socket.
    pub fn start(dir: &Path) -> Self {
        let mut daemon = Self {
            dir: dir.to_path_buf(),
            child: None,
        };
        daemon.restart();
        daemon
    }

    /// Server string clients connect to.
    pub fn server(&self) -> String {
        format!("unix:{}", self.socket().display())
    }

    fn socket(&self) -> PathBuf {
        self.dir.join("native")
    }

    /// Kill like a crash: clients see socket close only.
    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.kill());
            drop(child.wait());
        }
    }

    /// Start again on same socket, as systemd does after crash.
    pub fn restart(&mut self) {
        self.kill();
        let runtime = self.dir.join("runtime");
        let home = self.dir.join("home");
        for dir in [&runtime, &home] {
            std::fs::create_dir_all(dir).expect("daemon directory");
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("daemon.log"))
            .expect("daemon log");
        let child = Command::new("pulseaudio")
            .args([
                "-n",
                "--daemonize=no",
                "--exit-idle-time=-1",
                "--disable-shm=yes",
                "--use-pid-file=no",
                "--no-cpu-limit",
                "--realtime=no",
                "--high-priority=no",
                "--log-target=stderr",
                "-L",
            ])
            .arg(format!(
                "module-native-protocol-unix socket={} auth-anonymous=1",
                self.socket().display()
            ))
            .args(["-L", "module-null-sink sink_name=null"])
            .env("HOME", &home)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("PULSE_RUNTIME_PATH", &runtime)
            .env("PULSE_STATE_PATH", self.dir.join("state"))
            .env_remove("PULSE_SERVER")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("DISPLAY")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("pulseaudio would not start");
        self.child = Some(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while std::os::unix::net::UnixStream::connect(self.socket()).is_err() {
            if let Some(child) = self.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    panic!("pulseaudio exited ({status}): {}", self.log());
                }
            }
            assert!(
                Instant::now() < deadline,
                "pulseaudio did not open its socket: {}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Daemon's log, for failure messages.
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("daemon.log")).unwrap_or_default()
    }

    /// Record from null sink monitor, as process `pid` if given (`PULSE_PROP` sets client
    /// props), else as `parec`.
    pub fn record(&self, pid: Option<u32>) -> Recording {
        let mut command = Command::new("parec");
        command
            .arg(format!("--server={}", self.server()))
            .arg("--device=null.monitor")
            .env("HOME", self.dir.join("home"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(pid) = pid {
            command.env("PULSE_PROP", format!("application.process.id=\"{pid}\""));
        }
        Recording {
            child: command.spawn().expect("parec would not start"),
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.kill();
    }
}

/// `parec` recording. Killed on drop.
pub struct Recording {
    child: Child,
}

impl Recording {
    /// Recording process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Stop recording, like a crash.
    pub fn kill(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.stop();
    }
}
