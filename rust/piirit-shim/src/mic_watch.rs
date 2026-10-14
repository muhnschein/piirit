//! Whether this process has a recording stream open in `PulseAudio`.
//!
//! A call's audio route cannot be changed while the browser engine's
//! microphone stream is open. Gecko creates that stream with
//! `PA_STREAM_DONT_MOVE`, and Sailfish's policy module asserts that every
//! stream moved when the route does: the sound server aborts, and the
//! phone has no sound until systemd has restarted it. Gecko runs inside
//! this process, so its streams carry this process's id, and the app
//! changes the route only when it knows that no recording stream (a
//! source output, in `PulseAudio`'s words) carries it.
//!
//! "Knows" is the point of this module. A wrong "closed" costs the phone
//! its sound, so the answer has two parts: `known`, and `open`, which
//! means something only while `known` is true. Known is connected, and
//! subscribed to source-output events, and a full listing of the source
//! outputs taken since the last event. Anything else -- not connected
//! yet, the server gone, a listing under way after a stream came or went
//! while the answer was "closed" -- is not known, and reads as such.
//! "Open" going stale is not withdrawn the same way: an open that turns
//! out to be closed only keeps the route where it is for a moment longer.
//!
//! The work is libpulse's, on a `pa_threaded_mainloop` of its own: the Qt
//! thread never waits on the sound server, and nothing polls. An event
//! about any source output -- new, gone, changed -- is answered with a
//! fresh listing; a listing asked for while one is under way is taken
//! again once it is done, and only the last is believed. When the server
//! goes away, which on a phone it does, the answer is withdrawn at once
//! and a connection tried again after a second, then every two, until
//! one is made and listed. A daemon is never started from here
//! (`PA_CONTEXT_NOAUTOSPAWN`): on a phone systemd owns it, and anywhere
//! else one started by this process would be the wrong one.
//!
//! It is C++, in the `cpp!` block below, for the structs: libpulse's are
//! read through its own headers rather than written out again in Rust,
//! and the block is kept to plain calls that can be checked by reading,
//! one small function per thing libpulse can say. The one way
//! back into Rust is [`MicState`] handed to a callback on the mainloop's
//! thread, under its lock; [`MicWatch`] is what QML sees, and takes that
//! over to the Qt thread.

// `cpp!` expands to a call across the FFI boundary, which is `unsafe` by
// construction. Scoped to this file: the workspace denies it everywhere
// else, and docs/BUILDING.md says why this one is allowed.
#![allow(unsafe_code)]

use std::ffi::{c_void, CString};

use cpp::cpp;
use qmetaobject::*;

