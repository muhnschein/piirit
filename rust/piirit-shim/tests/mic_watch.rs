//! The microphone watch against a real sound server.
//!
//! The app changes a call's audio route only while the watch knows that
//! no recording stream of its own is open, because moving the browser
//! engine's takes `PulseAudio` down with it. So what is pinned here is
//! mostly about "known": that it comes only from a full listing, that it
//! goes the moment the server does and comes back with it, and that a
//! stream coming or going while the answer is "closed" withdraws the
//! answer until it has been listed again. And, of course, that "open"
//! follows a stream of the watched process and of no other.
//!
//! The daemon is a private one (`common/pulse.rs`); the streams are
//! `parec`'s, made to carry this test's process id where a stream of its
//! own is wanted, which is what a stream Gecko opens in the app's process
//! carries.

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use piirit_shim::{MicState, MicWatcher};

#[path = "common/pulse.rs"]
mod pulse;

/// Long enough for anything that is going to happen to have happened.
const WAIT: Duration = Duration::from_secs(5);
/// A reconnection waits a second or two before it tries.
const RECONNECT: Duration = Duration::from_secs(10);

const KNOWN_CLOSED: MicState = MicState {
    known: true,
    open: false,
};
const KNOWN_OPEN: MicState = MicState {
    known: true,
    open: true,
};
const UNKNOWN: MicState = MicState {
    known: false,
    open: false,
};

/// A watcher, and every state it has reported, in order.
struct Watch {
    name: &'static str,
    _watcher: MicWatcher,
    states: Receiver<MicState>,
    seen: Vec<MicState>,
}

impl Watch {
    fn start(name: &'static str, pid: u32, server: Option<&str>) -> Self {
        let (tx, states) = channel();
        let Some(watcher) = MicWatcher::start(pid, server, move |state| {
            tx.send(state).ok();
        }) else {
            panic!("the watcher did not start");
        };
        Self {
            name,
            _watcher: watcher,
            states,
            seen: Vec::new(),
        }
    }

    /// Where the next `wait_since` starts looking.
    fn mark(&self) -> usize {
        self.seen.len()
    }

    /// The state now: the last one reported, or unknown before any.
    fn now(&mut self) -> MicState {
        while let Ok(state) = self.states.try_recv() {
            self.seen.push(state);
        }
        self.seen.last().copied().unwrap_or_default()
    }

    /// Wait until the state is `want`.
    fn wait(&mut self, want: MicState, within: Duration, why: &str) {
        if self.now() == want {
            return;
        }
        let mark = self.mark();
        self.wait_since(mark, want, within, why);
    }

