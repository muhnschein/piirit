//! Chat opens with one screen fetched, rest lands after: maybe once reader already looks. Rows
//! landing while reader is at newest owe read receipts, as rows there at open do; rows landing
//! while scrolled away don't.

// Qt harness: see chat_actions.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Fake core: 101..=130 unread (and 2); chat opens with newest 20, 111..=130.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        // Page still sliding in: nobody looking yet.
        ChatMessages {
            id: messages
            account_id: 1
            chat_id: 1
            reading_history: true
        }
        function look(on) { messages.reading_history = !on; return 'ok' }
        function hydrate(first, last) { messages.hydrate(first, last); return 'ok' }
        function count() { return '' + messages.count }
        function hydrating() { return '' + messages.hydrating }
    }
";

/// Every message id sent a read receipt so far.
fn seen(journal: &Path) -> BTreeSet<u64> {
    common::calls(journal)
        .into_iter()
        .filter(|(name, _)| name == "markseen_msgs")
        .filter_map(|(_, params)| params.get(1).and_then(Value::as_array).cloned())
        .flatten()
        .filter_map(|id| id.as_u64())
        .collect()
}

/// Every message id fetched so far.
fn fetched(journal: &Path) -> BTreeSet<u64> {
    common::calls(journal)
        .into_iter()
        .filter(|(name, _)| name == "get_messages")
        .filter_map(|(_, params)| params.get(1).and_then(Value::as_array).cloned())
        .flatten()
        .filter_map(|id| id.as_u64())
        .collect()
}

#[test]
#[allow(clippy::too_many_lines)]
fn rows_landing_while_the_reader_looks_get_receipts_and_none_while_scrolled_away() {
    let temp = std::env::temp_dir().join(format!("piirit-open-receipts-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_LONG_CHAT", MESSAGES.to_string());
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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

    let opened: BTreeSet<u64> = (111..=130).collect();
    let landed_looking: BTreeSet<u64> = (101..=110).collect();
    let mut at_open = BTreeSet::new();
    let at_open_ptr: *mut BTreeSet<u64> = std::ptr::addr_of_mut!(at_open);
    let mut after_landing = BTreeSet::new();
    let after_landing_ptr: *mut BTreeSet<u64> = std::ptr::addr_of_mut!(after_landing);
    let mut first_fetch = BTreeSet::new();
    let first_fetch_ptr: *mut BTreeSet<u64> = std::ptr::addr_of_mut!(first_fetch);

    let path: PathBuf = journal.clone();
    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // Chat open; page settles, reader at newest: rows there get receipts.
    let at = path.clone();
    plan.until(1, 30, move |last| unsafe {
        if call!("count") != MESSAGES.to_string() && !last {
            return false;
        }
        *first_fetch_ptr = fetched(&at);
        call!("look", true);
        true
    });
    let at = path.clone();
    let wanted = opened.clone();
    plan.until(1, 30, move |last| unsafe {
        let now = seen(&at);
        if !now.is_superset(&wanted) && !last {
            return false;
        }
        *at_open_ptr = now;
        // View asks for rows above screen while reader still at newest.
        call!("hydrate", 99, 109);
        true
    });
    let at = path.clone();
    let wanted = landed_looking.clone();
    plan.until(1, 30, move |last| unsafe {
        let now = seen(&at);
        let done = call!("hydrating") == "false" && fetched(&at).contains(&101);
        let ready = done && now.is_superset(&wanted);
        if !ready && !last {
            return false;
        }
        *after_landing_ptr = now;
        // Reader scrolls up: rows landing now are history, not seen.
        call!("look", false);
        call!("hydrate", 0, 9);
        true
    });
    let at = path;
    plan.until(1, 30, move |last| unsafe {
        let done = call!("hydrating") == "false" && fetched(&at).contains(&2);
        if !done && !last {
            return false;
        }
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    let finally = seen(&journal);
    let context = format!(
        "first fetch: {first_fetch:?}; seen at open: {at_open:?}; after landing: \
         {after_landing:?}; at end: {finally:?}"
    );
    assert_eq!(
        first_fetch, opened,
        "the chat did not open with its newest screen of messages. {context}"
    );
    assert!(
        at_open.is_superset(&opened),
        "the messages on screen when the reader looked got no read receipt. {context}"
    );
    assert!(
        after_landing.is_superset(&landed_looking),
        "unread messages that loaded while the reader was at the newest got no read \
         receipt; the sender never learns they were read. {context}"
    );
    assert!(
        !finally.contains(&2),
        "a message that loaded while the reader was scrolled away got a read receipt. \
         {context}"
    );
}