cpp! {{
    #include <errno.h>
    #include <stdlib.h>
    #include <new>
    #include <string>
    #include <pulse/pulseaudio.h>

    // One watcher. Every field is the mainloop thread's, read and written
    // under the mainloop's lock: from its own callbacks, which libpulse
    // runs holding it, and from start and stop, which take it.
    struct PiiritMicWatch {
        pa_threaded_mainloop *loop = nullptr;
        pa_mainloop_api *api = nullptr;
        pa_context *context = nullptr;
        // The next connection attempt, while one is waited for.
        pa_time_event *retry = nullptr;
        // Empty for libpulse's own lookup.
        std::string server;
        uint32_t pid = 0;
        // What the Rust callback is handed back.
        void *user = nullptr;
        // Attempts since the last connection, for the wait before the next.
        unsigned failures = 0;
        // The server took the subscription.
        bool subscribed = false;
        // A listing is under way, and whether anything changed since it
        // was asked for, and whether it has seen the pid so far.
        bool listing = false;
        bool again = false;
        bool found = false;
        // What was last reported.
        bool known = false;
        bool open = false;
    };

    static void piirit_mic_list(PiiritMicWatch *w);

    // Hand a state to Rust, when it is not the one last handed over.
    static void piirit_mic_report(PiiritMicWatch *w, bool known, bool open) {
        open = known && open;
        if (w->known == known && w->open == open) {
            return;
        }
        w->known = known;
        w->open = open;
        void *user = w->user;
        rust!(PiiritMicWatch_report [user: *const c_void as "void *", known: bool as "bool", open: bool as "bool"] {
            // SAFETY: `user` is the `Report` the `MicWatcher` boxed and
            // keeps until after the mainloop thread -- the only thread
            // this runs on -- has been stopped and joined.
            let report = unsafe { &*user.cast::<Report>() };
            let state = MicState { known, open };
            // A panic must not unwind into libpulse.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| report(state))).is_err() {
                eprintln!("piirit: the microphone watch's callback panicked");
            }
        });
    }

    // Whether a stream's properties name `pid` as its process. libpulse
    // writes the id in decimal; anything else is not this process.
    static bool piirit_mic_belongs(const pa_proplist *props, uint32_t pid) {
        const char *value = pa_proplist_gets(props, PA_PROP_APPLICATION_PROCESS_ID);
        if (!value || *value < '0' || *value > '9') {
            return false;
        }
        char *end = nullptr;
        errno = 0;
        unsigned long id = strtoul(value, &end, 10);
        return errno == 0 && *end == '\0' && id == pid;
    }

    // Let go of the context, quietly: none of its callbacks run again.
    static void piirit_mic_drop(PiiritMicWatch *w) {
        pa_context *c = w->context;
        if (!c) {
            return;
        }
        w->context = nullptr;
        w->subscribed = false;
        w->listing = false;
        w->again = false;
        w->found = false;
        pa_context_set_state_callback(c, nullptr, nullptr);
        pa_context_set_subscribe_callback(c, nullptr, nullptr);
        pa_context_disconnect(c);
        pa_context_unref(c);
    }

    static void piirit_mic_connect(PiiritMicWatch *w);

    static void piirit_mic_retried(pa_mainloop_api *api, pa_time_event *e, const struct timeval *, void *userdata) {
        PiiritMicWatch *w = static_cast<PiiritMicWatch *>(userdata);
        api->time_free(e);
        w->retry = nullptr;
        piirit_mic_connect(w);
    }

    // Try again in a second, or two after that: soon enough after
    // systemd has restarted the server, and no busy loop when it has not.
    static void piirit_mic_retry(PiiritMicWatch *w) {
        if (w->retry) {
            return;
        }
        struct timeval when;
        pa_gettimeofday(&when);
        pa_timeval_add(&when, w->failures == 0 ? PA_USEC_PER_SEC : 2 * PA_USEC_PER_SEC);
        if (w->failures < 2) {
            w->failures++;
        }
        w->retry = w->api->time_new(w->api, &when, piirit_mic_retried, w);
    }

    // The connection is gone, or cannot be trusted: say so, and start over.
    static void piirit_mic_lost(PiiritMicWatch *w) {
        piirit_mic_drop(w);
        piirit_mic_report(w, false, false);
        piirit_mic_retry(w);
    }

    static void piirit_mic_subscribed(pa_context *c, int success, void *userdata) {
        PiiritMicWatch *w = static_cast<PiiritMicWatch *>(userdata);
        if (c != w->context) {
            return;
        }
        if (!success) {
            // No events, so no listing could be believed for long.
            piirit_mic_lost(w);
            return;
        }
        w->subscribed = true;
    }

    static void piirit_mic_output(pa_context *c, const pa_source_output_info *info, int eol, void *userdata) {
        PiiritMicWatch *w = static_cast<PiiritMicWatch *>(userdata);
        if (c != w->context) {
            return;
        }
        if (eol < 0) {
            piirit_mic_lost(w);
            return;
        }
        if (eol == 0) {
            if (info && piirit_mic_belongs(info->proplist, w->pid)) {
                w->found = true;
            }
            return;
        }
        w->listing = false;
        if (w->again) {
            piirit_mic_list(w);
            return;
        }
        // The server answers in order, so the subscription's answer came
        // before this one; not having had it is not a state to trust.
        if (!w->subscribed) {
            piirit_mic_lost(w);
            return;
        }
        piirit_mic_report(w, true, w->found);
    }

    // List every source output, or ask for it again once the listing
    // under way is done.
    static void piirit_mic_list(PiiritMicWatch *w) {
        if (w->listing) {
            w->again = true;
            return;
        }
        w->listing = true;
        w->again = false;
        w->found = false;
        pa_operation *op = pa_context_get_source_output_info_list(w->context, piirit_mic_output, w);
        if (!op) {
            piirit_mic_lost(w);
            return;
        }
        pa_operation_unref(op);
    }


    static void piirit_mic_event(pa_context *c, pa_subscription_event_type_t type, uint32_t, void *userdata) {
        PiiritMicWatch *w = static_cast<PiiritMicWatch *>(userdata);
        if (c != w->context) {
            return;
        }
        if ((type & PA_SUBSCRIPTION_EVENT_FACILITY_MASK) != PA_SUBSCRIPTION_EVENT_SOURCE_OUTPUT) {
            return;
        }
        // A "closed" is no longer known to be true until the listing says
        // so again; an "open" can wait for it.
        if (!w->open) {
            piirit_mic_report(w, false, false);
        }
        piirit_mic_list(w);
    }

    static void piirit_mic_state(pa_context *c, void *userdata) {
        PiiritMicWatch *w = static_cast<PiiritMicWatch *>(userdata);
        if (c != w->context) {
            return;
        }
        switch (pa_context_get_state(c)) {
        case PA_CONTEXT_READY: {
            w->failures = 0;
            // Subscribed first, so that nothing that changes between the
            // listing and the subscription goes unheard.
            pa_operation *op = pa_context_subscribe(c, PA_SUBSCRIPTION_MASK_SOURCE_OUTPUT, piirit_mic_subscribed, w);
            if (!op) {
                piirit_mic_lost(w);
                return;
            }
            pa_operation_unref(op);
            piirit_mic_list(w);
            break;
        }
        case PA_CONTEXT_FAILED:
        case PA_CONTEXT_TERMINATED:
            piirit_mic_lost(w);
            break;
        default:
            break;
        }
    }

    static void piirit_mic_connect(PiiritMicWatch *w) {
        pa_context *c = pa_context_new(w->api, "piirit");
        if (!c) {
            piirit_mic_retry(w);
            return;
        }
        w->context = c;
        pa_context_set_state_callback(c, piirit_mic_state, w);
        pa_context_set_subscribe_callback(c, piirit_mic_event, w);
        const char *server = w->server.empty() ? nullptr : w->server.c_str();
        // A failure here may already have been through the state
        // callback, which let the context go; only one that did not is
        // let go here.
        if (pa_context_connect(c, server, PA_CONTEXT_NOAUTOSPAWN, nullptr) < 0 && w->context == c) {
            piirit_mic_lost(w);
        }
    }

    static PiiritMicWatch *piirit_mic_start(uint32_t pid, const char *server, void *user) {
        PiiritMicWatch *w = new (std::nothrow) PiiritMicWatch();
        if (!w) {
            return nullptr;
        }
        w->loop = pa_threaded_mainloop_new();
        if (!w->loop) {
            delete w;
            return nullptr;
        }
        w->api = pa_threaded_mainloop_get_api(w->loop);
        w->pid = pid;
        w->user = user;
        if (server) {
            w->server = server;
        }
        // Before the thread exists, so nothing else holds the lock yet.
        piirit_mic_connect(w);
        if (pa_threaded_mainloop_start(w->loop) < 0) {
            piirit_mic_drop(w);
            if (w->retry) {
                w->api->time_free(w->retry);
            }
            pa_threaded_mainloop_free(w->loop);
            delete w;
            return nullptr;
        }
        return w;
    }

    // Never from the mainloop's own thread, which this joins.
    static void piirit_mic_stop(PiiritMicWatch *w) {
        pa_threaded_mainloop_lock(w->loop);
        piirit_mic_drop(w);
        if (w->retry) {
            w->api->time_free(w->retry);
            w->retry = nullptr;
        }
        pa_threaded_mainloop_unlock(w->loop);
        pa_threaded_mainloop_stop(w->loop);
        pa_threaded_mainloop_free(w->loop);
        delete w;
    }
}}

