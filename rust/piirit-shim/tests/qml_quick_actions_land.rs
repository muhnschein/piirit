//! Where a quick action from the cover lands: the chat list, brought to
//! the front, doing what the action asks.
//!
//! The search empties and asks for the keyboard. The QR code opens on the
//! side the action names. A chat is looked up before it is opened, so one
//! deleted since the action was set up is said to be gone rather than
//! opened empty; a chat in another profile puts that profile's list in
//! place of the whole stack, and the chat is opened from it once it is on
//! screen.

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

        // The page stack and the window, as far as the chat list reads
        // them. Each move is logged with what it was handed.
        property QtObject pageStack: QtObject {
            property var currentPage: null
            property string log: ''
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                log += 'push:' + name(page) + ':' + JSON.stringify(properties) + '|'
                return null
            }
            function replaceAbove(target, page, properties) {
                log += 'replaceAbove:' + name(page) + ':' + JSON.stringify(properties) + '|'
            }
            function pop() { log += 'pop|' }
        }
        property QtObject appWindow: QtObject {
            property int accountId: 0
            property Item chatList: null
            property int raised: 0
            function activate() { raised += 1 }
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
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The list is the page on top, or something else is.
        function onTop(itIs) {
            pageStack.currentPage = itIs === 'true' ? loader.item : probe
            return 'ok'
        }
        function registered() {
            return appWindow.chatList === loader.item ? 'registered' : 'not registered'
        }
        function raised() { return '' + appWindow.raised }
        function act(kind, accountId, chatId) {
            loader.item.quickAction(kind, accountId, chatId)
            return 'ok'
        }
        function type(text) { findIn(loader.item, 'chatSearchField').text = text; return 'ok' }
        // The keyboard is on its way to the field: asked for, or given.
        function searchAsked() {
            var field = findIn(loader.item, 'chatSearchField')
            return loader.item.searchWanted || field.focus ? 'asked' : 'not asked'
        }
        function stackLog() {
            var log = pageStack.log
            pageStack.log = ''
            return log
        }
        function deleteChat(chatId) {
            findIn(loader.item, 'chats').delete_chat(chatId)
            return 'ok'
        }
        // The page arriving on screen, as a stack puts it there.
        function arrive() {
            loader.item.status = PageStatus.Inactive
            loader.item.status = PageStatus.Active
            return '' + loader.item.arrivingChatId
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_quick_action_lands_on_the_chat_list_and_does_what_it_says() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-quick-land-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. Two profiles, so a chat can be in the
    // other one.
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    macro_rules! act {
        ($kind:expr, $account:expr, $chat:expr) => {
            call!("act", QString::from($kind), $account, $chat)
        };
    }

    let list = common::page_url("ChatListPage.qml");
    let other = list.clone();

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(list.clone()),
                QString::from(r#"{"accountId": 1}"#)
            )
        );
        record!("registered", call!("registered"));
        record!("on-top", call!("onTop", QString::from("true")));

        // The search: emptied, and the keyboard asked for.
        record!("typed", call!("type", QString::from("half a word")));
        record!("search", act!("search", 0, 0));
        record!(
            "search-text",
            call!(
                "get",
                QString::from("chatSearchField"),
                QString::from("text")
            )
        );
        record!("search-asked", call!("searchAsked"));
        record!("search-raised", call!("raised"));
        record!("search-stack", call!("stackLog"));

        // The QR code, on the side the action names.
        record!("qr", act!("qr", 0, 0));
        record!("qr-stack", call!("stackLog"));
        record!("scan", act!("scan", 0, 0));
        record!("scan-stack", call!("stackLog"));

        // Over another page, that page goes first.
        record!("covered", call!("onTop", QString::from("false")));
        record!("qr-over", act!("qr", 0, 0));
        record!("qr-over-stack", call!("stackLog"));
        record!("uncovered", call!("onTop", QString::from("true")));

        // A chat of this profile, looked up and then opened.
        record!("chat", act!("chat", 1, 2));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("chat-stack", call!("stackLog"));
        record!("delete", call!("deleteChat", 1));
    });

    // A chat deleted since: said, and not opened.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("gone", act!("chat", 1, 1));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("gone-stack", call!("stackLog"));
        record!(
            "gone-banner",
            call!("get", QString::from("errorBanner"), QString::from("text"))
        );
        // A chat of the other profile.
        record!("elsewhere", act!("chat", 2, 2));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("elsewhere-stack", call!("stackLog"));
        // The list that replaced the stack, arriving with the chat to open.
        record!(
            "arrived-load",
            call!(
                "load",
                QString::from(other.clone()),
                QString::from(
                    r#"{"accountId": 2, "arrivingChatId": 2, "arrivingChatName": "chat 2"}"#
                )
            )
        );
        record!("arrived-on-top", call!("onTop", QString::from("true")));
        record!("arrived", call!("arrive"));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("arrived-stack", call!("stackLog"));
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

    assert_eq!(value("load"), "ok", "the chat list did not load. {context}");
    assert_eq!(
        value("registered"),
        "registered",
        "the chat list does not tell the window it is where quick actions \
         land. {context}"
    );
    assert_eq!(
        value("search-text"),
        "",
        "the search does not start empty. {context}"
    );
    assert_eq!(
        value("search-asked"),
        "asked",
        "the search field is not given the keyboard. {context}"
    );
    assert_eq!(
        value("search-raised"),
        "1",
        "the window is not brought up for a quick action. {context}"
    );
    assert_eq!(
        value("search-stack"),
        "",
        "the search moved the stack, with the list already on top. {context}"
    );
    assert_eq!(
        value("qr-stack"),
        r#"push:QrPage.qml:{"accountId":1,"mode":0}|"#,
        "the QR code action does not open this profile's code. {context}"
    );
    assert_eq!(
        value("scan-stack"),
        r#"push:QrPage.qml:{"accountId":1,"mode":1}|"#,
        "the scan action does not open the scanner. {context}"
    );
    assert_eq!(
        value("qr-over-stack"),
        r#"pop|push:QrPage.qml:{"accountId":1,"mode":0}|"#,
        "what was open over the list is not cleared first. {context}"
    );
    let chat = value("chat-stack");
    assert!(
        chat.starts_with("push:ConversationPage.qml:")
            && chat.contains(r#""accountId":1"#)
            && chat.contains(r#""chatId":2"#)
            && chat.contains(r#""chatName":"chat 2""#),
        "the chat is not opened by name in this profile: {chat}. {context}"
    );
    assert_eq!(
        value("gone-stack"),
        "",
        "a deleted chat was opened. {context}"
    );
    assert_eq!(
        value("gone-banner"),
        "That chat no longer exists.",
        "a deleted chat is not said to be gone. {context}"
    );
    assert_eq!(
        value("elsewhere-stack"),
        r#"replaceAbove:ChatListPage.qml:{"accountId":2,"arrivingChatId":2,"arrivingChatName":"chat 2"}|"#,
        "another profile's chat does not bring that profile's list. {context}"
    );
    assert_eq!(
        value("arrived"),
        "0",
        "the arriving chat is still waiting once the list is on screen. \
         {context}"
    );
    let arrived = value("arrived-stack");
    assert!(
        arrived.starts_with("push:ConversationPage.qml:")
            && arrived.contains(r#""accountId":2"#)
            && arrived.contains(r#""chatId":2"#)
            && arrived.contains(r#""chatName":"chat 2""#),
        "the other profile's list does not open the chat it came for: \
         {arrived}. {context}"
    );
}
