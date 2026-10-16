//! Conversation page on a contact request: answer bar in field's place, field back on Accept.
//!
//! Page loaded from QML tree copy with `EnterKey` taken out; see
//! `common::qml_tree_without_enter_key` for why shipped file can't load headlessly.

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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, {
                accountId: 1,
                chatId: 1,
                chatName: 'Mira Kallio',
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
fn a_contact_request_opens_with_its_answer_where_the_field_would_be() {
    let temp = std::env::temp_dir().join(format!("piirit-request-page-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_REQUESTS", "1");
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml"))
            )
        );
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("bar", get!("requestBar", "visible"));
        record!("field", get!("inputRow", "visible"));
        record!("field-height", get!("inputRow", "height"));
        record!("subtitle", get!("conversationHeader", "subtitle"));
        record!("accept", call!("tap", QString::from("acceptButton")));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("accepted-bar", get!("requestBar", "visible"));
        record!("accepted-field", get!("inputRow", "visible"));
        record!("accepted-subtitle", get!("conversationHeader", "subtitle"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// What the page shows before and after Accept.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("bar"),
        "true",
        "a contact request opened without a way to answer it. {context}"
    );
    assert_eq!(
        value("field"),
        "false",
        "a contact request offers a field nothing can be sent from. {context}"
    );
    assert_eq!(
        value("field-height"),
        "0",
        "the hidden field still takes room above the bar. {context}"
    );
    assert_eq!(
        value("subtitle"),
        "Contact request",
        "the header does not say the chat is a request. {context}"
    );
    assert_eq!(value("accept"), "ok", "Accept was not there. {context}");
    assert_eq!(
        value("accepted-bar"),
        "false",
        "the request bar stayed after accepting. {context}"
    );
    assert_eq!(
        value("accepted-field"),
        "true",
        "accepting did not bring the message field back. {context}"
    );
    assert_eq!(
        value("accepted-subtitle"),
        "",
        "an accepted one-to-one chat is still called a request. {context}"
    );
}
