//! The group page shows what the core holds and writes the name back.
//!
//! Loaded headlessly against the stub Silica module and the recording
//! double: the members come up, the name is filled in from the core rather
//! than the page that opened it, typing a new one and leaving saves it, a
//! blanked one goes back to the group's rather than to the core, leaving
//! the group is asked about on a page of its own, and a chat that is not
//! a group offers no edits at all.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded rather than performed: only which page
/// leaving the group opens is asked about.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:LeaveGroupDialog.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap) -> QVariant),
}

impl PageStackProbe {
    fn push(&mut self, page: QString, _properties: QVariantMap) -> QVariant {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page);
        let current = self.log.to_string();
        self.log = format!("{current}push:{name}|").into();
        self.log_changed();
        QVariant::default()
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, chatId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: 1, chatId: chatId, chatName: 'from the list' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // `data` rather than `children`: the model is a plain QObject, so
        // it is not among an Item's visual children at all.
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
            // A ComboBox's items live in its menu, which is not among
            // its children.
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
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// Leaving the page is what applies what was typed. Back on
        /// screen first: the status has to change for the page to notice.
        function leave() {
            loader.item.status = PageStatus.Active
            loader.item.status = PageStatus.Deactivating
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_group_page_shows_the_group_and_renames_it() {
    let temp = std::env::temp_dir().join(format!("piirit-group-page-{}", std::process::id()));
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

    // The group: chat 2, with the account itself and one contact in it.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::page_url("GroupPage.qml")), 2)
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("loaded", get!("chat", "loaded"));
        record!("name", get!("groupNameField", "text"));
        record!("editable", get!("groupNameField", "readOnly"));
        record!("members", get!("membersHeader", "text"));
        record!("self-row", get!("memberRow1", "objectName"));
        record!("contact-row", get!("memberRow10", "objectName"));
        record!("add-offered", get!("addMembersButton", "visible"));
        record!("leave-offered", get!("leaveButton", "visible"));
        record!("menu-offered", get!("groupMenu", "visible"));
        record!("badge", get!("editBadge", "visible"));
        record!("remove-picture", get!("removePicture", "visible"));
        // Disappearing messages: off, as the core holds it, and a tap on
        // a duration goes to the core.
        record!("timer-off", get!("disappearingCombo", "currentIndex"));
        record!("timer-offered", get!("disappearingCombo", "enabled"));
        record!(
            "timer-pick",
            call!("click", QString::from("timerOption86400"))
        );
        // A new name, applied on the way out rather than a pause later.
        record!(
            "typed",
            call!(
                "setText",
                QString::from("groupNameField"),
                QString::from("Hikers")
            )
        );
        record!("leave", call!("leave"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        // A blanked name goes back to the group's rather than to the
        // core as nothing.
        record!(
            "blanked",
            call!(
                "setText",
                QString::from("groupNameField"),
                QString::from("")
            )
        );
        record!("leave-blank", call!("leave"));
        record!("refilled", get!("groupNameField", "text"));
        // Leaving is asked about on a page of its own.
        record!(
            "confirm-leave",
            call!("click", QString::from("leaveButton"))
        );
        // A one-to-one chat: shown, but nothing about it can be changed
        // here.
        record!(
            "load-single",
            call!("load", QString::from(common::page_url("GroupPage.qml")), 1)
        );
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("single-loaded", get!("chat", "loaded"));
        record!("single-readonly", get!("groupNameField", "readOnly"));
        record!("single-add", get!("addMembersButton", "visible"));
        record!("single-leave", get!("leaveButton", "visible"));
        // Nothing on the menu applies, so the menu itself is not there:
        // an empty pull-down is a pull that does nothing.
        record!("single-menu", get!("groupMenu", "visible"));
        record!("single-badge", get!("editBadge", "visible"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_page(&steps, &navigation, &common::calls(&journal));
}

/// What was shown came from the core, and what was typed went back to it.
#[allow(clippy::too_many_lines)]
fn assert_page(steps: &[(&str, String)], navigation: &str, calls: &[(String, Value)]) {
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
        "typed",
        "leave",
        "blanked",
        "leave-blank",
        "confirm-leave",
        "load-single",
        "timer-pick",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("refilled"),
        "Hikers",
        "a blanked name was not put back to the group's own. {context}"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|(name, _)| name == "set_chat_name")
            .count(),
        1,
        "a blanked name reached the core, or the typed one did not. {context}"
    );
    assert_eq!(
        navigation, "push:LeaveGroupDialog.qml|",
        "leaving the group is not asked about on a page of its own. {context}"
    );
    assert_eq!(
        value("timer-off"),
        "0",
        "a chat with no timer does not show Off. {context}"
    );
    assert_eq!(
        value("timer-offered"),
        "true",
        "the timer cannot be changed on a group the account is in. {context}"
    );
    let timer = calls
        .iter()
        .find(|(name, _)| name == "set_chat_ephemeral_timer")
        .map(|(_, params)| params.clone())
        .unwrap_or_default();
    assert_eq!(
        timer,
        serde_json::json!([1, 2, 86400]),
        "picking a duration did not reach the core. {context}"
    );
    assert_eq!(value("loaded"), "true", "the group never loaded. {context}");
    assert_eq!(
        value("name"),
        "chat 2",
        "the name field shows something other than the core's name. {context}"
    );
    assert_eq!(
        value("editable"),
        "false",
        "a group the account is in cannot be renamed. {context}"
    );
    assert!(
        value("members").starts_with('2'),
        "the heading does not count the two members. {context}"
    );
    assert_eq!(
        value("self-row"),
        "memberRow1",
        "the account's own row is missing. {context}"
    );
    assert_eq!(
        value("contact-row"),
        "memberRow10",
        "the other member's row is missing. {context}"
    );
    for label in ["add-offered", "leave-offered", "menu-offered", "badge"] {
        assert_eq!(
            value(label),
            "true",
            "{label} is not offered on a group the account is in. {context}"
        );
    }
    assert_eq!(
        value("remove-picture"),
        "false",
        "a group with no picture offers to remove one. {context}"
    );

    let rename = calls
        .iter()
        .find(|(name, _)| name == "set_chat_name")
        .map(|(_, params)| params.clone())
        .unwrap_or_default();
    assert_eq!(
        rename,
        serde_json::json!([1, 2, "Hikers"]),
        "leaving the page did not save the typed name. {context}"
    );

    assert_eq!(
        value("single-loaded"),
        "true",
        "the one-to-one chat never loaded. {context}"
    );
    assert_eq!(
        value("single-readonly"),
        "true",
        "a one-to-one chat offers renaming, which the core refuses. {context}"
    );
    for label in ["single-add", "single-leave", "single-menu", "single-badge"] {
        assert_eq!(
            value(label),
            "false",
            "{label} is offered on a chat that is not a group. {context}"
        );
    }
}
