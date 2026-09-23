//! The cover's quick actions in the settings: on the settings page, one
//! row under Advanced, beside the webxdc switch, opening the page they
//! are set up on; on that page, a short word on what they are, then two
//! pictures of the cover to choose between -- room for one action, or for
//! two -- then what each action does.
//!
//! Each writes what it does to the settings, and shows what the settings
//! hold; the pictures show it too, redrawn as it changes. A chat is picked
//! on the chat picker, and the action becomes a chat's only once one has
//! been picked; the choice then says which chat by the name the core has
//! for it now, lets the icon be chosen, and says so when the chat has been
//! deleted since. With room for one, the right action is hidden and kept.

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
        function count() { return '' + Settings.quickActionCount }
        function holds(side) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            return Settings[prefix] + '|' + Settings[prefix + 'Account'] + '|'
                   + Settings[prefix + 'Chat'] + '|' + Settings[prefix + 'Icon']
        }
        // The icon a picture of the cover shows in an action's place: the
        // file's name, or '' for the dot of one not chosen yet.
        function previewIcon(preview, index) {
            var picture = findIn(loader.item, preview)
            if (!picture) { return 'missing:' + preview }
            var spot = findIn(picture, 'previewAction' + index)
            if (!spot) { return 'missing:previewAction' + index }
            return ('' + spot.source).split('/').pop()
        }
        function clear() {
            Settings.quickActionCount = 1
            Settings.quickActionLeft = ''
            Settings.quickActionLeftAccount = 0
            Settings.quickActionLeftChat = 0
            Settings.quickActionLeftIcon = ''
            Settings.quickActionRight = ''
            Settings.quickActionRightAccount = 0
            Settings.quickActionRightChat = 0
            Settings.quickActionRightIcon = ''
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

        // On the settings page: one row under Advanced, then the webxdc
        // switch, and nothing after; no heading of their own any more.
        record!(
            "under-advanced",
            call!("underHeading", QString::from("Advanced"))
        );
        record!(
            "after-entry",
            call!("after", QString::from("quickActionsEntry"))
        );
        record!(
            "after-webxdc",
            call!("after", QString::from("webxdcSwitch"))
        );
        record!(
            "old-heading",
            call!("underHeading", QString::from("Quick actions"))
        );
        record!("apps-heading", call!("underHeading", QString::from("Apps")));
        record!("open", click!("quickActionsEntry"));
        record!("opened", call!("stackLog"));

        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("QuickActionsPage.qml"))
            )
        );

        // On their page: the word on what they are, the pictures, then
        // the actions.
        record!(
            "after-header",
            call!("after", QString::from("quickActionsHeader"))
        );
        record!(
            "after-words",
            call!("after", QString::from("quickActionsExplained"))
        );
        record!(
            "after-choices",
            call!("after", QString::from("quickActionCountChoices"))
        );
        record!(
            "after-left",
            call!("after", QString::from("leftQuickAction"))
        );
        record!("words", get!("quickActionsExplained", "text"));

        // A phone that has never been asked: room for one, and that one
        // none.
        record!("one-chosen", get!("oneActionPreview", "selected"));
        record!("two-unchosen", get!("twoActionsPreview", "selected"));
        record!("one-label", get!("leftActionCombo", "label"));
        record!("right-hidden", get!("rightQuickAction", "visible"));
        record!("left-none", get!("leftActionCombo", "currentIndex"));
        record!("left-none-says", get!("leftActionCombo", "value"));
        record!("icons-hidden", get!("leftActionIcons", "visible"));
        record!(
            "one-dot",
            call!("previewIcon", QString::from("oneActionPreview"), 0)
        );

        // The kinds that need nothing more are written on the tap, and the
        // pictures follow.
        record!("pick-search", click!("leftAction-search"));
        record!("left-search", call!("holds", QString::from("left")));
        record!("left-search-shown", get!("leftActionCombo", "currentIndex"));
        record!("left-search-says", get!("leftActionCombo", "value"));
        record!(
            "one-search",
            call!("previewIcon", QString::from("oneActionPreview"), 0)
        );
        record!(
            "two-left-search",
            call!("previewIcon", QString::from("twoActionsPreview"), 0)
        );
        record!(
            "two-right-dot",
            call!("previewIcon", QString::from("twoActionsPreview"), 1)
        );

        // Room for two: the other picture chosen, a left and a right.
        record!("choose-two", click!("twoActionsPreview"));
        record!("count-two", call!("count"));
        record!("two-chosen", get!("twoActionsPreview", "selected"));
        record!("one-unchosen", get!("oneActionPreview", "selected"));
        record!("left-label", get!("leftActionCombo", "label"));
        record!("right-shown", get!("rightQuickAction", "visible"));
        record!("pick-qr", click!("rightAction-qr"));
        record!("right-qr-shown", get!("rightActionCombo", "currentIndex"));
        record!(
            "two-right-qr",
            call!("previewIcon", QString::from("twoActionsPreview"), 1)
        );
        record!("pick-scan", click!("rightAction-scan"));
        record!("right-scan", call!("holds", QString::from("right")));
        record!("right-scan-shown", get!("rightActionCombo", "currentIndex"));
        record!("pick-none", click!("rightAction-none"));
        record!("right-none-again", call!("holds", QString::from("right")));
        record!("right-none-shown", get!("rightActionCombo", "currentIndex"));
        record!("pick-qr-again", click!("rightAction-qr"));

        // A chat is asked for, and nothing changes until one is picked.
        record!("pick-chat", click!("leftAction-chat"));
        record!("asked", call!("stackLog"));
        record!("unpicked", call!("holds", QString::from("left")));
        record!("unpicked-shown", get!("leftActionCombo", "currentIndex"));
        record!("picked", call!("pick", 2, QString::from("chat 2")));
        record!("left-chat", call!("holds", QString::from("left")));
        record!("left-chat-shown", get!("leftActionCombo", "currentIndex"));
        record!("icons-shown", get!("leftActionIcons", "visible"));
        record!("heart-lit", get!("leftIcon-heart", "highlighted"));
        record!(
            "two-left-heart",
            call!("previewIcon", QString::from("twoActionsPreview"), 0)
        );

        // Another icon, one of the later ones.
        record!("pick-dog", click!("leftIcon-dog"));
        record!("left-dog", call!("holds", QString::from("left")));
        record!("dog-lit", get!("leftIcon-dog", "highlighted"));
        record!("heart-unlit", get!("leftIcon-heart", "highlighted"));
        record!(
            "two-left-dog",
            call!("previewIcon", QString::from("twoActionsPreview"), 0)
        );
    });

    // The chat by the name the core has for it, in the choice itself;
    // then it goes.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("chat-says", get!("leftActionCombo", "value"));
        record!("delete", call!("deleteChat", 1));
    });

    // Chosen again, "Chat" picks again: one that has gone.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("change-chat", click!("leftAction-chat"));
        record!("asked-again", call!("stackLog"));
        record!("picked-gone", call!("pick", 1, QString::from("chat 1")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("gone-says", get!("leftActionCombo", "value"));
        record!("left-gone", call!("holds", QString::from("left")));

        // Back to room for one: the right hidden, and kept.
        record!("choose-one", click!("oneActionPreview"));
        record!("count-one", call!("count"));
        record!("right-hidden-again", get!("rightQuickAction", "visible"));
        record!("right-kept", call!("holds", QString::from("right")));
        record!("one-label-again", get!("leftActionCombo", "label"));
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
        value("under-advanced"),
        "quickActionsEntry",
        "the quick actions are not the first thing under Advanced. {context}"
    );
    assert_eq!(
        value("after-entry"),
        "webxdcSwitch",
        "the webxdc switch is not under Advanced with them. {context}"
    );
    assert_eq!(
        value("after-webxdc"),
        "nothing",
        "something follows Advanced. {context}"
    );
    assert_eq!(
        value("old-heading"),
        "missing:Quick actions",
        "the quick actions still have a heading of their own. {context}"
    );
    assert_eq!(
        value("apps-heading"),
        "missing:Apps",
        "Apps is still a heading of its own. {context}"
    );
    assert_eq!(
        value("opened"),
        "push:QuickActionsPage.qml:1:undefined|",
        "the row does not open the quick actions' page for this profile. \
         {context}"
    );
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
    assert_eq!(
        value("after-words"),
        "quickActionCountChoices",
        "the pictures to choose between do not follow the explanation. \
         {context}"
    );
    assert_eq!(value("after-choices"), "leftQuickAction", "{context}");
    assert_eq!(value("after-left"), "rightQuickAction", "{context}");
    assert!(
        !value("words").is_empty(),
        "the explanation says nothing. {context}"
    );
    assert_eq!(
        value("one-chosen"),
        "true",
        "a fresh phone does not have room for one. {context}"
    );
    assert_eq!(value("two-unchosen"), "false", "{context}");
    assert_eq!(
        value("one-label"),
        "Action",
        "the one action is not called the action. {context}"
    );
    assert_eq!(
        value("right-hidden"),
        "false",
        "the right action shows with room for one. {context}"
    );
    assert_eq!(value("left-none"), "0", "{context}");
    assert_eq!(value("left-none-says"), "None", "{context}");
    assert_eq!(
        value("icons-hidden"),
        "false",
        "the icons show with no chat action. {context}"
    );
    assert_eq!(
        value("one-dot"),
        "",
        "the picture shows an icon for an action not chosen. {context}"
    );
    assert_eq!(value("left-search"), "search|0|0|", "{context}");
    assert_eq!(value("left-search-shown"), "2", "{context}");
    assert_eq!(value("left-search-says"), "Search", "{context}");
    for label in ["one-search", "two-left-search"] {
        assert!(
            value(label).starts_with("search-"),
            "a picture does not show the search ({label}). {context}"
        );
    }
    assert_eq!(
        value("two-right-dot"),
        "",
        "the picture shows an icon for a right action not chosen. {context}"
    );
    assert_eq!(value("count-two"), "2", "{context}");
    assert_eq!(value("two-chosen"), "true", "{context}");
    assert_eq!(value("one-unchosen"), "false", "{context}");
    assert_eq!(value("left-label"), "Left", "{context}");
    assert_eq!(value("right-shown"), "true", "{context}");
    assert_eq!(value("right-qr-shown"), "3", "{context}");
    assert!(
        value("two-right-qr").starts_with("qr-"),
        "the picture does not follow the right action. {context}"
    );
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
    assert_eq!(value("icons-shown"), "true", "{context}");
    assert_eq!(value("heart-lit"), "true", "{context}");
    assert!(
        value("two-left-heart").starts_with("heart-"),
        "the picture does not show the chat's icon. {context}"
    );
    assert_eq!(value("left-dog"), "chat|1|2|dog", "{context}");
    assert_eq!(value("dog-lit"), "true", "{context}");
    assert_eq!(
        value("heart-unlit"),
        "false",
        "the icon picked before stays lit. {context}"
    );
    assert!(
        value("two-left-dog").starts_with("dog-"),
        "the picture does not follow the icon. {context}"
    );
    assert_eq!(
        value("chat-says"),
        "Chat: chat 2",
        "the choice does not say which chat it opens. {context}"
    );
    assert_eq!(
        value("asked-again"),
        "push:ChatPickerPage.qml:1:Choose a chat|",
        "choosing Chat again does not pick again. {context}"
    );
    assert_eq!(
        value("gone-says"),
        "Deleted chat",
        "an action whose chat has gone does not say so. {context}"
    );
    assert_eq!(
        value("left-gone"),
        "chat|1|1|dog",
        "picking another chat lost the icon. {context}"
    );
    assert_eq!(value("count-one"), "1", "{context}");
    assert_eq!(value("right-hidden-again"), "false", "{context}");
    assert_eq!(
        value("right-kept"),
        "qr|0|0|",
        "the right action was lost with room for one. {context}"
    );
    assert_eq!(value("one-label-again"), "Action", "{context}");
}
