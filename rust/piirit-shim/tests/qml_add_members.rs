//! Adding to an existing group.
//!
//! Picker greys members already in, narrows list to search, adds ticked people to the group it was
//! handed, returns to its page. Says nobody to add only once core has said so.

// Qt harness: unsafe set_var pre-Qt; borrow_as_ptr.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    // `qt_method!` decls must match dispatcher's by-value params; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded not performed.
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

// Group is the opening page's; here, the probe's.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
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
    let temp = std::env::temp_dir().join(format!("piirit-add-members-{}", std::process::id()));
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

    let mut plan = common::Plan::default();

    // Loaded once group is, so rows know who's in it.
    plan.after(2, move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("AddMembersPage.qml"))
            )
        );
        // Nothing read yet. Empty model: "no answer" early, "no contacts" later.
        record!("placeholder-fresh", get!("contactsPlaceholder", "enabled"));
    });

    plan.after(2, move || unsafe {
        // Ada already in; Grace not.
        record!("in-greyed", get!("memberRow10", "enabled"));
        record!("out-offered", get!("memberRow11", "enabled"));
        record!("nothing-picked", get!("addButton", "enabled"));
        // Tapping someone in does nothing.
        record!("tap-in", call!("click", QString::from("memberRow10")));
        record!("still-nothing", get!("addButton", "enabled"));
        // Search narrows to Grace, found by address.
        record!(
            "search",
            call!(
                "setText",
                QString::from("searchField"),
                QString::from("grace")
            )
        );
    });

    plan.after(2, move || unsafe {
        record!("ada-hidden", get!("memberRow10", "enabled"));
        record!("grace-found", get!("memberRow11", "enabled"));
        record!("tap-out", call!("click", QString::from("memberRow11")));
        record!("picked", get!("addButton", "enabled"));
        record!("add", call!("click", QString::from("addButton")));
        // Search no contact answers: list really empty.
        record!(
            "search-nobody",
            call!(
                "setText",
                QString::from("searchField"),
                QString::from("zzqq-nobody-is-called-this")
            )
        );
    });

    // Core answered, nobody to show: placeholder is the only thing on list.
    plan.after(2, move || unsafe {
        record!("placeholder-empty", get!("contactsPlaceholder", "enabled"));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    assert_outcome(
        &steps,
        &stack_box.pinned().borrow().log.to_string(),
        &common::calls(&journal),
    );
}

/// Only the one not in could be picked; picking added them and went back.
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

    for label in [
        "load",
        "tap-in",
        "search",
        "tap-out",
        "add",
        "search-nobody",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("placeholder-fresh"),
        "false",
        "\"No contacts to add\" is up before the core has said whether \
         there are any, so it flashes under the rows on the way in. {context}"
    );
    assert_eq!(
        value("placeholder-empty"),
        "true",
        "a search nobody answers to says nothing at all, which reads as a \
         list that has not loaded. {context}"
    );
    assert!(
        value("ada-hidden").starts_with("missing:"),
        "searching for Grace still lists Ada. {context}"
    );
    assert_eq!(
        value("grace-found"),
        "true",
        "searching for Grace did not find her, or found her greyed. {context}"
    );
    let grace = serde_json::json!([1, 0, "grace"]);
    assert!(
        calls
            .iter()
            .any(|(name, params)| name == "get_contacts" && *params == grace),
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
