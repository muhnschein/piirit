//! `MicWatch` in QML: watch on own process, answers sent to Qt thread. Pages bind
//! `known && !open`; followed on every change as own streams come and go in private
//! sound server. Free never true with stream open, even between two property changes: `known`
//! and `open` change together or not at all, as far as a binding can tell.

// Qt harness: needs `unsafe` for `env::set_var` pre-Qt and libpulse server lookup
// (`unused_unsafe`: edition 2024+), `borrow_as_ptr`, `single_shot` whole secs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use qmetaobject::*;

#[path = "common/pulse.rs"]
mod pulse;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property bool free: mic.known && !mic.open
        property string frees: ''
        onFreeChanged: frees += (free ? 'free ' : 'held ')
        MicWatch { id: mic }
        // As a page will have it: active from the start.
        MicWatch { id: declared; active: true }
        function declaredState() { return (declared.known ? 'known' : 'unknown') + '|' + (declared.open ? 'open' : 'closed') }
        function activate(on) { mic.active = on; return 'ok' }
        function state() { return (mic.known ? 'known' : 'unknown') + '|' + (mic.open ? 'open' : 'closed') }
        function changes() { return frees.trim() }
        function forge() {
            try { mic.known = true; mic.open = true } catch (e) { return 'refused' }
            return state()
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_route_is_free_only_while_it_is_known_that_nothing_of_ours_records() {
    if !pulse::tools_present() {
        return;
    }
    let scratch = pulse::Scratch::new();
    let daemon = pulse::Daemon::start(&scratch.dir);
    // SAFETY: nothing else runs yet; set pre-Qt and pre-libpulse lookup.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PULSE_SERVER", daemon.server());
    }
    piirit_shim::register_qml_types();
    let mut engine = QmlEngine::new();
    engine.load_data(QByteArray::from(PROBE_QML));
    let engine_ptr = std::ptr::addr_of_mut!(engine);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    let steps: Rc<RefCell<Vec<(&str, String)>>> = Rc::default();
    let daemon = Rc::new(daemon);
    let recording: Rc<RefCell<Option<pulse::Recording>>> = Rc::default();

    let s = Rc::clone(&steps);
    single_shot(Duration::from_secs(1), move || unsafe {
        s.borrow_mut().push(("inactive", call!("state")));
        call!("activate", true);
    });

    let (s, d, r) = (Rc::clone(&steps), Rc::clone(&daemon), Rc::clone(&recording));
    single_shot(Duration::from_secs(2), move || unsafe {
        s.borrow_mut().push(("active", call!("state")));
        s.borrow_mut().push(("declared", call!("declaredState")));
        *r.borrow_mut() = Some(d.record(Some(std::process::id())));
    });

    let (s, r) = (Rc::clone(&steps), Rc::clone(&recording));
    single_shot(Duration::from_secs(4), move || unsafe {
        s.borrow_mut().push(("recording", call!("state")));
        if let Some(recording) = r.borrow_mut().take() {
            recording.kill();
        }
    });

    let (s, d, r) = (Rc::clone(&steps), Rc::clone(&daemon), Rc::clone(&recording));
    single_shot(Duration::from_secs(6), move || unsafe {
        s.borrow_mut().push(("stopped", call!("state")));
        s.borrow_mut().push(("forged-open", call!("forge")));
        *r.borrow_mut() = Some(d.record(Some(std::process::id())));
    });

    // Stale report from stopped watcher must not count. Stream's end held behind busy
    // Qt thread during restart: taken, says "free" with stream open.
    let (s, d, r) = (Rc::clone(&steps), Rc::clone(&daemon), Rc::clone(&recording));
    single_shot(Duration::from_secs(8), move || unsafe {
        s.borrow_mut().push(("recording-again", call!("state")));
        if let Some(recording) = r.borrow_mut().take() {
            recording.kill();
        }
        std::thread::sleep(Duration::from_millis(500));
        call!("activate", false);
        s.borrow_mut().push(("deactivated", call!("state")));
        s.borrow_mut().push(("forged-known", call!("forge")));
        *r.borrow_mut() = Some(d.record(Some(std::process::id())));
        std::thread::sleep(Duration::from_millis(500));
        call!("activate", true);
    });

    let (s, r) = (Rc::clone(&steps), Rc::clone(&recording));
    single_shot(Duration::from_secs(10), move || unsafe {
        s.borrow_mut().push(("reactivated", call!("state")));
        s.borrow_mut().push(("changes", call!("changes")));
        drop(r.borrow_mut().take());
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map_or_else(|| "<step did not run>".to_string(), |(_, v)| v.clone())
    };
    let log = daemon.log();
    assert_eq!(value("inactive"), "unknown|closed", "{log}");
    assert_eq!(
        value("active"),
        "known|closed",
        "an active watch on a server with nothing recording did not come to \
         know it: {log}"
    );
    assert_eq!(
        value("declared"),
        "known|closed",
        "a watch declared active did not start"
    );
    assert_eq!(
        value("recording"),
        "known|open",
        "a recording stream of this process was not seen from QML"
    );
    assert_eq!(value("stopped"), "known|closed", "its end was not seen");
    assert_eq!(
        value("forged-open"),
        "known|closed",
        "QML could write `open`, which only the watch may"
    );
    assert_eq!(
        value("recording-again"),
        "known|open",
        "the second stream was not seen"
    );
    assert_eq!(
        value("deactivated"),
        "unknown|closed",
        "an answer outlived the watch that gave it"
    );
    assert_eq!(
        value("forged-known"),
        "unknown|closed",
        "QML could write `known`, which only the watch may"
    );
    assert_eq!(
        value("reactivated"),
        "known|open",
        "the restarted watch did not see the stream open when it started"
    );
    // Free once known; held from first event to its end; free; held from second through
    // restart to end. Any extra: "free" with our stream open.
    assert_eq!(
        value("changes"),
        "free held free held",
        "the route was free while a stream of ours was open, if only \
         between two property changes or from a stopped watcher's report"
    );
}
