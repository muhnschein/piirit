//! Quick action from cover lands: chat list comes front, doing what action asks.
//!
//! Search empties and asks for keyboard. QR opens on named side. Chat is looked up first:
//! deleted since, it is "gone", not opened empty. Chat in another profile replaces the
//! whole stack with that profile's list, then opens from it.

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

/// Probe, components dir filled in; see `qml_chat_list.rs`.
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
            // Page replaced above, by `aboveWhom`.
            property var target: null
            function replaceAbove(above, page, properties) {
                target = above
                log += 'replaceAbove:' + name(page) + ':' + JSON.stringify(properties) + '|'
            }
            function pop() { log += 'pop|' }
        }
        property QtObject appWindow: QtObject {
            property int accountId: 0
            property Item chatList: null
            property int raised: 0
            // A page up that must not be jumped away from.
            property bool quickActionsPaused: false
            function activate() { raised += 1 }
        }

        Loader { id: loader }
        // A list attached beside a finished backup, as BackupPage puts it.
        Loader { id: attachedLoader }
        function attach(properties) {
            attachedLoader.setSource(Qt.resolvedUrl(loader.source), JSON.parse(properties))
            return attachedLoader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function detach() { attachedLoader.setSource('', {}); return 'ok' }
        function paused(on) { appWindow.quickActionsPaused = on === 'true'; return 'ok' }
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
        // Contact page's chat in common, opened through list.
        function openAbove(chatId, chatName) {
            loader.item.openChatAbove(chatId, chatName)
            return 'ok'
        }
        function aboveWhom() {
            return pageStack.target === loader.item ? 'list' : '' + pageStack.target
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

    // SAFETY: single-threaded; set before Qt and server start. Two profiles: chat can be in other.
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

    // SAFETY: callbacks run only inside `exec()` on this thread.
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

        // Search: emptied, keyboard asked for.
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

        // QR on the side the action names.
        record!("qr", act!("qr", 0, 0));
        record!("qr-stack", call!("stackLog"));
        record!("scan", act!("scan", 0, 0));
        record!("scan-stack", call!("stackLog"));

        // Profiles, as pull-down opens them.
        record!("profiles", act!("profiles", 0, 0));
        record!("profiles-stack", call!("stackLog"));

        // Over another page: that page goes first.
        record!("covered", call!("onTop", QString::from("false")));
        record!("qr-over", act!("qr", 0, 0));
        record!("qr-over-stack", call!("stackLog"));
        record!("uncovered", call!("onTop", QString::from("true")));

        // Chat of this profile: looked up, then opened.
        record!("chat", act!("chat", 1, 2));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("chat-stack", call!("stackLog"));
        record!("delete", call!("deleteChat", 1));
    });

    // Chat deleted since: said, not opened.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("gone", act!("chat", 1, 1));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("gone-stack", call!("stackLog"));
        record!(
            "gone-banner",
            call!("get", QString::from("errorBanner"), QString::from("text"))
        );
        // Chat of the other profile.
        record!("elsewhere", act!("chat", 2, 2));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("elsewhere-stack", call!("stackLog"));
        // List that replaced stack, arriving with chat to open.
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

        // Page up mid-work (backup being written) under a tap: app comes forward; nothing popped.
        call!("onTop", QString::from("false"));
        call!("paused", QString::from("true"));
        record!("paused-raised-before", call!("raised"));
        record!("paused", act!("qr", 0, 0));
        record!("paused-raised", call!("raised"));
        record!("paused-stack", call!("stackLog"));
        call!("paused", QString::from("false"));
        call!("onTop", QString::from("true"));

        // List attached beside finished backup: swipe back takes it; one under it stays.
        record!(
            "attach",
            call!(
                "attach",
                QString::from(r#"{"accountId": 2, "attached": true}"#)
            )
        );
        record!("attached-registered", call!("registered"));
        call!("detach");
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("detached-registered", call!("registered"));

        // Chat in common, from contact page over a chat: one replace above list, which now
        // counts it as open.
        call!("stackLog");
        record!("above", call!("openAbove", 5, QString::from("chat 5")));
        record!("above-stack", call!("stackLog"));
        record!("above-whom", call!("aboveWhom"));
        record!(
            "above-viewing",
            call!(
                "get",
                QString::from("notifier"),
                QString::from("viewingChatId")
            )
        );
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
        value("paused-stack"),
        "",
        "a tap popped a page that was in the middle of something. {context}"
    );
    let raised_before: u32 = value("paused-raised-before").parse().unwrap_or_default();
    assert_eq!(
        value("paused-raised"),
        (raised_before + 1).to_string(),
        "the app did not come forward onto the page it kept. {context}"
    );
    assert_eq!(value("attach"), "ok", "{context}");
    assert_eq!(
        (
            value("attached-registered").as_str(),
            value("detached-registered").as_str()
        ),
        ("registered", "registered"),
        "a chat list attached beside a finished backup took the window's \
         quick actions, and left it with none once it went. {context}"
    );
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
        value("profiles-stack"),
        r#"push:ProfilesPage.qml:{"currentAccountId":1}|"#,
        "the profiles action does not open the profiles page. {context}"
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
    assert_eq!(
        (
            value("above"),
            value("above-stack"),
            value("above-whom"),
            value("above-viewing")
        ),
        (
            "ok".to_string(),
            r#"replaceAbove:ConversationPage.qml:{"accountId":2,"chatId":5,"chatName":"chat 5"}|"#
                .to_string(),
            "list".to_string(),
            "5".to_string()
        ),
        "a chat in common does not open in place of the pages over the list, \
         or the list does not count it as the open chat. {context}"
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
