//! A download that never finishes says why, and can be asked about again.
//!
//! Issue 107. The core gives a message `InProgress` when its remainder is
//! asked for and takes it out of that state only once the remainder has
//! been fetched and put in place. A remainder that is not on the server
//! -- not there yet, refused by the sender's relay for its size, expired
//! -- leaves it there for good: the core keeps the download queued and
//! retries it each time it looks at the server, says nothing while it
//! does, and refuses `download_full_message` for a message already in
//! that state. The row said "Downloading…", took no tap, and offered
//! nothing else.
//!
//! What the core can be asked: where it has seen the remainder
//! (`get_message_info_object`'s `serverUrls`), and to look at the server
//! now rather than at the end of IDLE (`maybe_network`). The row carries
//! the first, and a tap on it does the second.

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

mod common;

/// The seeded message the fake core leaves downloading for good.
const STUCK: u32 = 2;
/// One beside it that downloaded long ago.
const DONE: u32 = 1;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string errors: ''
        ChatMessages {
            id: chat
            account_id: 1
            chat_id: 1
            onError: errors += message + ';'
        }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        // The rows as a delegate reads them.
        Repeater {
            id: rows
            model: chat.rows
            delegate: Item {
                property int messageId: model.message_id
                property string downloadState: model.download_state
                property bool downloadMissing: model.download_missing
            }
        }
        function rowOf(messageId) {
            for (var i = 0; i < rows.count; i++) {
                var row = rows.itemAt(i)
                if (row && row.messageId === messageId) {
                    return row
                }
            }
            return null
        }
        function stateOf(messageId) {
            var row = rowOf(messageId)
            return row ? row.downloadState : 'no-row'
        }
        function missing(messageId) {
            var row = rowOf(messageId)
            return row ? '' + row.downloadMissing : 'no-row'
        }
        function loaded() { return '' + chat.loaded }
        function tap(messageId) { chat.download_full(messageId); return 'ok' }
        function errorsSoFar() { return errors }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_download_that_never_lands_says_so_and_asks_the_server_again() {
    let temp = std::env::temp_dir().join(format!("piirit-download-stuck-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_STUCK_MSG", STUCK.to_string());
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

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

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("loaded", call!("loaded")));
        (*steps_ptr).push(("state", call!("stateOf", STUCK)));
        (*steps_ptr).push(("missing", call!("missing", STUCK)));
        (*steps_ptr).push(("done-missing", call!("missing", DONE)));
        call!("tap", STUCK);
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("errors", call!("errorsSoFar")));
        (*steps_ptr).push(("after", call!("stateOf", STUCK)));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");
    assert_eq!(value("loaded"), "true", "the chat did not load. {context}");
    assert_eq!(
        value("state"),
        "InProgress",
        "the fake core did not leave the message downloading, so this proves nothing. {context}"
    );
    assert_eq!(
        value("missing"),
        "true",
        "a download whose remainder no server holds reads as one under way: \
         the core knows it has seen no copy, and the row never asked. {context}"
    );
    assert_eq!(
        value("done-missing"),
        "false",
        "a message that downloaded long ago reads as missing. {context}"
    );
    assert_eq!(
        value("errors"),
        "",
        "tapping a download under way raised an error: the core refuses \
         download_full_message for one. {context}"
    );
    assert_eq!(
        value("after"),
        "InProgress",
        "the row lost its state after the tap. {context}"
    );

    let methods = common::methods(&journal);
    assert!(
        methods.iter().any(|method| method == "maybe_network"),
        "a tap on a download under way did not ask the core to look at \
         the server again. Calls: {methods:?}"
    );
    assert!(
        !methods
            .iter()
            .any(|method| method == "download_full_message"),
        "a download under way was asked for again, which the core refuses. \
         Calls: {methods:?}"
    );
    // Once for the first read and once after the tap: whether the
    // remainder has reached a server is what the tap was asking.
    let asked = methods
        .iter()
        .filter(|method| *method == "get_message_info_object")
        .count();
    assert!(
        asked >= 2,
        "the row was not re-read for where the download stands after the \
         tap. Calls: {methods:?}"
    );
}
