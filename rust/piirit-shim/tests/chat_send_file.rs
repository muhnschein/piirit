//! Attachment send: wire shape, sender view.
//!
//! Pictures: `send_msg` as `Image` (core recodes). Other files: `misc_send_msg` (core names from
//! file). See `media.rs`.
//!
//! Pinned vs real core: `deltachat-jsonrpc/tests/real_server.rs`. Here: shim fills slots from
//! picker, incl. `file://` (URL property core can't open).

// Qt harness: unsafe set_var pre-Qt; borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// `Repeater` reads rows; `QAbstractListModel` gives JS nothing.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
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
        function sendPhoto() { chat.send_file('look at this', '/tmp/piirit-fake/holiday photo.png') }
        function sendUrl() { chat.send_file('', 'file:///tmp/piirit-fake/notes%20and%20more.txt') }
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
    let temp = std::env::temp_dir().join(format!("piirit-send-file-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });
    plan.after(2, move || unsafe {
        (*engine_ptr).invoke_method("sendPhoto".into(), &[]);
    });

    let mut photo_row = String::new();
    let photo_ptr: *mut String = std::ptr::addr_of_mut!(photo_row);
    plan.after(2, move || unsafe {
        let value = (*engine_ptr).invoke_method("newest".into(), &[]);
        *photo_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).invoke_method("sendUrl".into(), &[]);
    });

    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);
    plan.after(2, move || unsafe {
        (*engine_ptr).invoke_method("sendNothing".into(), &[]);
        let error = (*engine_ptr).invoke_method("error".into(), &[]);
        *report_ptr = QString::from_qvariant(error)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let sends: Vec<(String, Value)> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .collect();

    // Picture named: core recodes it, not `File` at original size.
    assert_eq!(
        sends.first(),
        Some(&(
            "send_msg".to_string(),
            serde_json::json!([
                1,
                1,
                {
                    "text": "look at this",
                    "file": "/tmp/piirit-fake/holiday photo.png",
                    "filename": "holiday photo.png",
                    "viewtype": "Image",
                    "quotedMessageId": null,
                }
            ])
        )),
        "a picked photo did not reach the core as a picture. Sends: {sends:?}"
    );

    // Other files: core names via other send. URL: strip `file://`, decode escapes, or core says
    // no such file.
    assert_eq!(
        sends.get(1),
        Some(&(
            "misc_send_msg".to_string(),
            serde_json::json!([
                1,
                1,
                null,
                "/tmp/piirit-fake/notes and more.txt",
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

    // Sender sees own attachment at once: row built from reply.
    assert_eq!(
        photo_row, "Image|holiday photo.png|/tmp/piirit-fake/holiday photo.png|look at this",
        "the sent row did not carry the attachment"
    );
}