    /// Wait until `want` has been reported since `mark`, and say where.
    fn wait_since(&mut self, mark: usize, want: MicState, within: Duration, why: &str) -> usize {
        let deadline = Instant::now() + within;
        loop {
            if let Some(at) = self.seen.iter().skip(mark).position(|state| *state == want) {
                return mark + at;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.states.recv_timeout(left) {
                Ok(state) => self.seen.push(state),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => panic!(
                    "{}: no {want:?} within {within:?}: {why}\nreported: {:?}",
                    self.name, self.seen
                ),
            }
        }
    }

    /// Everything reported since `mark`.
    fn since(&mut self, mark: usize) -> Vec<MicState> {
        self.now();
        self.seen.iter().skip(mark).copied().collect()
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_recording_of_this_process_is_followed_through_the_server_going_and_coming_back() {
    if !pulse::tools_present() {
        return;
    }
    let scratch = pulse::Scratch::new();
    let mut daemon = pulse::Daemon::start(&scratch.dir);
    let server = daemon.server();
    let me = std::process::id();

    let mut ours = Watch::start("ours", me, Some(&server));
    let mut other = Watch::start("other", me.wrapping_add(1), Some(&server));
    ours.wait(
        KNOWN_CLOSED,
        WAIT,
        "a server with nothing recording was not known to have nothing open",
    );
    other.wait(KNOWN_CLOSED, WAIT, "as for ours");
    assert_eq!(
        ours.seen,
        [KNOWN_CLOSED],
        "something other than the first listing's answer was reported first"
    );

    // A stream of this process's: open for ours, and for the other a
    // "closed" withdrawn until the listing after the event has said so
    // again -- and still closed.
    let other_mark = other.mark();
    let recording = daemon.record(Some(me));
    ours.wait(
        KNOWN_OPEN,
        WAIT,
        "a recording stream of the watched process was not seen: the new \
         stream's event was not answered with a listing, or the listing did \
         not match it on its process id",
    );
    let withdrawn = other.wait_since(
        other_mark,
        UNKNOWN,
        WAIT,
        "a stream appearing while the answer was \"closed\" left the old \
         answer standing until the listing came back",
    );
    other.wait_since(
        withdrawn + 1,
        KNOWN_CLOSED,
        WAIT,
        "the listing after the event did not settle the answer again",
    );
    assert!(
        !other.since(other_mark).contains(&KNOWN_OPEN),
        "another process's stream was taken for the watched one's: {:?}",
        other.seen
    );

    // Gone again: closed.
    recording.kill();
    ours.wait(
        KNOWN_CLOSED,
        WAIT,
        "the stream going away was not seen, so the route would stay held \
         for the rest of the call",
    );

    // A stream that carries its own process's id, not one it was given:
    // the id is read off the stream, whoever made it.
    let ours_mark = ours.mark();
    let theirs = daemon.record(None);
    let mut parec = Watch::start("parec", theirs.pid(), Some(&server));
    parec.wait(
        KNOWN_OPEN,
        WAIT,
        "parec's own stream was not matched to parec's process id",
    );
    let withdrawn = ours.wait_since(ours_mark, UNKNOWN, WAIT, "as for the other");
    ours.wait_since(withdrawn + 1, KNOWN_CLOSED, WAIT, "as for the other");
    assert!(
        !ours.since(ours_mark).contains(&KNOWN_OPEN),
        "another process's stream was taken for this one's: {:?}",
        ours.seen
    );
    theirs.kill();
    parec.wait(KNOWN_CLOSED, WAIT, "parec's stream going was not seen");
    drop(parec);

    // The server dies. Not known, at once -- not "closed", which is what a
    // dead server has and the reason the route must not move.
    daemon.kill();
    ours.wait(
        UNKNOWN,
        WAIT,
        "the server going away left an answer standing",
    );

    // Back, as systemd brings it: known again, from a fresh listing, and
    // subscribed again, so a new stream is seen.
    let mark = ours.mark();
    daemon.restart();
    ours.wait_since(
        mark,
        KNOWN_CLOSED,
        RECONNECT,
        "the watch did not connect again once the server was back",
    );
    let recording = daemon.record(Some(me));
    ours.wait(
        KNOWN_OPEN,
        WAIT,
        "after reconnecting, a new stream went unheard: the subscription \
         was not made again",
    );
    drop(recording);
    ours.wait(KNOWN_CLOSED, WAIT, "the stream going was not seen");

    // A stream that was open when the server went is not open in the
    // server that replaced it, and the answer comes from the new listing.
    let recording = daemon.record(Some(me));
    ours.wait(KNOWN_OPEN, WAIT, "the stream was not seen");
    let mark = ours.mark();
    daemon.kill();
    drop(recording);
    daemon.restart();
    ours.wait_since(
        mark,
        KNOWN_CLOSED,
        RECONNECT,
        "the answer after reconnecting was not the new server's",
    );
    assert_eq!(
        ours.since(mark).first(),
        Some(&UNKNOWN),
        "the answer was not withdrawn while the server was gone"
    );
}

#[test]
fn a_server_that_is_not_there_is_not_known() {
    let scratch = pulse::Scratch::new();
    let server = format!("unix:{}", scratch.dir.join("native").display());
    let mut watch = Watch::start("nobody", std::process::id(), Some(&server));
    // Long enough for two attempts to reconnect, a second apart.
    std::thread::sleep(Duration::from_millis(3500));
    assert_eq!(
        watch.now(),
        UNKNOWN,
        "a server nobody runs was reported on: {:?}",
        watch.seen
    );
    assert!(
        watch.seen.is_empty(),
        "unknown was not the state from the start: {:?}",
        watch.seen
    );
}

#[test]
fn starting_and_stopping_at_any_moment_is_safe() {
    if !pulse::tools_present() {
        return;
    }
    let scratch = pulse::Scratch::new();
    let mut daemon = pulse::Daemon::start(&scratch.dir);
    let server = daemon.server();
    let me = std::process::id();
    let recording = daemon.record(Some(me));
    // Dropped at once, mid-connection, mid-listing, and once answered.
    for round in 0..40_u64 {
        let watch = Watch::start("churn", me, Some(&server));
        std::thread::sleep(Duration::from_micros(round * 250));
        drop(watch);
    }
    let mut watch = Watch::start("settled", me, Some(&server));
    watch.wait(KNOWN_OPEN, WAIT, "the stream was not seen");
    assert_eq!(
        watch.seen,
        [KNOWN_OPEN],
        "an answer was given before the listing that would have seen the \
         stream already open"
    );
    drop(watch);
    // And while waiting to reconnect, with nothing there to reconnect to.
    daemon.kill();
    let mut watch = Watch::start("waiting", me, Some(&server));
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(watch.now(), UNKNOWN);
    drop(watch);
    drop(recording);
}

/// Set for the child below, to the directory it looks for a server in.
const CHILD: &str = "PIIRIT_MIC_WATCH_CHILD";

#[test]
fn no_daemon_is_ever_started_from_here() {
    // The watch, looking for a server where libpulse's own lookup would,
    // finds none. libpulse would start one there unless told not to --
    // except as root, where it never does, so this proves something only
    // when run as another user, as CI's runner is. It runs in a child,
    // with an environment pointing that lookup at an empty directory and
    // autospawn turned on.
    if let Some(dir) = std::env::var_os(CHILD) {
        let mut watch = Watch::start("lookup", std::process::id(), None);
        std::thread::sleep(Duration::from_millis(3500));
        let started = Path::new(&dir).join("runtime").join("native").exists();
        assert!(
            !started && watch.now() == UNKNOWN,
            "a server was started for the watch (socket there: {started}; \
             reported {:?})",
            watch.seen
        );
        return;
    }
    if !pulse::tools_present() {
        return;
    }
    let scratch = pulse::Scratch::new();
    let runtime = scratch.dir.join("runtime");
    let home = scratch.dir.join("home");
    for dir in [&runtime, &home] {
        std::fs::create_dir_all(dir).expect("child directory");
    }
    let client_conf = scratch.dir.join("client.conf");
    std::fs::write(&client_conf, "autospawn = yes\n").expect("client.conf");
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "no_daemon_is_ever_started_from_here",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD, &scratch.dir)
        .env("HOME", &home)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("PULSE_RUNTIME_PATH", &runtime)
        .env("PULSE_CLIENTCONFIG", &client_conf)
        .env_remove("PULSE_SERVER")
        .env_remove("DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .output()
        .expect("the child would not run");
    // Whatever it started goes too, failed or not.
    if let Ok(pid) = std::fs::read_to_string(runtime.join("pid")) {
        drop(std::process::Command::new("kill").arg(pid.trim()).status());
    }
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