/// What a watcher knows: whether it knows, and whether a recording stream
/// of the watched process is open. `open` is false whenever `known` is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MicState {
    /// Connected, subscribed, and listed since the last change.
    pub known: bool,
    /// A source output carries the watched process's id.
    pub open: bool,
}

/// The callback a watcher reports to, boxed once more so that C++ holds a
/// thin pointer to it.
type Report = Box<dyn Fn(MicState) + Send>;

/// Watches `PulseAudio` for recording streams of one process, on a thread
/// of its own, until dropped.
///
/// Every change of [`MicState`] is handed to the callback, on that
/// thread, in order; the state before the first is unknown. The callback
/// must not block and must not drop the watcher. Dropping it stops and
/// joins the thread, after which the callback is not called again.
pub struct MicWatcher {
    /// The C++ watcher; never null.
    handle: usize,
    /// What C++ calls back, at the address it was given, and read only
    /// through that. Dropped after the handle has been stopped.
    #[allow(dead_code)]
    report: Box<Report>,
}

impl MicWatcher {
    /// Start watching for recording streams whose `application.process.id`
    /// is `pid`, on `server` (a `PulseAudio` server string, such as
    /// `unix:/run/user/100000/pulse/native`), or wherever libpulse's own
    /// lookup finds one when `None`. `None` only when the mainloop could
    /// not be made, or `server` has a nul in it.
    pub fn start(
        pid: u32,
        server: Option<&str>,
        report: impl Fn(MicState) + Send + 'static,
    ) -> Option<Self> {
        let server = match server {
            Some(server) => Some(CString::new(server).ok()?),
            None => None,
        };
        let server_ptr = server
            .as_ref()
            .map_or(std::ptr::null(), |name| name.as_ptr());
        let report: Box<Report> = Box::new(Box::new(report));
        let user = std::ptr::addr_of!(*report).cast::<c_void>();
        // SAFETY: `server_ptr` is null or a nul-terminated string that
        // outlives the call, and is copied; `user` points at the `Report`
        // this watcher keeps for as long as the C++ one lives.
        let handle = cpp!(unsafe [pid as "uint32_t", server_ptr as "const char *",
                                  user as "void *"] -> *mut c_void as "void *" {
            return piirit_mic_start(pid, server_ptr, user);
        });
        if handle.is_null() {
            return None;
        }
        Some(Self {
            handle: handle as usize,
            report,
        })
    }
}

