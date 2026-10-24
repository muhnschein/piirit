//! Contact page's "Chats in common": every chat core lists with contact, as Delta Chat apps.
//!
//! Asked of core by contact (filtering held rows would miss unloaded and archived chats), never
//! as whole list: contact unknown till chat loads, and contact 0 means every chat. Not before page
//! first shown. Tap opens chat
//! in contact chat's place via list page (it tracks open chat); own chat: back to it. Member
//! leaves group: row goes. Saved messages: no section, nothing asked.

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
use serde_json::Value;

mod common;

/// Probe: page under test, page stack and window as far as contact page uses them; each step
/// waits on what it needs, checked every 100 ms. `__PAGE__` filled in.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        id: probe

        property QtObject pageStack: QtObject {
            property string log: ''
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                log += 'push:' + name(page) + ':' + properties.chatId + '|'
                return null
            }
            function navigateBack() { log += 'back|' }
        }
        property QtObject listPage: QtObject {
            property int accountId: 1
            property string log: ''
            function openChatAbove(chatId, chatName) {
                log += 'above:' + chatId + ':' + chatName + '|'
            }
        }
        property QtObject appWindow: QtObject {
            property QtObject chatList: probe.listPage
        }

        // Ada leaves group 2, as from another device.
        ChatInfo { id: group; account_id: 1; chat_id: 2 }

        Loader { id: loader }
        // Attached beside chat: off screen until swiped to.
        function load(chatId) {
            loader.setSource('', {})
            loader.setSource('__PAGE__', { accountId: 1, chatId: chatId, chatName: 'from the list',
                                           status: PageStatus.Inactive })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function show() { loader.item.status = PageStatus.Activating; return 'ok' }
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function count() {
            var chats = findIn(loader.item, 'commonChats')
            return chats ? chats.count : -1
        }
        // Row names top to bottom, by chat id.
        function rows() {
            var out = []
            for (var id = 1; id <= 9; id++) {
                if (findIn(loader.item, 'commonChatRow' + id)) { out.push(id) }
            }
            return out.join(',')
        }

        property var steps: []
        function record(label, value) { steps.push([label, '' + value]) }
        function stepsJson() { return JSON.stringify(steps) }
        property bool finished: false
        function isFinished() { return finished ? 'true' : 'false' }

        // What each step waits for; steps run in order.
        property int step: 0
        Timer {
            interval: 100; running: !probe.finished; repeat: true
            onTriggered: probe.advance()
        }
        function advance() {
            if (step === 0 && core.status === 'ready') {
                record('load', load(1))
                step = 1
            } else if (step === 1 && get('chat', 'loaded') === 'true'
                       && loader.item.contactId === 10) {
                // Contact known, page not shown: list has no account, asks nothing.
                var chats = findIn(loader.item, 'commonChats')
                record('unshown', chats.account_id + ':' + chats.contact_id)
                record('show', show())
                step = 2
            } else if (step === 2 && count() === 3) {
                record('header', get('commonChatsHeader', 'visible'))
                record('header-text', get('commonChatsHeader', 'text'))
                record('rows', rows())
                record('row-name', get('commonChatRow2', 'height') > 0 ? 'sized' : 'flat')
                record('tap-other', tap('commonChatRow2'))
                record('list-log', listPage.log)
                record('tap-own', tap('commonChatRow1'))
                record('stack-own', pageStack.log)
                pageStack.log = ''
                // List of another profile (switched meanwhile), then none: pushed over.
                listPage.accountId = 2
                record('tap-other-profile', tap('commonChatRow3'))
                record('stack-other-profile', pageStack.log)
                pageStack.log = ''
                appWindow.chatList = null
                record('tap-no-list', tap('commonChatRow3'))
                record('stack-no-list', pageStack.log)
                record('list-log-after', listPage.log)
                group.remove_member(10)
                step = 3
            } else if (step === 3 && count() === 2) {
                record('rows-after-leave', rows())
                record('header-after-leave', get('commonChatsHeader', 'visible'))
                record('load-self', load(7))
                record('show-self', show())
                step = 4
            } else if (step === 4 && get('chat', 'loaded') === 'true'
                       && loader.item.contactId === 1) {
                record('self-count', count())
                record('self-header', get('commonChatsHeader', 'visible'))
                finished = true
            }
        }
    }
