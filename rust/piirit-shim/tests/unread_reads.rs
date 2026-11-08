//! Unread count read once per burst of events, not per event.
//!
//! Each event that could move count (message, chat's messages, chat list, chat row)
//! asks for `get_fresh_msgs`. Core keeps one read out per profile: asks during one
//! are noted; one more read after it lands. However many asked, read after all, so
//! count ends where last event left it.
//!
//! Fake core holds first read till burst reached Qt thread: all of it comes while read is out.

// Qt harness: see download_limit.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::{json, Value};

mod common;

/// `get_fresh_msgs` calls so far, by params.
fn reads(journal: &std::path::Path) -> Vec<Value> {
    common::calls(journal)
        .into_iter()
        .filter(|(method, _)| method == "get_fresh_msgs")
        .map(|(_, params)| params)
        .collect()
}

/// Counts burst's last kind as relayed to QML.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property int tails: 0
        Connections {
            target: core
            onCore_event: if (kind === 'ChatlistItemChanged') tails = tails + 1
        }
        function tail() { return '' + tails }
    }
";

#[test]
fn a_burst_of_events_reads_the_count_once_more_not_once_each() {
    let temp = std::env::temp_dir().join(format!("piirit-unread-reads-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let arrive = temp.join("arrive");
    let _ = std::fs::remove_file(&arrive);
    let hold = temp.join("hold");
    std::fs::write(&hold, "get_fresh_msgs").expect("write the hold file");

    // SAFETY: single-threaded; set before Qt and server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Eight events that could move profile 1's count, one batch: two messages
        // arriving, plus a log line that moves nothing.
        std::env::set_var(
            "PIIRIT_FAKE_SEED_EVENTS",
            "IncomingMsg,MsgsChanged,ChatlistChanged,ChatlistItemChanged,Info,\
             IncomingMsg,MsgsChanged,ChatlistChanged,ChatlistItemChanged",
        );
        std::env::set_var("PIIRIT_FAKE_HOLD", &hold);
        // One more message once test drops file, after both reads landed: count may be read again.
        std::env::set_var("PIIRIT_FAKE_LATE_ARRIVAL", &arrive);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut after_burst: Vec<Value> = Vec::new();
    let after_burst_ptr: *mut Vec<Value> = std::ptr::addr_of_mut!(after_burst);

    let mut plan = common::Plan::default();

    // SAFETY for every block: fire only while `exec()` runs on this thread; all they point
    // at outlives it.
    // Burst's last event relayed: read let go.
    plan.until(1, 30, move |last| unsafe {
        let relayed = QString::from_qvariant((*engine_ptr).invoke_method("tail".into(), &[]))
            .map(|value| value.to_string())
            .unwrap_or_default();
        if relayed != "2" && !last {
            return false;
        }
        let _ = std::fs::remove_file(&hold);
        true
    });
    let seen = journal.clone();
    plan.until(1, 30, move |_| reads(&seen).len() >= 2);
    // Settled: nothing more coming from the burst.
    let seen = journal.clone();
    plan.after(0, move || unsafe {
        *after_burst_ptr = reads(&seen);
        std::fs::write(&arrive, "").expect("drop the arrival file");
    });
    let seen = journal.clone();
    plan.until(1, 30, move |_| reads(&seen).len() >= 3);
    plan.after(0, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let calls = common::methods(&journal);
    assert_eq!(
        after_burst,
        vec![json!([1]), json!([1])],
        "expected one read for the first of the eight events and one more \
         for the seven that came while it was out: {calls:?}"
    );
    assert_eq!(
        reads(&journal),
        vec![json!([1]), json!([1]), json!([1])],
        "a message arriving after the burst was settled did not have the \
         count read again: {calls:?}"
    );
}
