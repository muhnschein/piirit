//! A profile's unread count is read once per burst of events, not once
//! per event.
//!
//! The count on a profile's row follows every event that could have
//! moved it, and a message arriving is three or four of those: the
//! message, the chat's messages, the chat list, the chat's row. Each
//! read is `get_fresh_msgs`, which answers with the id of every unread
//! message the profile has. So the core object keeps one read out per
//! profile: what asks while one is out is noted, and one more read goes
//! when it lands -- one, however many asked, and after all of them, so
//! the count still ends up where the last event left it.
//!
//! The fake core holds each read back for half a second, so the events
//! that come together are all in before the first read lands.

// Qt harness: see download_limit.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::{json, Value};

mod common;

/// The `get_fresh_msgs` calls made so far, by their parameters.
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

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Eight events that could each have moved profile 1's count, in
        // one batch -- two messages arriving, as the core announces them
        // -- and a line of its log among them, which moves nothing.
        std::env::set_var(
            "PIIRIT_FAKE_SEED_EVENTS",
            "IncomingMsg,MsgsChanged,ChatlistChanged,ChatlistItemChanged,Info,\
             IncomingMsg,MsgsChanged,ChatlistChanged,ChatlistItemChanged",
        );
        std::env::set_var("PIIRIT_FAKE_FRESH_DELAY_MS", "500");
        // And one more message, well after both reads have landed: a
        // count that has been read is free to be read again.
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

    // SAFETY for both blocks: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    let seen = journal.clone();
    single_shot(Duration::from_secs(3), move || unsafe {
        *after_burst_ptr = reads(&seen);
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*engine_ptr).quit();
    });

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