";

/// Ticks to wait for the steps; only a broken run spends them.
const PATIENCE_SECONDS: u32 = 30;

#[test]
#[allow(clippy::too_many_lines)]
fn a_contacts_page_lists_the_chats_shared_with_them() {
    let temp = std::env::temp_dir().join(format!("piirit-chats-in-common-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits. Chat 7: saved messages.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_SELF_TALK", "7");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(
        PROBE_QML.replace("__PAGE__", &common::page_url("ContactPage.qml")),
    ));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps_json = String::new();
    let steps_ptr: *mut String = std::ptr::addr_of_mut!(steps_json);
    wait_for_steps(engine_ptr, steps_ptr, PATIENCE_SECONDS);

    engine.exec();

    let steps: Vec<(String, String)> = serde_json::from_str(&steps_json).unwrap_or_default();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the contact page did not load. {context}"
    );
    // Ada: her chat, the group, her archived chat; core's order.
    assert_eq!(
        (value("header"), value("header-text"), value("rows")),
        (
            "true".to_string(),
            "Chats in common".to_string(),
            "1,2,3".to_string()
        ),
        "the contact page does not list the chats shared with the contact \
         under its heading. {context}"
    );
    assert_eq!(
        (value("unshown"), value("show")),
        ("0:0".to_string(), "ok".to_string()),
        "the chats in common were loaded before the page was shown; it sits \
         beside every one-to-one chat, mostly never opened. {context}"
    );
    assert_eq!(
        value("row-name"),
        "sized",
        "a common chat's row has no height. {context}"
    );
    assert_eq!(
        (value("tap-other"), value("list-log")),
        ("ok".to_string(), "above:2:chat 2|".to_string()),
        "tapping a common chat did not open it through the chat list, which \
         knows which chat is open. {context}"
    );
    assert_eq!(
        (value("tap-own"), value("stack-own")),
        ("ok".to_string(), "back|".to_string()),
        "tapping the chat the page belongs to did not go back to it. {context}"
    );
    assert_eq!(
        (
            value("tap-other-profile"),
            value("stack-other-profile"),
            value("tap-no-list"),
            value("stack-no-list"),
            value("list-log-after")
        ),
        (
            "ok".to_string(),
            "push:ConversationPage.qml:3|".to_string(),
            "ok".to_string(),
            "push:ConversationPage.qml:3|".to_string(),
            "above:2:chat 2|".to_string()
        ),
        "with no chat list of this profile to open through, a common chat \
         did not open over the page, or went to the wrong list. {context}"
    );
    assert_eq!(
        (value("rows-after-leave"), value("header-after-leave")),
        ("1,3".to_string(), "true".to_string()),
        "the group the contact left stays among the chats in common. \
         {context}"
    );
    assert_eq!(
        (value("self-count"), value("self-header")),
        ("0".to_string(), "false".to_string()),
        "saved messages shows chats in common: every chat is shared with \
         oneself. {context}"
    );

    let entries: Vec<Value> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "get_chatlist_entries")
        .map(|(_, params)| params)
        .collect();
    assert!(
        !entries.is_empty(),
        "the core was never asked for the chats in common"
    );
    assert!(
        entries
            .iter()
            .all(|params| params.pointer("/3").and_then(Value::as_u64) == Some(10)),
        "the contact page asked the core for chats not by Ada: a whole \
         list, or by oneself. calls: {entries:?}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

/// Each second: probe done, or patience spent? Then take steps and quit. Whole seconds: vendored
/// `single_shot` drops fraction.
fn wait_for_steps(engine: *mut QmlEngine, steps: *mut String, patience: u32) {
    single_shot(Duration::from_secs(1), move || unsafe {
        let finished = (*engine).invoke_method("isFinished".into(), &[]);
        let finished = QString::from_qvariant(finished)
            .map(|value| value.to_string())
            .unwrap_or_default();
        if finished == "true" || patience == 0 {
            *steps = QString::from_qvariant((*engine).invoke_method("stepsJson".into(), &[]))
                .map(|value| value.to_string())
                .unwrap_or_default();
            (*engine).quit();
        } else {
            wait_for_steps(engine, steps, patience - 1);
        }
    });
}