impl Drop for MicWatcher {
    fn drop(&mut self) {
        let watch = self.handle as *mut c_void;
        // SAFETY: the handle came from `piirit_mic_start` and is stopped
        // exactly once, here, from a thread that is not the mainloop's:
        // the callback, the only code that runs there, is told not to
        // drop the watcher. Once this returns the thread is joined, and
        // `report`, dropped after this body, can go.
        cpp!(unsafe [watch as "PiiritMicWatch *"] {
            piirit_mic_stop(watch);
        });
    }
}

/// This process's recording streams, as QML sees them.
///
/// ```qml
/// MicWatch { id: mic; active: call.state === "active" }
/// // The route may change only while it is known that nothing records.
/// property bool routeFree: mic.known && !mic.open
/// ```
#[derive(QObject, Default)]
pub struct MicWatch {
    base: qt_base_class!(trait QObject),

    /// Watch while true; while false nothing is connected and nothing is
    /// known.
    pub active: qt_property!(bool; WRITE set_active NOTIFY active_changed),
    /// Emitted when [`Self::active`] changes.
    pub active_changed: qt_signal!(),
    /// Whether [`Self::open`] can be believed: connected to the sound
    /// server, and listed since the last change. Read-only: qmetaobject
    /// makes every property writable that is not `CONST`, and a constant
    /// one is never re-read, so a write from QML is taken and dropped.
    pub known: qt_property!(bool; READ is_known WRITE refuse NOTIFY known_changed),
    /// Emitted when [`Self::known`] changes.
    pub known_changed: qt_signal!(),
    /// Whether this process has a recording stream open. False while not
    /// known, which does not mean closed. Read-only, as `known` is.
    pub open: qt_property!(bool; READ is_open WRITE refuse NOTIFY open_changed),
    /// Emitted when [`Self::open`] changes.
    pub open_changed: qt_signal!(),

    /// The watcher, while active.
    watcher: Option<MicWatcher>,
    /// Counts activations, so that a report from a watcher since stopped
    /// is not taken for one from the current one.
    generation: u64,
}

impl MicWatch {
    /// Whether [`Self::open`] can be believed.
    #[must_use]
    pub fn is_known(&self) -> bool {
        self.known
    }

    /// Whether a recording stream of this process is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// What a write to `known` or `open` comes to: nothing. Only the
    /// watch says what it knows.
    pub fn refuse(&mut self, _value: bool) {}

    /// Start or stop watching.
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.generation = self.generation.wrapping_add(1);
        // Stopped first, either way: a watcher is joined before the next
        // one starts, and what it knew goes with it.
        self.watcher = None;
        self.take(MicState::default());
        if active {
            let generation = self.generation;
            let ptr: QPointer<Self> = QPointer::from(&*self);
            let done = queued_callback(move |state: MicState| {
                let Some(this) = ptr.as_pinned() else { return };
                if this.borrow().generation != generation {
                    return;
                }
                this.borrow_mut().take(state);
            });
            self.watcher = MicWatcher::start(std::process::id(), None, done);
            if self.watcher.is_none() {
                eprintln!("piirit: could not start watching the microphone");
            }
        }
        self.active_changed();
    }

    /// Take a state from the watcher. Both properties are set before
    /// either is announced, so that a binding over the two never sees
    /// `known` from one state and `open` from the other: known and not
    /// yet open, on the way to open, is exactly the wrong answer.
    fn take(&mut self, state: MicState) {
        let known = state.known;
        let open = state.known && state.open;
        let known_changed = self.known != known;
        let open_changed = self.open != open;
        self.known = known;
        self.open = open;
        if known_changed {
            self.known_changed();
        }
        if open_changed {
            self.open_changed();
        }
    }
}
