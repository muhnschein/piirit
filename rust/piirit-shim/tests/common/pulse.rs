//! A `PulseAudio` daemon of a test's own, and recording streams in it.
//!
//! The daemon listens on a socket in a directory of its own, takes anyone
//! who connects (`auth-anonymous`), and has one null sink, whose monitor
//! is a source `parec` can record from. Nothing of the machine's own
//! sound is touched: no default script, no D-Bus, no shared memory.

// Not every test uses every helper; and a helper that cannot set the
// daemon up fails the test that asked, saying what it could not do,
// which clippy allows of a test function but not of one it calls.
#![allow(dead_code, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Whether `pulseaudio` and `parec` are there to test with. Missing is a
/// skip on a developer's machine and a failure under CI, which must
/// never pass by not running.
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

/// A directory of the test's own, short enough for a socket path (108
/// bytes on Linux), removed on drop.
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

/// A running daemon. Killed on drop.
pub struct Daemon {
    dir: PathBuf,
    child: Option<Child>,
}

impl Daemon {
    /// Start a daemon in `dir`, and wait until its socket takes
    /// connections.
    pub fn start(dir: &Path) -> Self {
        let mut daemon = Self {
            dir: dir.to_path_buf(),
            child: None,
        };
        daemon.restart();
        daemon
    }

    /// The server string clients reach it by.
    pub fn server(&self) -> String {
        format!("unix:{}", self.socket().display())
    }

    fn socket(&self) -> PathBuf {
        self.dir.join("native")
    }

    /// Kill it the way a crash would: no goodbye to its clients beyond
    /// the socket closing.
    pub fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.kill());
            drop(child.wait());
        }
    }

    /// Start it again, on the same socket, as systemd would after a
    /// crash.
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

    /// What the daemon said, for a failure message.
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("daemon.log")).unwrap_or_default()
    }

    /// Record from the null sink's monitor, as the process `pid` when one
    /// is given -- `PULSE_PROP` sets the client's properties, which its
    /// streams carry -- or as `parec` itself.
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

/// A `parec` recording. Killed on drop.
pub struct Recording {
    child: Child,
}

impl Recording {
    /// The recording process's own id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Stop recording, as a crash would.
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
