//! Long chat addressable from open: row per message, fill only what is viewed.
//!
//! First message is row 0 always: going to start is a scroll, not a fetch.

// Qt harness: see qml_chat_list.rs.
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
use serde_json::Value;

mod common;

/// More than a page, so filling in must happen.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string revealedAt: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Repeater {
            id: rows
            model: chat.rows
            Item {
                property int mid: model.message_id
                property bool filled: model.loaded
                property int day: model.day_number
            }
        }
        Connections {
            target: chat
            onRevealed: revealedAt = message_id + '@' + row
        }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        function count() { return '' + chat.count }
        /// The first and last row, which is the whole chat or it is not.
        function edges() {
            if (rows.count === 0) { return 'empty' }
            return rows.itemAt(0).mid + '..' + rows.itemAt(rows.count - 1).mid
        }
        /// How many rows have their message, as against standing empty.
        function filled() {
            var total = 0
            for (var i = 0; i < rows.count; i++) {
                if (rows.itemAt(i).filled) { total += 1 }
            }
            return '' + total
        }
        function filledAt(index) {
            if (index < 0 || index >= rows.count) { return 'no-row' }
            return '' + rows.itemAt(index).filled
        }
        /// The day a row is filed under, fetched or not. A list section is
        /// this number, so a row that does not know it yet is a heading
        /// saying 1 January 1970.
        function dayAt(index) {
            if (index < 0 || index >= rows.count) { return 'no-row' }
            return '' + rows.itemAt(index).day
        }
        /// How many rows are under the epoch, which must be none.
        function epochRows() {
            var total = 0
            for (var i = 0; i < rows.count; i++) {
                if (rows.itemAt(i).day === 0) { total += 1 }
            }
            return '' + total
        }
        function hydrate(first, last) { chat.hydrate(first, last); return 'ok' }
        function reveal(id) { chat.reveal(id); return 'ok' }
        function revealed() { return revealedAt }
        function arrive() { chat.send('from the other end'); return 'ok' }
    }
";

#[test]
fn every_message_has_a_row_and_only_what_is_looked_at_is_fetched() {
    let temp = std::env::temp_dir().join(format!("piirit-paging-{}", std::process::id()));
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
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("opened", call!("count")));
        (*steps_ptr).push(("opened-edges", call!("edges")));
        (*steps_ptr).push(("opened-filled", call!("filled")));
        // Row 0 from start: no fetch to find the beginning.
        (*steps_ptr).push(("first-empty", call!("filledAt", 0)));
        // Knows its day before anything else.
        (*steps_ptr).push(("epoch-rows", call!("epochRows")));
        (*steps_ptr).push(("day-empty", call!("dayAt", 0)));
        // Search result: lookup, not fetch.
        (*steps_ptr).push(("reveal", call!("reveal", 3)));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("revealed", call!("revealed")));
        // Scrolled to top: fill in what is there.
        (*steps_ptr).push(("scrolled", call!("hydrate", 0, 20)));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("first-filled", call!("filledAt", 0)));
        (*steps_ptr).push(("day-filled", call!("dayAt", 0)));
        (*steps_ptr).push(("still-whole", call!("count")));
        (*steps_ptr).push(("still-edges", call!("edges")));
        (*steps_ptr).push(("arrive", call!("arrive")));
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("arrived", call!("count")));
        (*steps_ptr).push(("arrived-edges", call!("edges")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps, &journal);
}

/// What run shows, out of test body.
fn assert_outcome(steps: &[(&str, String)], journal: &std::path::Path) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let number = |label: &str| value(label).parse::<u32>().unwrap_or(0);
    let context = format!("steps: {steps:?}");

    // Whole chat addressable from open.
    assert_eq!(
        number("opened"),
        MESSAGES,
        "a {MESSAGES}-message chat did not open with a row per message, so \
         where the beginning is depends on what has been fetched. {context}"
    );
    assert_eq!(
        value("opened-edges"),
        format!("1..{MESSAGES}"),
        "the rows do not run from the first message to the last. {context}"
    );
    // Only one page fetched.
    assert_eq!(
        number("opened-filled"),
        50,
        "opening the chat fetched something other than one page of \
         messages -- the point of a row per message is that it costs a \
         list of numbers, not a list of messages. {context}"
    );
    assert_eq!(
        value("first-empty"),
        "false",
        "the first message was fetched on open, so this run says nothing \
         about filling in. {context}"
    );

    assert_days(steps);

    // Search result needs no fetch.
    assert_eq!(
        value("revealed"),
        "3@2",
        "revealing message 3 did not report its row straight away. \
         {context}"
    );

    // Scrolling fills in, changes nothing else.
    assert_eq!(
        value("first-filled"),
        "true",
        "the rows at the top were not filled in when they were asked for. \
         {context}"
    );
    assert_eq!(
        number("still-whole"),
        MESSAGES,
        "filling rows in changed how many there are: it has to happen in \
         place, or the view loses its position every time. {context}"
    );
    assert_eq!(
        value("still-edges"),
        format!("1..{MESSAGES}"),
        "filling rows in moved the ends of the chat. {context}"
    );

    // Arrival: one more row at end.
    assert_eq!(
        number("arrived"),
        MESSAGES + 1,
        "a message arriving did not add a row. {context}"
    );
    assert_eq!(
        value("arrived-edges"),
        format!("1..{}", MESSAGES + 1),
        "a message arriving moved the beginning of the chat. {context}"
    );

    // Never whole chat in one go.
    let biggest = common::calls(journal)
        .into_iter()
        .filter(|(method, _)| method == "get_messages")
        .filter_map(|(_, params)| {
            params
                .as_array()
                .and_then(|array| array.get(1))
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .max()
        .unwrap_or(0);
    assert!(
        biggest <= 50,
        "one fetch asked the core for {biggest} messages at once; the whole \
         point is that no single fetch is the size of the chat. {context}"
    );
}

/// Row knows its day before its message, and keeps it.
///
/// Unfilled rows sit under epoch heading (1 Jan 1970); each moves to real day on arrival,
/// resizing a laid-out heading over the row beneath.
fn assert_days(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("epoch-rows"),
        "0",
        "{} rows opened under day 0, so the list has a section headed 1 \
         January 1970 in it. {context}",
        value("epoch-rows")
    );
    assert_ne!(
        value("day-empty"),
        "0",
        "the first row, which has not been fetched, does not know its day. \
         {context}"
    );
    assert_eq!(
        value("day-filled"),
        value("day-empty"),
        "the first row moved from day {:?} to day {:?} when it was filled \
         in, so its heading is rebuilt and resized after the view has laid \
         out around it. {context}",
        value("day-empty"),
        value("day-filled")
    );
}
