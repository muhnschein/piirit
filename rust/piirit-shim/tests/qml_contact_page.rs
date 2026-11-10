//! Contact page of one-to-one chat: name, about line, connection state; never an email address.
//! Read first; acts from its pulley: nickname on its own page (saved once, on the way back;
//! blank goes back to theirs), delete chat, block contact, keys. Chat gone: conversation
//! under the page goes too.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe
        // Stack as far as the page uses it: conversation (chat 1) under it, list under that.
        property QtObject pageStack: QtObject {
            property string log: ''
            property var currentPage: null
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                log += 'push:' + name(page) + ':' + JSON.stringify(properties) + '|'
                return name(page) === 'BlockContactDialog.qml' ? probe.dialog : null
            }
            function completeAnimation() {}
            function previousPage(page) {
                return page === loader.item ? probe.conversation
                     : page === probe.conversation ? probe.list : null
            }
            function pop(page) { log += 'pop:' + page.objectName + '|' }
        }
        property QtObject conversation: QtObject { objectName: 'conversation'; property int chatId: 1 }
        property QtObject list: QtObject { objectName: 'list'; property int chatId: 0 }
        property QtObject dialog: QtObject { signal accepted() }
        property QtObject appWindow: QtObject {
            property string log: ''
            property var chatList: null
            function placeCall(accountId, chatId, video) {
                log += 'call:' + chatId + ':' + video + '|'
                return true
            }
            function showCall() { log += 'show|' }
        }
        Loader { id: loader }
        Loader { id: editLoader }
        function load(url, chatId) {
            loader.setSource(url, { accountId: 1, chatId: chatId, chatName: 'from the list' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Edit name page, as the pulley opens it: the contact page's own chat.
        function loadEdit(url) {
            editLoader.setSource(url, {
                chat: findIn(loader.item, 'chat'), contactId: loader.item.contactId,
                givenName: loader.item.givenName, ownName: loader.item.ownName,
                displayName: loader.item.displayName
            })
            return editLoader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        // `property` may be a path: 'icon.source'.
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + property.split('.').reduce(function(at, key) { return at[key] }, item)
        }
        function editGet(name, property) {
            var item = findIn(editLoader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function type(value) {
            var item = findIn(editLoader.item, 'nameField')
            if (!item) { return 'missing:nameField' }
            item.text = value
            return 'ok'
        }
        // A tap on a MouseArea: its clicked() carries the event, and
        // QML refuses to emit it without one.
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked(null)
            return 'ok'
        }
        /// Swipe back: Deactivating is what saves.
        function leaveEdit() {
            editLoader.item.status = PageStatus.Active
            editLoader.item.status = PageStatus.Deactivating
            return 'ok'
        }
        function block() { dialog.accepted(); return 'ok' }
        function stackLog() { var was = pageStack.log; pageStack.log = ''; return was }
        function callLog() { var was = appWindow.log; appWindow.log = ''; return was }
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
fn the_contact_page_names_the_contact_and_no_address() {
    let temp = std::env::temp_dir().join(format!("piirit-contact-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt.
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

    let page = common::page_url("ContactPage.qml");
    let edit = common::page_url("EditNamePage.qml");
    let mut plan = common::Plan::default();

    // Chat 1 is with Ada, who has written an about line.
    let url = page.clone();
    plan.after(1, move || unsafe {
        record!("load", call!("load", QString::from(url.clone()), 1));
    });

    let edit_first = edit.clone();
    plan.after(2, move || unsafe {
        record!("loaded", get!("chat", "loaded"));
        record!("name", get!("nameLabel", "name"));
        record!("own-name-shown", get!("ownNameLabel", "visible"));
        record!("initial", get!("avatarInitial", "text"));
        record!("status", get!("statusLabel", "text"));
        record!("status-shown", get!("statusLabel", "visible"));
        record!("seen", get!("seenLabel", "text"));
        record!("seen-shown", get!("seenLabel", "visible"));
        record!("encryption", get!("encryptionLabel", "text"));
        record!("timer", get!("disappearingCombo", "currentIndex"));
        record!("texts", call!("texts"));
        for entry in ["deleteChat", "blockContact", "encryptionInfo", "editName"] {
            record!(entry, get!(entry, "visible"));
        }
        record!("calls-shown", get!("callButtons", "visible"));
        record!("calls-row", get!("callsRow", "visible"));
        record!("call", call!("tap", QString::from("callButton")));
        record!("video-call", call!("tap", QString::from("videoCallButton")));
        record!("call-log", call!("callLog"));
        record!(
            "tap-encryption",
            call!("tap", QString::from("encryptionTap"))
        );
        for row in ["galleryRow", "audioRow", "filesRow"] {
            record!(row, call!("tap", QString::from(row)));
        }
        record!("paired", get!("doors", "columns"));
        record!("calls-x", get!("callsRow", "x"));
        record!("files-x", get!("filesRow", "x"));
        record!("call-icon", get!("callButtonIcon", "source"));
        record!("call-icon-size", get!("callButtonIcon", "sourceSize.width"));
        record!("call-shift", get!("callButton", "inkShift"));
        record!("video-shift", get!("videoCallButton", "inkShift"));
        record!(
            "call-offset",
            get!("callButtonContent", "anchors.horizontalCenterOffset")
        );
        record!("call-label", get!("callButtonLabel", "text"));
        record!(
            "menu-order",
            call!("menuOrder", QString::from("contactMenu"))
        );
        record!(
            "band",
            format!(
                "{},{}",
                get!("headerBand", "visible"),
                get!("headerBand", "y")
            )
        );
        record!(
            "band-gap",
            format!(
                "{},{}",
                get!("bandGap", "visible"),
                get!("bandGap", "height")
            )
        );
        record!("tap-edit", call!("tap", QString::from("editName")));
        record!("opened", call!("stackLog"));
        record!(
            "load-edit",
            call!("loadEdit", QString::from(edit_first.clone()))
        );
        record!(
            "field",
            call!("editGet", QString::from("nameField"), QString::from("text"))
        );
        record!(
            "placeholder",
            call!(
                "editGet",
                QString::from("nameField"),
                QString::from("placeholderText")
            )
        );
        record!(
            "label",
            call!(
                "editGet",
                QString::from("nameField"),
                QString::from("label")
            )
        );
        // Left without a change: nothing to send.
        record!("leave-unchanged", call!("leaveEdit"));
        record!("typed", call!("type", QString::from("  Countess ")));
        record!("leave", call!("leaveEdit"));
    });

    let edit_again = edit.clone();
    plan.after(2, move || unsafe {
        record!("renamed", get!("nameLabel", "name"));
        record!("own-name-after", get!("ownNameLabel", "text"));
        record!("own-name-after-shown", get!("ownNameLabel", "visible"));
        record!(
            "load-edit-again",
            call!("loadEdit", QString::from(edit_again.clone()))
        );
        record!(
            "field-again",
            call!("editGet", QString::from("nameField"), QString::from("text"))
        );
        // Nickname same as their own: nothing to tell apart.
        record!("same", call!("type", QString::from("ada")));
        record!("leave-same", call!("leaveEdit"));
    });

    let edit_last = edit;
    plan.after(2, move || unsafe {
        record!("own-name-same-shown", get!("ownNameLabel", "visible"));
        record!(
            "load-edit-last",
            call!("loadEdit", QString::from(edit_last.clone()))
        );
        record!("blanked", call!("type", QString::from("")));
        record!("leave-again", call!("leaveEdit"));
    });

    plan.after(2, move || unsafe {
        record!("theirs-again", get!("nameLabel", "name"));
        record!("own-name-gone", get!("ownNameLabel", "visible"));
        record!("delete", call!("tap", QString::from("deleteChat")));
    });

    // Countdown (stub: 50 ms), then core's delete.
    let url = page;
    plan.after(1, move || unsafe {
        record!("after-delete", call!("stackLog"));
        record!(
            "load-blocking",
            call!("load", QString::from(url.clone()), 2)
        );
    });

    plan.after(2, move || unsafe {
        record!("tap-block", call!("tap", QString::from("blockContact")));
        record!("asked-block", call!("stackLog"));
        record!("block", call!("block"));
    });

    plan.after(1, move || unsafe {
        record!("after-block", call!("stackLog"));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let calls = common::calls(&journal);
    assert_page(&steps);
    assert_actions(&steps, &calls);
    assert_rename(&steps, &calls);
}

fn value<'a>(steps: &'a [(&str, String)], label: &str) -> &'a str {
    steps
        .iter()
        .find(|(name, _)| *name == label)
        .map_or("<step did not run>", |(_, value)| value.as_str())
}

/// Nickname page: their own name in label and placeholder; sent once, trimmed, on the way
/// back; blank is the core's way back to theirs; unchanged sends nothing.
fn assert_rename(steps: &[(&str, String)], calls: &[(String, Value)]) {
    let context = format!("steps: {steps:?}");
    for step in [
        "load-edit",
        "leave-unchanged",
        "typed",
        "leave",
        "load-edit-again",
        "same",
        "leave-same",
        "load-edit-last",
        "blanked",
    ] {
        assert_eq!(value(steps, step), "ok", "step {step} failed. {context}");
    }
    assert_eq!(
        (
            value(steps, "field"),
            value(steps, "placeholder"),
            value(steps, "label")
        ),
        ("", "ada", "Nickname for \"ada\""),
        "the nickname page does not start empty under the name they chose. {context}"
    );
    let renames: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "change_contact_name")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        renames.len(),
        3,
        "expected two renames and one blanking on the wire, none for leaving \
         unchanged: {calls:?}"
    );
    assert_eq!(
        (
            renames[0].pointer("/1").and_then(Value::as_u64),
            renames[0].pointer("/2").and_then(Value::as_str)
        ),
        (Some(10), Some("Countess")),
        "the typed name did not reach the core for the contact, trimmed: {renames:?}"
    );
    assert_eq!(
        value(steps, "own-name-same-shown"),
        "false",
        "the page shows the contact's own name under a nickname that is the same. {context}"
    );
    assert_eq!(
        renames[2].pointer("/2").and_then(Value::as_str),
        Some(""),
        "a blanked field did not reach the core as an empty name, which is \
         what puts the contact's own back: {renames:?}"
    );
    assert_eq!(
        (
            value(steps, "renamed"),
            value(steps, "own-name-after"),
            value(steps, "own-name-after-shown")
        ),
        ("Countess", "Calls themselves ada", "true"),
        "the page does not show the nickname, with the name they chose under it. {context}"
    );
    assert_eq!(
        value(steps, "field-again"),
        "Countess",
        "the nickname page does not hold the nickname given. {context}"
    );
    assert_eq!(
        (value(steps, "theirs-again"), value(steps, "own-name-gone")),
        ("ada", "false"),
        "blanking the name did not go back to the contact's own. {context}"
    );
}

/// Pulley, call buttons, rows: each does what it says.
fn assert_actions(steps: &[(&str, String)], calls: &[(String, Value)]) {
    let context = format!("steps: {steps:?}");
    for entry in ["deleteChat", "blockContact", "encryptionInfo", "editName"] {
        assert_eq!(
            value(steps, entry),
            "true",
            "pulley has no {entry}. {context}"
        );
    }
    assert_eq!(
        (
            value(steps, "calls-shown"),
            value(steps, "calls-row"),
            value(steps, "call-log")
        ),
        ("true", "true", "call:1:false|call:1:true|"),
        "Call and Video call do not call this chat, or no Calls row. {context}"
    );
    let opened = value(steps, "opened");
    assert!(
        opened.contains(
            "push:EncryptionPage.qml:{\"accountId\":1,\"contactId\":10,\"name\":\"ada\"}|"
        ),
        "the encryption line does not open the contact's keys. {context}"
    );
    for kind in ["gallery", "audio", "files"] {
        assert!(
            opened.contains(&format!(
                "push:ChatMediaPage.qml:{{\"accountId\":1,\"chatId\":1,\"kind\":\"{kind}\"}}|"
            )),
            "the {kind} row does not open that kind's own page. {context}"
        );
    }
    // Two to a line: Files ends the first pair's line, Calls starts no line.
    assert_eq!(
        (
            value(steps, "paired"),
            value(steps, "files-x"),
            value(steps, "calls-x")
        ),
        ("2", "0", "270"),
        "the rows in this chat are not two to a line. {context}"
    );
    assert_eq!(
        (value(steps, "call-icon"), value(steps, "call-label")),
        ("image://theme/icon-m-call", "Call"),
        "Call button has no call icon or label. {context}"
    );
    // Stub Theme: iconSizeSmallPlus, dp(n) = n.
    assert_eq!(
        (
            value(steps, "call-icon-size"),
            value(steps, "call-shift"),
            value(steps, "video-shift"),
            value(steps, "call-offset")
        ),
        ("48", "2", "1", "-2"),
        "call icons not small, or icon and label not pulled to the button's centre. {context}"
    );
    assert_eq!(
        value(steps, "band"),
        "true,0",
        "no ambience band behind the contact's top block. {context}"
    );
    assert_eq!(
        value(steps, "menu-order"),
        "encryptionInfo|deleteChat|blockContact|editName|",
        "the pulley is not encryption info, delete, block, edit name, top to bottom. {context}"
    );
    assert!(
        opened.contains("push:EditNamePage.qml:"),
        "Edit name does not open its page. {context}"
    );
    assert_eq!(
        (value(steps, "delete"), value(steps, "after-delete")),
        ("ok", "pop:list|"),
        "deleting the chat did not go back past its conversation. {context}"
    );
    let deleted: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "delete_chat")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        deleted.len(),
        1,
        "the chat was not deleted once, after the countdown: {calls:?}"
    );
    assert_eq!(deleted[0].pointer("/1").and_then(Value::as_u64), Some(1));
    assert!(
        value(steps, "asked-block").contains("push:BlockContactDialog.qml:"),
        "Block contact does not ask first. {context}"
    );
    assert_eq!(
        (value(steps, "block"), value(steps, "after-block")),
        ("ok", "pop:conversation|"),
        "blocking did not leave the page. {context}"
    );
    assert!(
        calls.iter().any(|(name, params)| name == "block_contact"
            && params.pointer("/1").and_then(Value::as_u64) == Some(10)),
        "the contact was not blocked: {calls:?}"
    );
}

/// Contact's words, picture, connection state; no address text.
fn assert_page(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    assert_eq!(
        value(steps, "load"),
        "ok",
        "the contact page did not load. {context}"
    );
    assert_eq!(
        value(steps, "loaded"),
        "true",
        "the chat never loaded. {context}"
    );
    assert_eq!(
        value(steps, "band-gap"),
        "true,4",
        "no extra air between the band and the first heading. {context}"
    );
    assert_eq!(
        (value(steps, "name"), value(steps, "own-name-shown")),
        ("ada", "false"),
        "the page does not name the contact, or repeats their own name. {context}"
    );
    assert_eq!(
        value(steps, "initial"),
        "A",
        "the avatar is not standing in with the contact's initial. {context}"
    );
    assert_eq!(
        (value(steps, "status"), value(steps, "status-shown")),
        ("Poet and mathematician", "true"),
        "the contact's own line about themselves is missing. {context}"
    );
    assert_eq!(
        value(steps, "encryption"),
        "End-to-end encrypted",
        "a contact that can be encrypted to is not said to be. {context}"
    );
    assert!(
        !value(steps, "texts").contains('@'),
        "an email address is drawn on the contact page. {context}"
    );
    // Ada never seen: core says "Old".
    assert_eq!(
        (value(steps, "seen-shown"), value(steps, "seen")),
        ("true", "Seen a long time ago"),
        "a contact the core has not seen for a long time is not said to be. {context}"
    );
    assert_eq!(
        value(steps, "timer"),
        "0",
        "the contact page does not offer disappearing messages, or shows \
         something other than Off for a chat with no timer. {context}"
    );
}
