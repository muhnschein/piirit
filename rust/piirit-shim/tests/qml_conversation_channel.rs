//! Conversation page on a joined channel: no message field (only owner posts). Ordinary chat
//! beside it keeps field.
//!
//! Page loaded from QML tree copy with `EnterKey` taken out; see
//! `common::qml_tree_without_enter_key`. Steps run in probe off page's own `loaded`, not clock.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Checks, a second apart, before probe counts as stuck.
const PATIENCE: u32 = 59;

/// Readings probe takes, by label.
const READINGS: [&str; 4] = [
    "channel-field",
    "channel-field-height",
    "chat-field",
    "chat-field-height",
];

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        id: probe
        property string pageUrl: ''
        property var readings: ({})
        // 0 channel opening, 1 ordinary chat opening, 2 done.
        property int step: 0
        Loader { id: loader }
        function open(chatId) {
            loader.setSource(probe.pageUrl, {
                accountId: 1,
                chatId: chatId,
                chatName: 'News',
                status: PageStatus.Active
            })
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
        function start(url) {
            probe.pageUrl = url
            var result = probe.open(2)
            poll.running = result === 'ok'
            return result
        }
        // Field read once chat loaded (shape lands with messages).
        function take(prefix) {
            var row = findIn(loader.item, 'inputRow')
            probe.readings[prefix + '-field'] = row ? '' + row.visible : 'missing'
            probe.readings[prefix + '-field-height'] = row ? '' + row.height : 'missing'
        }
        Timer {
            id: poll
            interval: 50
            repeat: true
            onTriggered: {
                var messages = findIn(loader.item, 'messages')
                if (!messages || !messages.loaded) { return }
                if (probe.step === 0) {
                    probe.take('channel')
                    probe.step = 1
                    probe.open(1)
                } else if (probe.step === 1) {
                    probe.take('chat')
                    probe.step = 2
                    poll.running = false
                }
            }
        }
        function finished() { return probe.step === 2 ? 'yes' : 'no' }
        function reading(label) {
            var value = probe.readings[label]
            return value === undefined ? 'unread' : value
        }
    }
";

#[test]
fn a_joined_channel_opens_without_a_message_field() {
    let temp = std::env::temp_dir().join(format!("piirit-channel-page-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_CHANNELS", "2");
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

    let steps: common::Steps = common::Steps::default();
    let page = common::page_url_in(&tree, "ConversationPage.qml");
    let loading = steps.clone();
    plan.after(1, move || unsafe {
        common::record(
            &loading,
            "load",
            call!("start", QString::from(page.clone())).into(),
        );
    });

    // Once a second: done, or out of time.
    let done = steps.clone();
    plan.until(1, PATIENCE, move |last| unsafe {
        if !last && call!("finished") != "yes" {
            return false;
        }
        for label in READINGS {
            common::record(&done, label, call!("reading", QString::from(label)).into());
        }
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    let context = format!("steps: {steps:?}");
    let value = |label: &str| common::value_of(&steps, label).to_string();
    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("channel-field"),
        "false",
        "a joined channel offers a field its reader cannot post from. {context}"
    );
    assert_eq!(
        value("channel-field-height"),
        "0",
        "the hidden field still takes room under the channel's posts. {context}"
    );
    assert_eq!(
        value("chat-field"),
        "true",
        "an ordinary chat lost its message field. {context}"
    );
    assert_ne!(
        value("chat-field-height"),
        "0",
        "an ordinary chat's field takes no room. {context}"
    );
}
