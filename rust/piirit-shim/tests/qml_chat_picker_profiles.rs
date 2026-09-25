//! The chat picker's choice of profile: offered to a caller whose chat can
//! be any profile's, and to no other.
//!
//! Asked for, and with more than one profile to choose from, a choice over
//! the list names the profile the list is on, the way the profiles page
//! names it, and turning it to another profile turns the list to that
//! profile's chats -- which is where `accountId`, read back by the caller,
//! says the picked chat is from. Not asked for, there is no choice:
//! forwarding and sharing send from the profile they were started in.

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

/// The probe, with the components directory filled in; see `qml_chat_list.rs`.
fn probe_qml() -> String {
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://__COMPONENTS__'
    Item {
        id: probe

        property QtObject pageStack: QtObject {
            function pop() {}
        }

        Loader { id: loader }
        function load(url, properties) {
            loader.setSource('', {})
            loader.setSource(url, JSON.parse(properties))
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
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function page(property) { return '' + loader.item[property] }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function readProfiles() { core.refresh_accounts(); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_picker_offers_the_other_profiles_only_when_asked() {
    let temp =
        std::env::temp_dir().join(format!("piirit-qml-picker-profiles-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. Two profiles, so there is a choice.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml().as_str()));

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
    macro_rules! load {
        ($properties:expr) => {
            call!(
                "load",
                QString::from(common::page_url("ChatPickerPage.qml")),
                QString::from($properties)
            )
        };
    }

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!("read-profiles", call!("readProfiles"));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        // Asked for: the choice, on the profile the picker opened on.
        record!(
            "load-choice",
            load!(r#"{"accountId": 1, "profileChoice": true}"#)
        );
        record!("profiles", call!("page", QString::from("profileCount")));
        record!("choice-shown", get!("pickerProfileCombo", "visible"));
        record!("choice-says", get!("pickerProfileCombo", "value"));
        record!("list-on", get!("pickerChats", "account_id"));

        // Turned to the other profile, the list follows.
        record!("turn", call!("click", QString::from("pickerProfile2")));
        record!("page-on", call!("page", QString::from("accountId")));
        record!("list-turned", get!("pickerChats", "account_id"));
        record!("turned-says", get!("pickerProfileCombo", "value"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("turned-loaded", call!("page", QString::from("chatsLoaded")));
        record!("turned-count", get!("pickerChats", "count"));

        // Not asked for: no choice, whatever there is to choose from.
        record!("load-plain", load!(r#"{"accountId": 1}"#));
        record!("plain-hidden", get!("pickerProfileCombo", "visible"));

        // Opened on a profile deleted since -- a quick action's -- with
        // the profile the settings were opened from to fall back on.
        record!(
            "load-gone",
            load!(r#"{"accountId": 9, "profileChoice": true, "fallbackAccountId": 2}"#)
        );
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("gone-on", call!("page", QString::from("accountId")));
        record!("gone-list", get!("pickerChats", "account_id"));
        record!("gone-loaded", call!("page", QString::from("chatsLoaded")));
        record!("gone-error", call!("page", QString::from("errorMessage")));
        // With nothing to fall back on, the first profile there is.
        record!(
            "load-gone-first",
            load!(r#"{"accountId": 9, "profileChoice": true}"#)
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("gone-first-on", call!("page", QString::from("accountId")));
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

    assert_eq!(
        value("load-choice"),
        "ok",
        "the picker did not load. {context}"
    );
    assert_eq!(
        value("profiles"),
        "2",
        "the picker does not count the profiles there are. {context}"
    );
    assert_eq!(
        value("choice-shown"),
        "true",
        "the picker asked for a choice of profile does not offer one. \
         {context}"
    );
    assert_eq!(
        value("choice-says"),
        "account1@example.org",
        "the choice does not name the profile the list is on. {context}"
    );
    assert_eq!(value("list-on"), "1", "{context}");
    assert_eq!(
        value("page-on"),
        "2",
        "turning the choice does not say which profile it was turned to. \
         {context}"
    );
    assert_eq!(
        value("list-turned"),
        "2",
        "the list does not follow the profile chosen. {context}"
    );
    assert_eq!(
        value("turned-says"),
        "account2@example.org",
        "the choice does not name the profile it was turned to. {context}"
    );
    assert_eq!(
        value("turned-loaded"),
        "true",
        "the other profile's chats never arrive. {context}"
    );
    assert_ne!(
        value("turned-count"),
        "0",
        "the other profile's list is empty. {context}"
    );
    assert_eq!(
        value("plain-hidden"),
        "false",
        "a picker not asked for a choice of profile offers one. {context}"
    );
    assert_eq!(value("load-gone"), "ok", "{context}");
    assert_eq!(
        (
            value("gone-on").as_str(),
            value("gone-list").as_str(),
            value("gone-loaded").as_str(),
            value("gone-error").as_str()
        ),
        ("2", "2", "true", ""),
        "a picker opened on a deleted profile stayed on it: an error over \
         an empty list, and no way to turn it with one profile left. \
         {context}"
    );
    assert_eq!(value("gone-first-on"), "1", "{context}");
}
