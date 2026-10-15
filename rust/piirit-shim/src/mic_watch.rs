//! Recording stream open in `PulseAudio` for this process.
//!
//! Call audio route must not change while Gecko's mic stream (`PA_STREAM_DONT_MOVE`) is open:
//! Sailfish policy module asserts all streams moved, sound server aborts, silent till systemd
//! restarts it. Gecko runs in-process: its streams carry this pid.
//!
//! Wrong "closed" costs sound, so `known` + `open`. Unknown: not connected, server gone, or
//! relisting after change while "closed". Stale "open" only delays route change: not withdrawn.
//!
//! libpulse on own `pa_threaded_mainloop`: Qt thread never waits; no polling. Any source-output
//! event: fresh listing; one asked mid-listing reruns after; only last believed. Server gone:
//! unknown at once; retry 1 s, then every 2 s. Never autospawn (`PA_CONTEXT_NOAUTOSPAWN`):
//! systemd owns daemon; elsewhere ours would be wrong one.
//!
//! C++ in `cpp!` so libpulse structs come from its headers; one small fn per libpulse callback.
//! [`MicState`] goes to a callback on mainloop thread, under lock; [`MicWatch`] moves it to Qt.

// `cpp!` is FFI: `unsafe`. Allowed here only: docs/BUILDING.md.
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

    // Mainloop-thread fields, under its lock.
    struct PiiritMicWatch {
        pa_threaded_mainloop *loop = nullptr;
        pa_mainloop_api *api = nullptr;
        pa_context *context = nullptr;
        // Next connection attempt.
        pa_time_event *retry = nullptr;
        // Empty: libpulse's own lookup.
        std::string server;
        uint32_t pid = 0;
        // Passed to the Rust callback.
        void *user = nullptr;
        // Failed attempts: sets wait.
        unsigned failures = 0;
        // Server accepted subscription.
        bool subscribed = false;
        // Listing under way; changed since asked; pid seen.
        bool listing = false;
        bool again = false;
        bool found = false;
        // Last reported.
        bool known = false;
        bool open = false;
    };

    static void piirit_mic_list(PiiritMicWatch *w);

    // Hand state to Rust if changed.
    static void piirit_mic_report(PiiritMicWatch *w, bool known, bool open) {
        open = known && open;
        if (w->known == known && w->open == open) {
            return;
        }
        w->known = known;
        w->open = open;
        void *user = w->user;
        rust!(PiiritMicWatch_report [user: *const c_void as "void *", known: bool as "bool", open: bool as "bool"] {
            // SAFETY: `user` lives until this mainloop thread is joined.
            let report = unsafe { &*user.cast::<Report>() };
            let state = MicState { known, open };
            // No unwind into libpulse.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| report(state))).is_err() {
                eprintln!("piirit: the microphone watch's callback panicked");
            }
        });
    }

    // Pid decimal; else not us.
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

    // Drop context quietly: no callbacks.
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

    // Soon after restart, no busy loop.
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

    // Gone or untrusted: report, restart.
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
            // No events: no listing trusted for long.
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
        // Server answers in order; no subscription answer yet: untrusted.
        if (!w->subscribed) {
            piirit_mic_lost(w);
            return;
        }
        piirit_mic_report(w, true, w->found);
    }

    // List every source output; re-ask if one is running.
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
        // "Closed" unknown until listed; "open" can wait.
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
            // Subscribe first: nothing changing in between is missed.
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
        // State callback may have dropped context already; drop if not.
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
        // Thread not started yet: no lock held.
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

    // Not from mainloop thread, which this joins.
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

/// Known, and whether this process records; `open` false while not `known`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MicState {
    /// Connected, subscribed, listed since last change.
    pub known: bool,
    /// A source output carries the watched pid.
    pub open: bool,
}

/// Callback; boxed twice for thin C++ pointer.
type Report = Box<dyn Fn(MicState) + Send>;

/// Watches `PulseAudio` for one process's recording streams.
///
/// [`MicState`] changes reach callback on watcher thread, in order, unknown until first.
/// Callback must not block or drop watcher. Drop joins thread; no callbacks after.
pub struct MicWatcher {
    /// C++ watcher; never null.
    handle: usize,
    /// Read by C++ at its address; dropped after handle.
    #[allow(dead_code)]
    report: Box<Report>,
}

impl MicWatcher {
    /// Watch streams whose `application.process.id` is `pid`.
    ///
    /// `server`: `PulseAudio` string (`unix:/run/user/100000/pulse/native`); `None`: libpulse
    /// lookup. `None` if mainloop fails or `server` has nul.
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
        // SAFETY: `server_ptr` null or nul-terminated, copied. `user` kept alive.
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
        // SAFETY: stopped once, here, off mainloop thread; joined before `report` drops.
        cpp!(unsafe [watch as "PiiritMicWatch *"] {
            piirit_mic_stop(watch);
        });
    }
}

/// This process's recording streams, for QML.
///
/// ```qml
/// MicWatch { id: mic; active: call.state === "active" }
/// // Route may change only while known that nothing records.
/// property bool routeFree: mic.known && !mic.open
/// ```
#[derive(QObject, Default)]
pub struct MicWatch {
    base: qt_base_class!(trait QObject),

    /// Watch while true; false: nothing connected or known.
    pub active: qt_property!(bool; WRITE set_active NOTIFY active_changed),
    /// [`Self::active`] changed.
    pub active_changed: qt_signal!(),
    /// [`Self::open`] believable. Read-only: qmetaobject makes non-`CONST` props writable and
    /// never re-reads `CONST`, so writes dropped.
    pub known: qt_property!(bool; READ is_known WRITE refuse NOTIFY known_changed),
    /// [`Self::known`] changed.
    pub known_changed: qt_signal!(),
    /// Recording stream open. False while not known, which isn't closed. Read-only.
    pub open: qt_property!(bool; READ is_open WRITE refuse NOTIFY open_changed),
    /// [`Self::open`] changed.
    pub open_changed: qt_signal!(),

    /// Watcher while active.
    watcher: Option<MicWatcher>,
    /// Activation count: late reports ignored.
    generation: u64,
}

impl MicWatch {
    /// Whether [`Self::open`] is believable.
    #[must_use]
    pub fn is_known(&self) -> bool {
        self.known
    }

    /// Recording stream open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Ignores writes to `known`/`open`.
    pub fn refuse(&mut self, _value: bool) {}

    /// Start or stop watching.
    pub fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.generation = self.generation.wrapping_add(1);
        // Stop first: join old watcher before next.
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

    /// Both set before either announced: bindings never see known-but-not-yet-open.
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
