//! `PIIRIT_MEMORY_LOG=<seconds>`: stderr line, resident size of app and core.
//!
//! Phone has no heaptrack/valgrind. Shows which grows: app (images) or core (DB cache).

use std::time::Duration;

/// Start if env is whole seconds. Daemon thread: dies with process.
pub fn start_from_env() {
    let Some(every) = std::env::var("PIIRIT_MEMORY_LOG")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|seconds| *seconds > 0)
    else {
        return;
    };
    let result = std::thread::Builder::new()
        .name("memory-log".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(every));
            eprintln!("{}", report());
        });
    if result.is_err() {
        eprintln!("memory: could not start the log thread");
    }
}

/// One line: app and core resident size.
fn report() -> String {
    let app = resident_mib("self").map_or_else(|| "?".to_string(), |mib| format!("{mib} MiB"));
    let core = piirit_shim::server_pid()
        .and_then(|pid| resident_mib(&pid.to_string()))
        .map_or_else(|| "not running".to_string(), |mib| format!("{mib} MiB"));
    format!("memory: app {app} resident, core {core}")
}

/// `VmRSS` of `/proc/<pid>/status`, MiB.
fn resident_mib(pid: &str) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    resident_kib(&status).map(|kib| kib / 1024)
}

/// `VmRSS:` line of status, KiB.
fn resident_kib(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|kib| kib.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resident_size_is_read_off_the_status_file() {
        let status =
            "Name:\tharbour-piirit\nVmPeak:\t  912000 kB\nVmRSS:\t  262144 kB\nThreads:\t9\n";
        assert_eq!(resident_kib(status), Some(262_144));
        assert_eq!(
            resident_kib("Name:\tx\nThreads:\t1\n"),
            None,
            "a status without VmRSS (a zombie's) must not read as zero"
        );
    }

    #[test]
    fn this_process_has_a_resident_size() {
        assert!(
            resident_mib("self").is_some(),
            "/proc/self/status is unreadable, so the log would say nothing"
        );
    }
}
