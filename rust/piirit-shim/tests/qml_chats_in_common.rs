//! Contact page's "Chats in common": asked of core by contact, never as the whole list.
//! Tap opens the chat via the list page; the own chat goes back.

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
            property var currentPage: null
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                log += 'push:' + name(page) + ':' + properties.chatId + '|'
                if (name(page) === 'CommonChatsPage.qml') {
                    allLoader.setSource(page, properties)
                    currentPage = allLoader.item
                }
                return null
            }
            function navigateBack() { log += 'back|' }
            function previousPage(page) { return page === loader.item ? 'conversation' : null }
            function pop(page) { log += 'pop:' + page + '|' }
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

        ChatInfo { id: group; account_id: 1; chat_id: 2 }

        Loader { id: loader }
        Loader { id: allLoader }
        function load(chatId) {
            loader.setSource('', {})
            loader.setSource('__PAGE__', { accountId: 1, chatId: chatId, chatName: 'from the list',
                                           status: PageStatus.Inactive })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function show() {
            loader.item.status = PageStatus.Activating
            pageStack.currentPage = loader.item
            return 'ok'
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
        function rows() {
            var header = findIn(loader.item, 'commonChatsHeader')
            var out = []
            var kids = header ? header.parent.children : []
            for (var i = 0; i < kids.length; i++) {
                var name = kids[i].objectName
                if (name.indexOf('commonChatRow') === 0 && kids[i].visible) {
                    out.push(name.substring('commonChatRow'.length))
                }
            }
            return out.join(',')
        }
        function allText(node) {
            if (!node) { return '' }
            var out = node.text !== undefined ? node.text + '|' : ''
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) { out += allText(kids[i]) }
            if (node.contentItem && node.contentItem !== node) { out += allText(node.contentItem) }
            return out
        }
        function rowText(name) { return allText(findIn(loader.item, name)) }
        function allRows() {
            var view = findIn(allLoader.item, 'commonChatsList')
            if (!view) { return 'missing:commonChatsList' }
            view.forceLayout()
            var out = []
            var ids = [1, 2, 3, 201, 202, 203]
            for (var i = 0; i < ids.length; i++) {
                if (findIn(view, 'allCommonChatRow' + ids[i])) { out.push(ids[i]) }
            }
            return out.join(',')
        }
        function tapAll(name) {
            var item = findIn(allLoader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }

        property var steps: []
        function record(label, value) { steps.push([label, '' + value]) }
        function stepsJson() { return JSON.stringify(steps) }
        property bool finished: false
        function isFinished() { return finished ? 'true' : 'false' }

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
                var chats = findIn(loader.item, 'commonChats')
                record('unshown', chats.account_id + ':' + chats.contact_id)
                record('show', show())
                step = 2
            } else if (step === 2 && count() === 6) {
                record('header', get('commonChatsHeader', 'visible'))
                record('header-text', get('commonChatsHeader', 'text'))
                record('rows', rows())
                record('all-shown', get('commonChatsAll', 'visible'))
                record('row-text', rowText('commonChatRow2'))
                record('row-name', get('commonChatRow2', 'height') > 0 ? 'sized' : 'flat')
                record('tap-other', tap('commonChatRow2'))
                record('list-log', listPage.log)
                record('tap-own', tap('commonChatRow1'))
                record('stack-own', pageStack.log)
                pageStack.log = ''
                record('tap-all', tap('commonChatsAll'))
                record('stack-all', pageStack.log)
                record('all-rows', allRows())
                listPage.log = ''
                record('all-tap-other', tapAll('allCommonChatRow3'))
                record('all-list-log', listPage.log)
                pageStack.log = ''
                record('all-tap-own', tapAll('allCommonChatRow1'))
                record('all-stack-own', pageStack.log)
                pageStack.currentPage = loader.item
                pageStack.log = ''
                listPage.accountId = 2
                record('tap-other-profile', tap('commonChatRow201'))
                record('stack-other-profile', pageStack.log)
                pageStack.log = ''
                appWindow.chatList = null
                record('tap-no-list', tap('commonChatRow201'))
                record('stack-no-list', pageStack.log)
                record('list-log-after', listPage.log)
                group.remove_member(10)
                step = 3
            } else if (step === 3 && count() === 5) {
                record('rows-after-leave', rows())
                record('all-after-leave', get('commonChatsAll', 'visible'))
                record('header-after-leave', get('commonChatsHeader', 'text'))
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

const PATIENCE_SECONDS: u32 = 30;

#[test]
#[allow(clippy::too_many_lines)]
fn a_contacts_page_lists_the_chats_shared_with_them() {
    let temp = std::env::temp_dir().join(format!("piirit-chats-in-common-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before the server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_SELF_TALK", "7");
        std::env::set_var("PIIRIT_FAKE_MANY_CHATS", "3");
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
    assert_eq!(
        (
            value("header"),
            value("header-text"),
            value("rows"),
            value("all-shown")
        ),
        (
            "true".to_string(),
            "Chats in common (6)".to_string(),
            "1,2,201,202,203".to_string(),
            "true".to_string()
        ),
        "the contact page does not list the first five chats shared with \
         the contact under a heading counting all, with Show all below. \
         {context}"
    );
    assert_eq!(
        (value("unshown"), value("show")),
        ("0:0".to_string(), "ok".to_string()),
        "the chats in common were loaded before the page was shown; it sits \
         beside every one-to-one chat, mostly never opened. {context}"
    );
    assert!(
        value("row-text").contains("chat 2") && !value("row-text").contains("last in"),
        "a common chat's row does not show just its name. {context}"
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
        (value("tap-all"), value("stack-all"), value("all-rows")),
        (
            "ok".to_string(),
            "push:CommonChatsPage.qml:undefined|".to_string(),
            "1,2,3,201,202,203".to_string()
        ),
        "Show all does not open a page listing every chat in common. \
         {context}"
    );
    assert_eq!(
        (
            value("all-tap-other"),
            value("all-list-log"),
            value("all-tap-own"),
            value("all-stack-own")
        ),
        (
            "ok".to_string(),
            "above:3:chat 3|".to_string(),
            "ok".to_string(),
            "pop:conversation|".to_string()
        ),
        "a chat tapped on the Show all page does not open as on the contact \
         page, or the contact's own chat is not gone back to. {context}"
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
            "push:ConversationPage.qml:201|".to_string(),
            "ok".to_string(),
            "push:ConversationPage.qml:201|".to_string(),
            "above:3:chat 3|".to_string()
        ),
        "with no chat list of this profile to open through, a common chat \
         did not open over the page, or went to the wrong list. {context}"
    );
    assert_eq!(
        (
            value("rows-after-leave"),
            value("all-after-leave"),
            value("header-after-leave")
        ),
        (
            "1,201,202,203,3".to_string(),
            "false".to_string(),
            "Chats in common (5)".to_string()
        ),
        "the group the contact left stays among the chats in common, or \
         Show all stays with five left. {context}"
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

/// Whole seconds: vendored `single_shot` drops the fraction.
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
