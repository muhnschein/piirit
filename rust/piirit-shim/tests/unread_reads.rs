//! Unread count read once per burst of events, not per event.
//!
//! Each event that could move count (message, chat's messages, chat list, chat row)
//! asks for `get_fresh_msgs`. Core keeps one read out per profile: asks during one
//! are noted; one more read after it lands. However many asked, read after all, so
//! count ends where last event left it.
//!
//! Fake core holds reads back half a second, so burst is all in before first read.

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

#[test]
fn a_burst_of_events_reads_the_count_once_more_not_once_each() {
    let temp = std::env::temp_dir().join(format!("piirit-unread-reads-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

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
        std::env::set_var("PIIRIT_FAKE_FRESH_DELAY_MS", "500");
        // One more message after both reads landed: count may be read again.
        std::env::set_var("PIIRIT_FAKE_LATE_ARRIVAL_MS", "4000");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from("import QtQuick 2.0\nItem {}\n"));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut after_burst: Vec<Value> = Vec::new();
    let after_burst_ptr: *mut Vec<Value> = std::ptr::addr_of_mut!(after_burst);

    let mut plan = common::Plan::default();

    // SAFETY for both blocks: fire only while `exec()` runs on this thread; all they point
    // at outlives it.
    let seen = journal.clone();
    plan.after(3, move || unsafe {
        *after_burst_ptr = reads(&seen);
    });
    plan.after(3, move || unsafe {
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
