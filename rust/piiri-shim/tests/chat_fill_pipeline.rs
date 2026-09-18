//! Fetches for the rows around a moving reader overlap.
//!
//! One fetch at a time was not enough. A reader flinging up through the
//! history moves faster than a round trip, and a fetch for rows they had
//! already passed held up the fetch for the rows in front of them until it
//! landed: a screen of blanks for as long as that took, reported from a
//! phone as an empty screen for a second on a fast scroll.
//!
//! What is pinned here is that a second ask, made while the first is still
//! in the air, goes out at once -- and that both land in place.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Several pages, so two asks can be about rows a page apart.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        ChatMessages { id: chat; account_id: 1; chat_id: 1 }
        Repeater {
            id: rows
            model: chat.rows
            Item { property bool filled: model.loaded }
        }
        function count() { return '' + chat.count }
        function filledAt(index) {
            if (index < 0 || index >= rows.count) { return 'no-row' }
            return '' + rows.itemAt(index).filled
        }
        function ask(first, last) { chat.hydrate(first, last); return 'ok' }
        function busy() { return chat.hydrating ? 'yes' : 'no' }
    }
";

#[test]
fn a_second_ask_goes_out_while_the_first_is_still_in_the_air() {
    let temp = std::env::temp_dir().join(format!("piiri-fill-pipeline-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRI_FAKE_LONG_CHAT", MESSAGES.to_string());
        // Long enough that the second ask is made while the first is
        // still being answered, which on a phone takes no arranging.
        std::env::set_var("PIIRI_FAKE_FETCH_DELAY_MS", "700");
    }

    piiri_shim::register_qml_types();

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
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("opened", call!("count")));
        // The top of the chat, and then somewhere a page further down,
        // before the first ask can possibly have been answered.
        (*steps_ptr).push(("ask-top", call!("ask", 0, 10)));
        (*steps_ptr).push(("ask-far", call!("ask", 70, 80)));
    });
    single_shot(Duration::from_millis(4400), move || unsafe {
        (*steps_ptr).push(("busy", call!("busy")));
    });
    single_shot(Duration::from_millis(6500), move || unsafe {
        (*steps_ptr).push(("top-filled", call!("filledAt", 0)));
        (*steps_ptr).push(("far-filled", call!("filledAt", 70)));
        (*steps_ptr).push(("idle", call!("busy")));
        (*steps_ptr).push(("still-whole", call!("count")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps, &journal);
}

/// What the run has to show for itself, out of the test body.
fn assert_outcome(steps: &[(&str, String)], journal: &std::path::Path) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("opened"),
        MESSAGES.to_string(),
        "the chat did not open with a row per message. {context}"
    );

    // The one fetch that opened the chat, and then both asks. Whether the
    // second waited for the first is read off the fake core's own clock:
    // an answer takes 700 milliseconds, so two asks that reached it closer
    // together than that were in the air at once.
    let arrivals: Vec<u64> = common::records(journal)
        .iter()
        .filter(|call| call.get("method").and_then(Value::as_str) == Some("get_messages"))
        .filter_map(|call| call.get("at").and_then(Value::as_u64))
        .collect();
    assert_eq!(
        arrivals.len(),
        3,
        "{} fetches reached the core: the chat's opening page and then the \
         two asks, and fewer means an ask was dropped while another was in \
         the air. {context}",
        arrivals.len()
    );
    let gap = arrivals[2].saturating_sub(arrivals[1]);
    assert!(
        gap < 700,
        "the second ask reached the core {gap}ms after the first, which is \
         a whole answer later: it waited for the first to land, and the \
         rows in front of the reader stayed blank for a round trip they did \
         not need to wait for. {context}"
    );
    assert_eq!(
        value("busy"),
        "yes",
        "rows were on their way and the model did not say so. {context}"
    );

    // Both land, each where it was asked for.
    assert_eq!(
        value("top-filled"),
        "true",
        "the top of the chat was not filled in. {context}"
    );
    assert_eq!(
        value("far-filled"),
        "true",
        "the rows a page further down were not filled in: the second ask \
         was dropped. {context}"
    );
    assert_eq!(
        value("idle"),
        "no",
        "every fetch had landed and the model still said rows were on \
         their way. {context}"
    );
    assert_eq!(
        value("still-whole"),
        MESSAGES.to_string(),
        "filling rows in changed how many there are. {context}"
    );
}
