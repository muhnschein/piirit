//! `PIIRIT_MEMORY_LOG=<seconds>`: a line on stderr every so often with
//! the resident size of this process and of the core it spawned.
//!
//! Profiling a Qt app on a phone has no good tools -- no `heaptrack`, no
//! `valgrind`, and `top` shows one number for two processes that grow
//! for different reasons. This is the cheap thing that answers the first
//! question, which is *which one*: the app, where every decoded image is
//! a texture, or the core, where the database cache lives. Read from
//! `/proc`, which is what `ps` reads; the same two numbers are the ones
//! `smem` and `/proc/<pid>/smaps_rollup` refine.

use std::time::Duration;

/// Start logging if the variable is set to a whole number of seconds;
/// do nothing otherwise. The thread is a daemon: it holds nothing and
/// dies with the process.
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

/// One line: the app's resident size, and the core's while it runs.
fn report() -> String {
    let app = resident_mib("self").map_or_else(|| "?".to_string(), |mib| format!("{mib} MiB"));
    let core = piirit_shim::server_pid()
        .and_then(|pid| resident_mib(&pid.to_string()))
        .map_or_else(|| "not running".to_string(), |mib| format!("{mib} MiB"));
    format!("memory: app {app} resident, core {core}")
}

/// `VmRSS` of `/proc/<pid>/status`, in whole MiB.
fn resident_mib(pid: &str) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    resident_kib(&status).map(|kib| kib / 1024)
}

/// The `VmRSS:` line of a `/proc/<pid>/status`, in KiB.
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
