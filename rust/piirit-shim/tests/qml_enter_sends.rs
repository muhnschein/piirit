//! The return key sends, once the reader has asked it to.
//!
//! The message field is a multi-line one: the return key puts in a line
//! break and the button sends, which is what every other client on the
//! phone does with a message longer than a remark. The settings page
//! offers the other arrangement, where the key sends. What the keyboard
//! does with the key is Silica's `EnterKey`, which has no stub and is
//! taken out of the page every test loads (see
//! `common::qml_tree_without_enter_key`); `qml_syntax.rs` holds the
//! shipped file to routing its click to `enterPressed`, and this drives
//! that function: nothing goes out while the setting is off, and the
//! message goes out once it is on.

// Qt harness: see qml_send_file.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::Path;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// The probe imports the components by absolute URL, to reach the
/// `Settings` singleton the page reads; it is loaded from data, which has
/// no directory of its own to resolve one against. The copied tree's
/// components, not the source tree's: a singleton is one per directory it
/// is imported from, and the page is loaded from the copy.
fn probe_qml(tree: &Path) -> String {
    let components = tree.join("components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}
        // Created the way pageStack.push does, still on its way in.
        function load(url, accountId, chatId) {{
            loader.setSource('', {{}})
            loader.setSource(url, {{
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Activating
            }})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function settle() {{ loader.item.status = PageStatus.Active; return 'ok' }}
        function findIn(node, name) {{
            if (!node) {{ return null }}
            if (node.objectName === name) {{ return node }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findIn(kids[i], name)
                if (hit) {{ return hit }}
            }}
            if (node.contentItem && node.contentItem !== node) {{
                return findIn(node.contentItem, name)
            }}
            return null
        }}
        function type(text) {{
            var field = findIn(loader.item, 'messageField')
            if (!field) {{ return 'missing:messageField' }}
            field.text = text
            return 'ok'
        }}
        function typed() {{
            var field = findIn(loader.item, 'messageField')
            return field ? field.text : 'missing:messageField'
        }}
        // What the keyboard's key does on the shipped page.
        function enter() {{ loader.item.enterPressed(); return 'ok' }}
        function sends() {{ return '' + loader.item.enterSends }}
        // The settings page's switch, from the other side.
        function setSends(on) {{ Settings.enterSends = on; return 'ok' }}
    }}
",
        components.display()
    )
}

const MESSAGE: &str = "one line, sent with the key";

#[test]
#[allow(clippy::too_many_lines)]
fn the_return_key_sends_only_once_asked_to() {
    let temp = std::env::temp_dir().join(format!("piirit-enter-sends-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml(&tree)));

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1
            )
        );
        record!("settle", call!("settle"));
    });

    // The chat is in by now. Off, the key sends nothing; on, it sends
    // what is in the field.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("sends-by-default", call!("sends"));
        record!("type", call!("type", QString::from(MESSAGE)));
        record!("enter-while-off", call!("enter"));
        record!("still-typed", call!("typed"));
        record!("turn-on", call!("setSends", true));
        record!("sends-once-on", call!("sends"));
        record!("enter-while-on", call!("enter"));
    });

    // The send has been answered, and the field cleared by it.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("typed-after-send", call!("typed"));
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
    let calls = common::calls(&journal);
    let sent: Vec<String> = calls
        .iter()
        .filter(|(name, _)| name == "misc_send_msg")
        .map(|(_, params)| params.to_string())
        .collect();
    let context = format!("steps: {steps:?}\nsends: {sent:?}");

    for (label, expected, complaint) in [
        ("load", "ok", "the conversation page did not load"),
        (
            "sends-by-default",
            "false",
            "the return key sends before anyone asked it to",
        ),
        ("type", "ok", "there is no field to type into"),
        (
            "still-typed",
            MESSAGE,
            "the return key took the message out of the field while the setting was off",
        ),
        (
            "sends-once-on",
            "true",
            "the page does not follow the setting",
        ),
        (
            "typed-after-send",
            "",
            "the message is still in the field after the key sent it",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    assert_eq!(
        sent.len(),
        1,
        "the return key sent {} message(s): none while the setting is off, \
         one once it is on. {context}",
        sent.len()
    );
    assert!(
        sent[0].contains(MESSAGE),
        "what the key sent is not what was in the field. {context}"
    );
    let _ = std::fs::remove_dir_all(&temp);
}
