//! Capture sent at once (`sendShared`) shows in its chat, even with slow chat load and slow
//! send answer (e.g. video copied into core).
//!
//! Rows read from core on open. Send answered mid-read put its message on a row the read then
//! replaced with pre-send state: in chat, not on page, until reopened. So nothing goes alone
//! until chat loaded, and sent message lands on rows page shows.

// Qt harness: see qml_send_file.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, properties) {
            var given = JSON.parse(properties)
            given.status = PageStatus.Active
            loader.setSource(url, given)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_capture_sent_at_once_shows_in_a_chat_that_was_still_loading() {
    let temp =
        std::env::temp_dir().join(format!("piirit-qml-at-once-loading-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();
    let cache = temp.join("cache");
    let captures = cache.join("piirit/piirit/captures");
    std::fs::create_dir_all(&captures).expect("create the captures directory");

    let photo = captures.join("photo-20261013-070707.png");
    std::fs::copy(common::a_real_picture(), &photo).expect("copy a picture");

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("XDG_CACHE_HOME", &cache);
        // Chat's first page 2s coming; send answered after 0.5s: after message list read, long
        // before its page.
        std::env::set_var("PIIRIT_FAKE_PAGE_DELAY_MS", "2000");
        std::env::set_var("PIIRIT_FAKE_SEND_DELAY_MS", "500");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! probe {
        ($label:expr, $name:expr, $property:expr) => {
            (*steps_ptr).push((
                $label,
                call!("get", QString::from($name), QString::from($property)),
            ))
        };
    }

    let page = common::page_url_in(&tree, "ConversationPage.qml");
    let photo_path = photo.to_string_lossy().into_owned();

    let mut plan = common::Plan::default();

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    plan.after(1, move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(page.as_str()),
                QString::from(
                    serde_json::json!({
                        "accountId": 1,
                        "chatId": 1,
                        "sharedFile": photo_path,
                        "sendShared": true,
                    })
                    .to_string()
                )
            ),
        ));
    });

    // Sent and answered by now.
    plan.after(7, move || unsafe {
        probe!("loaded", "messages", "loaded");
        probe!("rows", "messages", "count");
        probe!("bar", "attachmentBar", "visible");
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");
    let sends: Vec<_> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .collect();

    assert_eq!(value("load"), "ok", "the chat did not load. {context}");
    assert_eq!(
        sends.len(),
        1,
        "the picture was not sent once, on its own. {context}. Sends: {sends:?}"
    );
    assert_eq!(
        (value("loaded").as_str(), value("bar").as_str()),
        ("true", "false"),
        "the chat did not load, or the picture is still on the bar. {context}"
    );
    // Two chat messages, plus the sent one.
    assert_eq!(
        value("rows"),
        "3",
        "the message sent is not on the page, only in the chat. {context}"
    );
}
