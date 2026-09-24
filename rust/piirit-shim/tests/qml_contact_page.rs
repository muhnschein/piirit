//! The contact page shows who a one-to-one chat is with, and no address.
//!
//! Loaded headlessly against the stub Silica module and the recording
//! double: the one member of chat 1 comes up with their name, their line
//! about themselves and the state of the connection, and nothing on the
//! page is an email address.
//!
//! The name can be the reader's own for them: typing one and leaving
//! writes it to the core, which then shows it everywhere; blanking it
//! writes an empty name, which is the core's own way back to what the
//! contact calls themselves.

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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, chatId) {
            loader.setSource(url, { accountId: 1, chatId: chatId, chatName: 'from the list' })
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
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        // A tap on a MouseArea: its clicked() carries the event, and
        // QML refuses to emit it without one.
        // Put the cursor in a name field, as a tap on it does.
        function edit(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.edit()
            return '' + item.editing
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked(null)
            return 'ok'
        }
        /// Leaving the page is what applies what was typed. Back on
        /// screen first: the status has to change for the page to notice.
        function leave() {
            loader.item.status = PageStatus.Active
            loader.item.status = PageStatus.Deactivating
            return 'ok'
        }
        // Every string drawn anywhere on the page, for the one thing
        // none of them may be.
        function allText(node) {
            if (!node) { return '' }
            var out = (node.text !== undefined && node.visible) ? node.text + '|' : ''
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                out += allText(kids[i])
            }
            if (node.contentItem && node.contentItem !== node) {
                out += allText(node.contentItem)
            }
            return out
        }
        function texts() { return allText(loader.item) }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_contact_page_names_the_contact_and_no_address() {
    let temp = std::env::temp_dir().join(format!("piirit-contact-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

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

    // Chat 1 is with Ada, who has written a line.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("ContactPage.qml")),
                1
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("loaded", get!("chat", "loaded"));
        record!("name", get!("contactNameField", "placeholderText"));
        record!("initial", get!("avatarInitial", "text"));
        record!("status", get!("statusLabel", "text"));
        record!("status-shown", get!("statusLabel", "visible"));
        record!("encryption", get!("encryptionLabel", "text"));
        record!("timer", get!("disappearingCombo", "currentIndex"));
        record!("texts", call!("texts"));
        // No name given yet: the field is empty, and what they call
        // themselves stands in it.
        record!("given", get!("contactNameField", "text"));
        record!("own", get!("contactNameField", "placeholderText"));
        // A field from the start, with the hint under it once the
        // cursor is in it and not before.
        record!("hint-before", get!("nameHint", "visible"));
        record!("field-before", get!("contactNameField", "visible"));
        record!("edit", call!("edit", QString::from("contactNameControl")));
        record!("hint-editing", get!("nameHint", "visible"));
        record!("field-editing", get!("contactNameField", "visible"));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("contactNameField"),
                QString::from("Countess")
            )
        );
        record!("leave", call!("leave"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        // The core reloaded behind the save, so the name in the field is
        // the one given.
        record!("renamed", get!("contactNameField", "text"));
        record!("hint-after-leave", get!("nameHint", "visible"));
        record!("field-kept", get!("contactNameField", "text"));
        record!(
            "blanked",
            call!(
                "setText",
                QString::from("contactNameField"),
                QString::from("")
            )
        );
        record!("leave-again", call!("leave"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("theirs-again", get!("contactNameField", "placeholderText"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_page(&steps);
    assert_rename(&steps, &common::calls(&journal));
}

/// A typed name reaches the core as the contact's, and a blank one
/// reaches it as empty -- which is the core's own way back to theirs.
fn assert_rename(steps: &[(&str, String)], calls: &[(String, Value)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        value("given"),
        "",
        "the field holds a name nobody gave. {context}"
    );
    assert_eq!(
        value("own"),
        "ada",
        "the field does not show what the contact calls themselves. {context}"
    );
    for step in ["typed", "leave", "blanked", "leave-again"] {
        assert_eq!(value(step), "ok", "step {step} failed. {context}");
    }
    assert_eq!(
        value("edit"),
        "true",
        "the cursor did not go into the name field. {context}"
    );
    for (label, expected) in [
        ("hint-before", "false"),
        ("field-before", "true"),
        ("hint-editing", "true"),
        ("field-editing", "true"),
        ("hint-after-leave", "false"),
    ] {
        assert_eq!(
            value(label),
            expected,
            "the name is not a field that says what it is for while it is \
             being typed in: {label}. {context}"
        );
    }
    let renames: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "change_contact_name")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        renames.len(),
        2,
        "expected one rename and one blanking on the wire: {calls:?}"
    );
    assert_eq!(
        renames[0].pointer("/1").and_then(Value::as_u64),
        Some(10),
        "the rename did not name the contact: {renames:?}"
    );
    assert_eq!(
        renames[0].pointer("/2").and_then(Value::as_str),
        Some("Countess"),
        "the typed name did not reach the core: {renames:?}"
    );
    assert_eq!(
        renames[1].pointer("/2").and_then(Value::as_str),
        Some(""),
        "a blanked field did not reach the core as an empty name, which is \
         what puts the contact's own back: {renames:?}"
    );
    assert_eq!(
        value("renamed"),
        "Countess",
        "the page does not show the name it just gave. {context}"
    );
    assert_eq!(
        value("field-kept"),
        "Countess",
        "the reload behind the save wiped the field. {context}"
    );
    assert_eq!(
        value("theirs-again"),
        "ada",
        "blanking the name did not go back to the contact's own. {context}"
    );
}

/// The contact's own words and picture, the state of the connection, and
/// nothing that looks like an address.
fn assert_page(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    assert_eq!(
        value("load"),
        "ok",
        "the contact page did not load. {context}"
    );
    assert_eq!(value("loaded"), "true", "the chat never loaded. {context}");
    assert_eq!(
        value("name"),
        "ada",
        "the page does not name the contact. {context}"
    );
    assert_eq!(
        value("initial"),
        "A",
        "the avatar is not standing in with the contact's initial. {context}"
    );
    assert_eq!(
        value("status"),
        "Poet and mathematician",
        "the contact's own line about themselves is missing. {context}"
    );
    assert_eq!(
        value("status-shown"),
        "true",
        "a line that was written is hidden. {context}"
    );
    assert_eq!(
        value("encryption"),
        "End-to-end encrypted",
        "a contact that can be encrypted to is not said to be. {context}"
    );
    assert!(
        !value("texts").contains('@'),
        "an email address is drawn on the contact page. {context}"
    );
    assert_eq!(
        value("timer"),
        "0",
        "the contact page does not offer disappearing messages, or shows \
         something other than Off for a chat with no timer. {context}"
    );
}
