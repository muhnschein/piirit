//! What sending an attachment puts on the wire, and what the sender then
//! sees.
//!
//! Two shapes, because the core treats one file differently from the
//! rest: a picture is named `Image` through `send_msg`, which is what
//! puts it through the core's own recoding, and everything else goes
//! through `misc_send_msg` for the core to name from the file itself.
//! See `media.rs`.
//!
//! The parameter shape is pinned against the real core by
//! `deltachat-jsonrpc/tests/real_server.rs`, which sends one message with a
//! file and one without. This asserts the shim fills those slots from what
//! a picker handed it -- including the `file://` form, which is what a URL
//! property produces and what the core cannot open.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
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

/// The `Repeater` is how the test reads the rows: a `QAbstractListModel`
/// hands nothing to JavaScript directly.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        property string lastError: ''
        ChatMessages { id: chat; account_id: 1; chat_id: 1; onError: lastError = message }
        Repeater {
            id: rows
            model: chat.rows
            Item {
                property string name: model.file_name
                property string path: model.file_path
                property string kind: model.view_type
                property string body: model.text
            }
        }
        function sendPhoto() { chat.send_file('look at this', '/tmp/piiri-fake/holiday photo.png') }
        function sendUrl() { chat.send_file('', 'file:///tmp/piiri-fake/notes%20and%20more.txt') }
        function sendNothing() { chat.send_file('orphan', '') }
        // The newest row, which after a send is the message just sent.
        function newest() {
            if (rows.count === 0) return '(no rows)'
            var row = rows.itemAt(rows.count - 1)
            return row.kind + '|' + row.name + '|' + row.path + '|' + row.body
        }
        function error() { return lastError }
    }
";

#[test]
fn a_picked_file_reaches_the_core_and_comes_back_on_the_row() {
    let temp = std::env::temp_dir().join(format!("piiri-send-file-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*engine_ptr).invoke_method("sendPhoto".into(), &[]);
    });

    let mut photo_row = String::new();
    let photo_ptr: *mut String = std::ptr::addr_of_mut!(photo_row);
    single_shot(Duration::from_secs(5), move || unsafe {
        let value = (*engine_ptr).invoke_method("newest".into(), &[]);
        *photo_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).invoke_method("sendUrl".into(), &[]);
    });

    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);
    single_shot(Duration::from_secs(7), move || unsafe {
        (*engine_ptr).invoke_method("sendNothing".into(), &[]);
        let error = (*engine_ptr).invoke_method("error".into(), &[]);
        *report_ptr = QString::from_qvariant(error)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    let sends: Vec<(String, Value)> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .collect();

    // account, chat, MessageData: a picture is named, so that the core
    // recodes it rather than leaving a `File` at its original size.
    assert_eq!(
        sends.first(),
        Some(&(
            "send_msg".to_string(),
            serde_json::json!([
                1,
                1,
                {
                    "text": "look at this",
                    "file": "/tmp/piiri-fake/holiday photo.png",
                    "filename": "holiday photo.png",
                    "viewtype": "Image",
                    "quotedMessageId": null,
                }
            ])
        )),
        "a picked photo did not reach the core as a picture. Sends: {sends:?}"
    );

    // Everything else is the core's to name, through the other send.
    //
    // A URL is what a `url` property hands back, and the core takes a path:
    // the scheme has to go and the escapes have to be decoded, or the file
    // does not exist as far as the core is concerned.
    assert_eq!(
        sends.get(1),
        Some(&(
            "misc_send_msg".to_string(),
            serde_json::json!([
                1,
                1,
                null,
                "/tmp/piiri-fake/notes and more.txt",
                "notes and more.txt",
                null,
                null
            ])
        )),
        "a file:// URL was not turned into a path the core can open, or a \
         caption-free send sent an empty body instead of none. Sends: {sends:?}"
    );

    assert_eq!(
        sends.len(),
        2,
        "sending with no file picked still called the core. Sends: {sends:?}"
    );
    assert_eq!(
        report, "no file to send",
        "sending with no file picked said nothing about it"
    );

    // The sender sees their own attachment straight away: the row is built
    // from the reply, not waiting on the event.
    assert_eq!(
        photo_row, "Image|holiday photo.png|/tmp/piiri-fake/holiday photo.png|look at this",
        "the sent row did not carry the attachment"
    );
}
