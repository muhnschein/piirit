//! Adding to a group that already exists.
//!
//! The picker greys whoever is already in, narrows the list to what is
//! typed into its search field, adds whoever was ticked through the group
//! it was handed, and goes back to that group's page.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see piiri-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded rather than performed.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:Foo.qml|pop|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

impl PageStackProbe {
    fn push(&mut self, page: QString, _properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let current = self.log.to_string();
        self.log = format!("{current}push:{name}|").into();
        self.log_changed();
    }

    fn pop(&mut self) {
        let current = self.log.to_string();
        self.log = format!("{current}pop|").into();
        self.log_changed();
    }
}

// The group is the one the page that opens this one owns; here that is
// the probe's.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        ChatInfo {
            id: group
            account_id: 1
            chat_id: 2
        }
        Connections {
            target: core
            onStatus_changed: {
                if (core.status === 'ready') { group.reload() }
            }
        }
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { accountId: 1, chat: group })
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
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

#[test]
fn the_picker_adds_who_was_ticked_and_greys_who_is_in() {
    let temp = std::env::temp_dir().join(format!("piiri-add-members-{}", std::process::id()));
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
    let stack_box = QObjectBox::new(PageStackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
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

    // Loaded once the group is, so the rows can tell who is in it.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("AddMembersPage.qml"))
            )
        );
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        // Ada is in already; Grace is not.
        record!("in-greyed", get!("memberRow10", "enabled"));
        record!("out-offered", get!("memberRow11", "enabled"));
        record!("nothing-picked", get!("addButton", "enabled"));
        // Tapping someone already in does nothing.
        record!("tap-in", call!("click", QString::from("memberRow10")));
        record!("still-nothing", get!("addButton", "enabled"));
        // Searching narrows the list to Grace, whom the fake core finds by
        // her address.
        record!(
            "search",
            call!(
                "setText",
                QString::from("searchField"),
                QString::from("grace")
            )
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("ada-hidden", get!("memberRow10", "enabled"));
        record!("grace-found", get!("memberRow11", "enabled"));
        record!("tap-out", call!("click", QString::from("memberRow11")));
        record!("picked", get!("addButton", "enabled"));
        record!("add", call!("click", QString::from("addButton")));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(
        &steps,
        &stack_box.pinned().borrow().log.to_string(),
        &common::calls(&journal),
    );
}

/// Only the one who was not in could be picked, and picking them added
/// them to the group and went back.
fn assert_outcome(steps: &[(&str, String)], navigation: &str, calls: &[(String, Value)]) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}\nnavigation: {navigation}\ncalls: {names:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    for label in ["load", "tap-in", "search", "tap-out", "add"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert!(
        value("ada-hidden").starts_with("missing:"),
        "searching for Grace still lists Ada. {context}"
    );
    assert_eq!(
        value("grace-found"),
        "true",
        "searching for Grace did not find her, or found her greyed. {context}"
    );
    let searched: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "get_contacts")
        .map(|(_, params)| params.clone())
        .collect();
    assert!(
        searched.contains(&serde_json::json!([1, 0, "grace"])),
        "the typed search never reached the core as a query. {context}"
    );
    assert_eq!(
        value("in-greyed"),
        "false",
        "someone already in the group can be picked again. {context}"
    );
    assert_eq!(
        value("out-offered"),
        "true",
        "someone not in the group cannot be picked. {context}"
    );
    assert_eq!(
        value("nothing-picked"),
        "false",
        "adding is offered with nobody picked. {context}"
    );
    assert_eq!(
        value("still-nothing"),
        "false",
        "tapping someone already in counted as picking them. {context}"
    );
    assert_eq!(
        value("picked"),
        "true",
        "picking someone did not enable adding. {context}"
    );

    let added: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "add_contact_to_chat")
        .map(|(_, params)| params.clone())
        .collect();
    assert_eq!(
        added,
        vec![serde_json::json!([1, 2, 11])],
        "the picked contact was not the one added, or not the only one. {context}"
    );
    assert!(
        navigation.ends_with("pop|"),
        "adding did not go back to the group. {context}"
    );
}
