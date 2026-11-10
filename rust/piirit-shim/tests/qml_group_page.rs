//! Group page shows what core holds: name, count, description, first members. Edit group page
//! writes name and description back on the way out; blanked name reverts to group's, not sent to
//! core empty. Leaving group asked on own page; once left, Delete chat stands in its place.
//! Non-group chat: no edits.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    // `qt_method!` params match dispatcher: piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica `pageStack`, recorded: only which page leaving opens.
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
        Loader { id: editLoader }
        /// Edit group page, as the pulley opens it: on the group page's own chat.
        function loadEdit(url) {
            editLoader.setSource('', {})
            editLoader.setSource(url, { chat: findIn(loader.item, 'chat') })
            return editLoader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function editGet(name, property) {
            var item = findIn(editLoader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function editSet(name, value) {
            var item = findIn(editLoader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        /// Swipe back: Deactivating is what saves.
        function leaveEdit() {
            editLoader.item.status = PageStatus.Active
            editLoader.item.status = PageStatus.Deactivating
            return 'ok'
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
        /// A method of the model, called by name: leaving the group is
        /// what the page asks the model for after its confirmation page.
        function ask(name, method) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item[method]()
            return 'ok'
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        // Pulley entries top to bottom, as declared: objectNames.
        function menuOrder(name) {
            var walk = function(node) {
                var out = node.text !== undefined && node.objectName ? node.objectName + '|' : ''
                var kids = node.data !== undefined ? node.data : node.children
                for (var i = 0; kids && i < kids.length; i++) { out += walk(kids[i]) }
                return out
            }
            var menu = findIn(loader.item, name)
            return menu ? walk(menu) : 'missing:' + name
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_group_page_shows_the_group_and_renames_it() {
    let temp = std::env::temp_dir().join(format!("piirit-group-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_DESCRIPTION", "Walks\nBring boots");
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
    let page = common::page_url("GroupPage.qml");
    let edit = common::page_url("EditGroupPage.qml");

    // Group: chat 2, account and one contact.
    let url = page.clone();
    plan.after(1, move || unsafe {
        record!("load", call!("load", QString::from(url.clone()), 2));
    });

    let edit_url = edit.clone();
    plan.after(2, move || unsafe {
        record!("loaded", get!("chat", "loaded"));
        record!("name", get!("nameLabel", "name"));
        record!("members", get!("memberCount", "text"));
        record!("member-names", get!("memberNames", "name"));
        record!("members-row", get!("membersRow", "visible"));
        // Add members and Invite moved to the members page's pulley.
        record!("add-row", get!("addMembersRow", "visible"));
        record!("invite-row", get!("inviteRow", "visible"));
        record!("menu-order", call!("menuOrder", QString::from("groupMenu")));
        record!("band", get!("headerBand", "visible"));
        record!(
            "band-gap",
            format!(
                "{},{}",
                get!("bandGap", "visible"),
                get!("bandGap", "height")
            )
        );
        record!("leave-offered", get!("leaveButton", "visible"));
        record!("menu-offered", get!("groupMenu", "visible"));
        record!("edit-offered", get!("editGroup", "visible"));
        record!("delete-offered", get!("deleteChat", "visible"));
        // Members row, a media row: each opens its page.
        for row in ["membersRow", "galleryRow"] {
            record!(row, call!("click", QString::from(row)));
        }
        // Disappearing messages: off per core; duration tap goes to core.
        record!("timer-off", get!("disappearingCombo", "currentIndex"));
        record!("timer-offered", get!("disappearingCombo", "enabled"));
        record!(
            "timer-pick",
            call!("click", QString::from("timerOption86400"))
        );
        // Description: line break shown.
        record!("desc-visible", get!("descriptionLabel", "visible"));
        record!("desc-text", get!("descriptionLabel", "text"));
        record!(
            "load-edit",
            call!("loadEdit", QString::from(edit_url.clone()))
        );
        record!(
            "edit-name",
            call!(
                "editGet",
                QString::from("groupNameField"),
                QString::from("text")
            )
        );
        record!(
            "edit-desc",
            call!(
                "editGet",
                QString::from("descriptionField"),
                QString::from("text")
            )
        );
        record!(
            "change-picture",
            call!(
                "editGet",
                QString::from("changePicture"),
                QString::from("visible")
            )
        );
        record!(
            "remove-picture",
            call!(
                "editGet",
                QString::from("removePicture"),
                QString::from("visible")
            )
        );
        // New ones applied on way out, together.
        record!(
            "desc-typed",
            call!(
                "editSet",
                QString::from("descriptionField"),
                QString::from("  Bring water\nand boots \n")
            )
        );
        record!(
            "typed",
            call!(
                "editSet",
                QString::from("groupNameField"),
                QString::from("Hikers")
            )
        );
        record!("leave", call!("leaveEdit"));
    });

    let edit_url = edit.clone();
    plan.after(2, move || unsafe {
        // Sent value comes back trimmed; page shows that.
        record!("desc-saved", get!("descriptionLabel", "text"));
        record!("renamed", get!("nameLabel", "name"));
        record!(
            "load-edit-again",
            call!("loadEdit", QString::from(edit_url.clone()))
        );
        record!(
            "desc-cleared",
            call!(
                "editSet",
                QString::from("descriptionField"),
                QString::from("")
            )
        );
        // Blanked name reverts to group's, not sent to core as nothing.
        record!(
            "blanked",
            call!(
                "editSet",
                QString::from("groupNameField"),
                QString::from("")
            )
        );
        record!("leave-blank", call!("leaveEdit"));
        // Leave asked on own page.
        record!(
            "confirm-leave",
            call!("click", QString::from("leaveButton"))
        );
    });

    let edit_url = edit.clone();
    plan.after(2, move || unsafe {
        record!("refilled", get!("nameLabel", "name"));
        record!("blank-error", get!("errorBanner", "text"));
        record!("empty-visible", get!("descriptionLabel", "visible"));
        record!(
            "load-edit-empty",
            call!("loadEdit", QString::from(edit_url.clone()))
        );
        // Empty description: field there to write one.
        record!(
            "empty-field",
            call!(
                "editGet",
                QString::from("descriptionField"),
                QString::from("visible")
            )
        );
        record!(
            "desc-retyped",
            call!(
                "editSet",
                QString::from("descriptionField"),
                QString::from("Back soon")
            )
        );
        record!("leave-again", call!("leaveEdit"));
    });

    plan.after(2, move || unsafe {
        record!(
            "leave-group",
            call!("ask", QString::from("chat"), QString::from("leave"))
        );
    });

    let url = page;
    plan.after(2, move || unsafe {
        // After leaving: description readable; edits gone, Delete chat instead of Leave.
        record!("left-visible", get!("descriptionLabel", "visible"));
        record!("left-text", get!("descriptionLabel", "text"));
        record!("left-edit", get!("editGroup", "visible"));
        record!("left-leave", get!("leaveButton", "visible"));
        record!("left-delete", get!("deleteChat", "visible"));
        // One-to-one chat: shown, nothing changeable.
        record!("load-single", call!("load", QString::from(url.clone()), 1));
    });

    plan.after(2, move || unsafe {
        record!("single-loaded", get!("chat", "loaded"));
        record!("single-leave", get!("leaveButton", "visible"));
        record!("single-edit", get!("editGroup", "visible"));
        record!("single-desc", get!("descriptionLabel", "visible"));
        record!(
            "load-edit-single",
            call!("loadEdit", QString::from(edit.clone()))
        );
        record!(
            "single-change-picture",
            call!(
                "editGet",
                QString::from("pictureMenu"),
                QString::from("visible")
            )
        );
        record!(
            "single-name-field",
            call!(
                "editGet",
                QString::from("groupNameField"),
                QString::from("visible")
            )
        );
        record!(
            "single-desc-field",
            call!(
                "editGet",
                QString::from("descriptionField"),
                QString::from("visible")
            )
        );
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_page(&steps, &navigation, &common::calls(&journal));
}

/// Shown from core; typed value sent back to it.
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
        "load-edit",
        "typed",
        "leave",
        "load-edit-again",
        "blanked",
        "leave-blank",
        "confirm-leave",
        "load-single",
        "timer-pick",
        "membersRow",
        "galleryRow",
        "desc-typed",
        "desc-cleared",
        "load-edit-empty",
        "desc-retyped",
        "leave-again",
        "leave-group",
        "load-edit-single",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(value("loaded"), "true", "the group never loaded. {context}");
    assert_eq!(
        (value("name"), value("members")),
        ("chat 2".to_string(), "2 member(s)".to_string()),
        "the page does not name the group and count its two members. {context}"
    );
    assert_eq!(
        (value("members-row"), value("member-names")),
        ("true".to_string(), "ada".to_string()),
        "the members row does not name the other member, without the reader. {context}"
    );
    assert_eq!(
        (value("add-row"), value("invite-row")),
        (
            "missing:addMembersRow".to_string(),
            "missing:inviteRow".to_string()
        ),
        "Add members or Invite still a row on the group page. {context}"
    );
    assert_eq!(
        value("band"),
        "true",
        "no ambience band behind the group's top block. {context}"
    );
    assert_eq!(
        value("band-gap"),
        "true,4",
        "no extra air between the band and the first heading. {context}"
    );
    // Top to bottom; Delete chat only once left.
    assert_eq!(
        value("menu-order"),
        "encryptionInfo|leaveButton|deleteChat|editGroup|",
        "the pulley is not encryption info, leave, edit, top to bottom. {context}"
    );
    for label in [
        "leave-offered",
        "menu-offered",
        "edit-offered",
        "change-picture",
    ] {
        assert_eq!(
            value(label),
            "true",
            "{label} is not offered on a group the account is in. {context}"
        );
    }
    assert_eq!(
        (value("delete-offered"), value("remove-picture")),
        ("false".to_string(), "false".to_string()),
        "a group the account is in offers Delete chat, or one with no picture \
         offers to remove it. {context}"
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

    // Description: shown as core holds it, line break kept; edited on its page, sent trimmed
    // and came back; read-only after leaving.
    assert_eq!(
        (value("desc-visible"), value("desc-text")),
        ("true".to_string(), "Walks\nBring boots".to_string()),
        "the description is not the core's, line break and all. {context}"
    );
    assert_eq!(
        (value("edit-name"), value("edit-desc")),
        ("chat 2".to_string(), "Walks\nBring boots".to_string()),
        "the edit page does not start from the core's name and description. {context}"
    );
    assert_eq!(
        (value("desc-saved"), value("renamed")),
        ("Bring water\nand boots".to_string(), "Hikers".to_string()),
        "the saved name and description did not come back. {context}"
    );
    assert_eq!(
        value("refilled"),
        "Hikers",
        "a blanked name did not keep the group's own. {context}"
    );
    assert_eq!(
        value("blank-error"),
        "",
        "a blanked name was sent anyway and core's refusal shown. {context}"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|(name, _)| name == "set_chat_name")
            .map(|(_, params)| params.clone())
            .collect::<Vec<_>>(),
        vec![serde_json::json!([1, 2, "Hikers"])],
        "a blanked name reached the core, or the typed one did not. {context}"
    );
    assert_eq!(
        (value("empty-visible"), value("empty-field")),
        ("false".to_string(), "true".to_string()),
        "an empty description is drawn, or the edit page gives no field to \
         write one. {context}"
    );
    let described: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "set_chat_description")
        .map(|(_, params)| params.clone())
        .collect();
    assert_eq!(
        described,
        vec![
            serde_json::json!([1, 2, "Bring water\nand boots"]),
            serde_json::json!([1, 2, ""]),
            serde_json::json!([1, 2, "Back soon"]),
        ],
        "leaving the edit page did not send each typed description, trimmed. {context}"
    );
    assert_eq!(
        navigation, "push:MembersPage.qml|push:ChatMediaPage.qml|push:LeaveGroupDialog.qml|",
        "the members row or Gallery opens no page of its own, or leaving the group is not \
         asked about on one. {context}"
    );
    assert_eq!(
        (value("left-visible"), value("left-text")),
        ("true".to_string(), "Back soon".to_string()),
        "a description is hidden once the group is left. {context}"
    );
    assert_eq!(
        (
            value("left-edit"),
            value("left-leave"),
            value("left-delete")
        ),
        ("false".to_string(), "false".to_string(), "true".to_string()),
        "a group the account has left still offers edits, or no Delete chat. {context}"
    );

    assert_eq!(
        value("single-loaded"),
        "true",
        "the one-to-one chat never loaded. {context}"
    );
    for label in [
        "single-leave",
        "single-edit",
        "single-desc",
        "single-change-picture",
        "single-name-field",
        "single-desc-field",
    ] {
        assert_eq!(
            value(label),
            "false",
            "{label} is offered on a chat that is not a group. {context}"
        );
    }
}
