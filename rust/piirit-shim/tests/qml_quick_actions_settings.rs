//! The cover's quick actions in the settings: on the settings page, above
//! Apps, one row a side saying what it does, either opening the page they
//! are set up on; on that page, a short word on what they are, then the
//! left one and the right.
//!
//! Each writes what it does to the settings, and shows what the settings
//! hold. A chat is picked on the chat picker, and the action becomes a
//! chat's only once one has been picked; it then says which chat by the
//! name the core has for it now, lets the icon be chosen, and says so
//! when the chat has been deleted since.

// Qt harness: see qml_pages.rs.
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

/// The probe, with the components directory filled in; see
/// `qml_general_settings.rs`.
fn probe_qml() -> String {
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    import 'file://__COMPONENTS__'
    Item {
        id: probe

        // The page stack, handing back a picker the probe can answer for.
        property QtObject pageStack: QtObject {
            property string log: ''
            property QtObject picker: QtObject {
                signal chatPicked(int chatId, string chatName)
            }
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                log += 'push:' + name(page) + ':' + properties.accountId + ':'
                       + properties.title + '|'
                return picker
            }
        }

        // For deleting a chat out from under an action.
        ChatList { id: helper; account_id: 1 }

        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { accountId: 1 })
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
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        // What the settings hold for one side, as kind|account|chat|icon.
        function holds(side) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            return Settings[prefix] + '|' + Settings[prefix + 'Account'] + '|'
                   + Settings[prefix + 'Chat'] + '|' + Settings[prefix + 'Icon']
        }
        function clear() {
            Settings.quickActionLeft = ''
            Settings.quickActionLeftAccount = 0
            Settings.quickActionLeftChat = 0
            Settings.quickActionLeftIcon = ''
            Settings.quickActionRight = ''
            return 'ok'
        }
        function pick(chatId, chatName) {
            pageStack.picker.chatPicked(chatId, chatName)
            return 'ok'
        }
        function stackLog() {
            var log = pageStack.log
            pageStack.log = ''
            return log
        }
        function deleteChat(chatId) { helper.delete_chat(chatId); return 'ok' }
        // What stands after an item in the page's column: its objectName,
        // or the text of a heading.
        function after(name) {
            var item = findIn(loader.item, name)
            return item ? describe(next(item)) : 'missing:' + name
        }
        function underHeading(heading) {
            var header = findText(loader.item, heading)
            return header ? describe(next(header)) : 'missing:' + heading
        }
        function next(item) {
            var kids = item.parent.children
            for (var i = 0; i + 1 < kids.length; i++) {
                if (kids[i] === item) { return kids[i + 1] }
            }
            return null
        }
        function describe(item) {
            if (!item) { return 'nothing' }
            return item.objectName !== '' ? item.objectName : 'heading:' + item.text
        }
        function findText(node, text) {
            if (!node) { return null }
            if (node.text === text && node.objectName === '') { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findText(kids[i], text)
                if (hit) { return hit }
            }
            return null
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_quick_actions_are_set_up_on_a_page_of_their_own() {
    let temp =
        std::env::temp_dir().join(format!("piirit-qml-quick-settings-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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
    macro_rules! click {
        ($name:expr) => {
            call!("click", QString::from($name))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!("clear", call!("clear"));
        record!(
            "load-settings",
            call!("load", QString::from(common::page_url("SettingsPage.qml")))
        );

        // On the settings page: a row a side, right under their heading
        // and right above Apps, each saying what that side does.
        record!(
            "under-heading",
            call!("underHeading", QString::from("Quick actions"))
        );
        record!(
            "after-left-entry",
            call!("after", QString::from("leftQuickActionEntry"))
        );
        record!(
            "after-right-entry",
            call!("after", QString::from("rightQuickActionEntry"))
        );
        record!("left-entry-none", get!("leftQuickActionEntry", "value"));
        record!("right-entry-none", get!("rightQuickActionEntry", "value"));
        record!("open-left", click!("leftQuickActionEntry"));
        record!("opened-left", call!("stackLog"));
        record!("open-right", click!("rightQuickActionEntry"));
        record!("opened-right", call!("stackLog"));

        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("QuickActionsPage.qml"))
            )
        );

        // On their page: the word on what they are first, then left, then
        // right.
        record!(
            "after-header",
            call!("after", QString::from("quickActionsHeader"))
        );
        record!(
            "after-words",
            call!("after", QString::from("quickActionsExplained"))
        );
        record!(
            "after-left",
            call!("after", QString::from("leftQuickAction"))
        );
        record!(
            "after-right",
            call!("after", QString::from("rightQuickAction"))
        );
        record!("words", get!("quickActionsExplained", "text"));

        // Nothing on a phone that has never been asked.
        record!("left-none", get!("leftActionCombo", "currentIndex"));
        record!("right-none", get!("rightActionCombo", "currentIndex"));
        record!("chat-hidden", get!("leftActionChatButton", "visible"));
        record!("icons-hidden", get!("leftActionIcons", "visible"));

        // The kinds that need nothing more are written on the tap.
        record!("pick-search", click!("leftAction-search"));
        record!("left-search", call!("holds", QString::from("left")));
        record!("left-search-shown", get!("leftActionCombo", "currentIndex"));
        record!("pick-qr", click!("rightAction-qr"));
        record!("right-qr-shown", get!("rightActionCombo", "currentIndex"));
        record!("pick-scan", click!("rightAction-scan"));
        record!("right-scan", call!("holds", QString::from("right")));
        record!("right-scan-shown", get!("rightActionCombo", "currentIndex"));
        record!("pick-none", click!("rightAction-none"));
        record!("right-none-again", call!("holds", QString::from("right")));
        record!("right-none-shown", get!("rightActionCombo", "currentIndex"));

        // A chat is asked for, and nothing changes until one is picked.
        record!("pick-chat", click!("leftAction-chat"));
        record!("asked", call!("stackLog"));
        record!("unpicked", call!("holds", QString::from("left")));
        record!("unpicked-shown", get!("leftActionCombo", "currentIndex"));
        record!("picked", call!("pick", 2, QString::from("chat 2")));
        record!("left-chat", call!("holds", QString::from("left")));
        record!("left-chat-shown", get!("leftActionCombo", "currentIndex"));
        record!("chat-shown", get!("leftActionChatButton", "visible"));
        record!("icons-shown", get!("leftActionIcons", "visible"));
        record!("heart-lit", get!("leftIcon-heart", "highlighted"));

        // Another icon.
        record!("pick-star", click!("leftIcon-star"));
        record!("left-star", call!("holds", QString::from("left")));
        record!("star-lit", get!("leftIcon-star", "highlighted"));
        record!("heart-unlit", get!("leftIcon-heart", "highlighted"));
    });

    // The chat by the name the core has for it; then it goes.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("chat-name", get!("leftActionChatButton", "value"));
        record!("delete", call!("deleteChat", 1));
    });

    // Picked again, it is one that has gone.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("change-chat", click!("leftActionChatButton"));
        record!("asked-again", call!("stackLog"));
        record!("picked-gone", call!("pick", 1, QString::from("chat 1")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("gone-name", get!("leftActionChatButton", "value"));
        record!("left-gone", call!("holds", QString::from("left")));

        // Back on the settings page, the rows say what was set up.
        record!("pick-qr-again", click!("rightAction-qr"));
        record!(
            "reload-settings",
            call!("load", QString::from(common::page_url("SettingsPage.qml")))
        );
        record!("left-entry-chat", get!("leftQuickActionEntry", "value"));
        record!("right-entry-qr", get!("rightQuickActionEntry", "value"));
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
        value("load-settings"),
        "ok",
        "the settings page did not load. {context}"
    );
    assert_eq!(
        value("under-heading"),
        "leftQuickActionEntry",
        "the left action's row is not right under the quick actions' \
         heading. {context}"
    );
    assert_eq!(
        value("after-left-entry"),
        "rightQuickActionEntry",
        "{context}"
    );
    assert_eq!(
        value("after-right-entry"),
        "heading:Apps",
        "the quick actions are not right above Apps. {context}"
    );
    for label in ["left-entry-none", "right-entry-none"] {
        assert_eq!(
            value(label),
            "None",
            "a fresh phone's row does not say there is no action ({label}). \
             {context}"
        );
    }
    for label in ["opened-left", "opened-right"] {
        assert_eq!(
            value(label),
            "push:QuickActionsPage.qml:1:undefined|",
            "a row does not open the quick actions' page for this profile \
             ({label}). {context}"
        );
    }
    assert_eq!(
        value("load"),
        "ok",
        "the quick actions' page did not load. {context}"
    );
    assert_eq!(
        value("after-header"),
        "quickActionsExplained",
        "the quick actions are not explained right under the page's \
         header. {context}"
    );
    assert_eq!(value("after-words"), "leftQuickAction", "{context}");
    assert_eq!(value("after-left"), "rightQuickAction", "{context}");
    assert_eq!(
        value("after-right"),
        "nothing",
        "something follows the right action on its page. {context}"
    );
    assert!(
        !value("words").is_empty(),
        "the explanation says nothing. {context}"
    );
    for label in ["left-none", "right-none"] {
        assert_eq!(
            value(label),
            "0",
            "a fresh phone does not show no action ({label}). {context}"
        );
    }
    for label in ["chat-hidden", "icons-hidden"] {
        assert_eq!(
            value(label),
            "false",
            "a chat's controls show with no chat action ({label}). {context}"
        );
    }
    assert_eq!(value("left-search"), "search|0|0|", "{context}");
    assert_eq!(value("left-search-shown"), "2", "{context}");
    assert_eq!(value("right-qr-shown"), "3", "{context}");
    assert_eq!(value("right-scan"), "scan|0|0|", "{context}");
    assert_eq!(value("right-scan-shown"), "4", "{context}");
    assert_eq!(value("right-none-again"), "|0|0|", "{context}");
    assert_eq!(value("right-none-shown"), "0", "{context}");
    assert_eq!(
        value("asked"),
        "push:ChatPickerPage.qml:1:Choose a chat|",
        "choosing a chat does not ask which, from this profile. {context}"
    );
    assert_eq!(
        value("unpicked"),
        "search|0|0|",
        "the action changed before a chat was picked. {context}"
    );
    assert_eq!(
        value("unpicked-shown"),
        "2",
        "the choice shows a chat before one was picked. {context}"
    );
    assert_eq!(
        value("left-chat"),
        "chat|1|2|heart",
        "the picked chat is not the action's, with the first icon. {context}"
    );
    assert_eq!(value("left-chat-shown"), "1", "{context}");
    assert_eq!(value("chat-shown"), "true", "{context}");
    assert_eq!(value("icons-shown"), "true", "{context}");
    assert_eq!(value("heart-lit"), "true", "{context}");
    assert_eq!(value("left-star"), "chat|1|2|star", "{context}");
    assert_eq!(value("star-lit"), "true", "{context}");
    assert_eq!(value("heart-unlit"), "false", "{context}");
    assert_eq!(
        value("chat-name"),
        "chat 2",
        "the action does not say which chat it opens. {context}"
    );
    assert_eq!(
        value("asked-again"),
        "push:ChatPickerPage.qml:1:Choose a chat|",
        "the chat cannot be changed. {context}"
    );
    assert_eq!(
        value("gone-name"),
        "Deleted chat",
        "an action whose chat has gone does not say so. {context}"
    );
    assert_eq!(
        value("left-gone"),
        "chat|1|1|star",
        "picking another chat lost the icon. {context}"
    );
    assert_eq!(
        value("reload-settings"),
        "ok",
        "the settings page did not load again. {context}"
    );
    assert_eq!(
        value("left-entry-chat"),
        "Chat",
        "the left row does not say it opens a chat. {context}"
    );
    assert_eq!(
        value("right-entry-qr"),
        "My QR code",
        "the right row does not say it shows the QR code. {context}"
    );
}
