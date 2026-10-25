//! Row fetches for a fast-moving reader overlap.
//!
//! One fetch at a time was too few: a fling outruns the round trip, and passed rows' fetch blocked
//! rows ahead (blank screen on fast scroll). Pinned: second ask goes out while first is in air; both land.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Several pages, so two asks can be a page apart.
const MESSAGES: u32 = 130;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
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
    let temp = std::env::temp_dir().join(format!("piirit-fill-pipeline-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_LONG_CHAT", MESSAGES.to_string());
        // Long enough: second ask made while first still answering.
        std::env::set_var("PIIRIT_FAKE_FETCH_DELAY_MS", "700");
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.after(3, move || unsafe {
        (*steps_ptr).push(("opened", call!("count")));
        // Chat top, then a page further down, before first ask can be answered.
        (*steps_ptr).push(("ask-top", call!("ask", 0, 10)));
        (*steps_ptr).push(("ask-far", call!("ask", 70, 80)));
    });
    plan.after(0, move || unsafe {
        (*steps_ptr).push(("busy", call!("busy")));
    });
    plan.after(2, move || unsafe {
        (*steps_ptr).push(("top-filled", call!("filledAt", 0)));
        (*steps_ptr).push(("far-filled", call!("filledAt", 70)));
        (*steps_ptr).push(("idle", call!("busy")));
        (*steps_ptr).push(("still-whole", call!("count")));
        (*engine_ptr).quit();
    });

    plan.run();
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
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("opened"),
        MESSAGES.to_string(),
        "the chat did not open with a row per message. {context}"
    );

    // Fetch that opened chat, then both asks. Second waiting on first read off fake's clock:
    // answer takes 700 ms; asks reaching it closer than that were in air at once.
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

    // Both land, each where asked.
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
