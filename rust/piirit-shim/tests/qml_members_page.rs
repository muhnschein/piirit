//! Members page: every member, search by name (never address), pulley's invite opens group's QR
//! code, tap opens a member's page where a chat with them exists, else their menu; Send message
//! makes the chat and opens it; Remove waits out platform's countdown, then asks core. Search
//! cleared once page gone (model shared with group page).

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

/// Silica `pageStack`, recorded: which page a tap opened, on which chat.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:ContactPage.qml(chat=1)|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap) -> QVariant),
}

impl PageStackProbe {
    fn push(&mut self, page: QString, properties: QVariantMap) -> QVariant {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page);
        let chat =
            i32::from_qvariant(properties.value(QString::from("chatId"), QVariant::default()))
                .unwrap_or_default();
        // QR page only: says whether group or channel.
        let kind =
            bool::from_qvariant(properties.value(QString::from("isChannel"), QVariant::default()))
                .map_or(String::new(), |channel| format!(",channel={channel}"));
        let current = self.log.to_string();
        self.log = format!("{current}push:{name}(chat={chat}{kind})|").into();
        self.log_changed();
        QVariant::default()
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        /// Group's model, as the group page holds it.
        ChatInfo { id: chat; objectName: 'chat'; account_id: 1; chat_id: 2 }
        Connections {
            target: core
            onCore_event: chat.handle_event(context_id, kind, payload_json)
        }
        Loader { id: loader; width: 540; height: 960 }
        function load(url) {
            chat.reload()
            loader.setSource(url, { accountId: 1, chat: chat, pendingDelay: 100 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function unload() { loader.setSource('', {}); return 'ok' }
        function filter() { return chat.member_filter }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                var inside = findIn(node.contentItem, name)
                if (inside) { return inside }
            }
            if (node.headerItem) {
                var inHeader = findIn(node.headerItem, name)
                if (inHeader) { return inHeader }
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
        // A menu item of one particular row: every row has one.
        function clickIn(rowName, itemName) {
            var row = findIn(loader.item, rowName)
            if (!row) { return 'missing:' + rowName }
            var item = findIn(row.menu, itemName)
            if (!item) { return 'missing:' + itemName }
            item.clicked()
            return 'ok'
        }
        // Countdown over one row: the page raises it there.
        function countdownOn(rowName) {
            var row = findIn(loader.item, rowName)
            if (!row) { return 'missing:' + rowName }
            return row.remorse ? '' + row.remorse.active : 'none'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_members_page_finds_opens_and_removes_members() {
    let temp = std::env::temp_dir().join(format!("piirit-members-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Self, Ada (chat 1 with her), member0 and member1 (no chat).
        std::env::set_var("PIIRIT_FAKE_CROWD", "2");
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
            let result = (*engine_ptr).invoke_method($name.into(), &[$(QVariant::from($arg)),*]);
            QString::from_qvariant(result).map(|value| value.to_string()).unwrap_or_default()
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

    let core_ptr = std::ptr::addr_of!(core_box);
    let stack_ptr = std::ptr::addr_of!(stack_box);
    let journal_path = journal.clone();
    let mut plan = common::Plan::default();
    // SAFETY for every block: fire only inside `exec()`; `engine`, `core_box`, `stack_box`,
    // `steps` outlive it.
    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    plan.after(0, move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::page_url("MembersPage.qml")))
        );
    });
    plan.until(1, 20, move |last| unsafe {
        if get!("memberList", "count") != "4" && !last {
            return false;
        }
        record!("title", get!("membersHeader", "title"));
        record!("count", get!("memberList", "count"));
        record!("add-item", get!("addMembersItem", "visible"));
        record!("invite-item", get!("inviteItem", "visible"));
        record!("menu", get!("membersMenu", "visible"));
        record!("self-menu", get!("memberRow1", "menu"));
        // Name, any case.
        record!(
            "search-name",
            call!(
                "setText",
                QString::from("searchField"),
                QString::from("ADA")
            )
        );
        record!("found-by-name", get!("memberList", "count"));
        record!("found-row", get!("memberRow10", "objectName"));
        record!("nobody-yet", get!("membersPlaceholder", "enabled"));
        // Addresses never searched: nobody, and page says so.
        record!(
            "search-address",
            call!(
                "setText",
                QString::from("searchField"),
                QString::from("example.org")
            )
        );
        record!("found-by-address", get!("memberList", "count"));
        record!("nobody", get!("membersPlaceholder", "enabled"));
        record!("nobody-text", get!("membersPlaceholder", "text"));
        record!(
            "search-clear",
            call!("setText", QString::from("searchField"), QString::from(""))
        );
        record!("all-again", get!("memberList", "count"));
        record!("add", call!("click", QString::from("addMembersItem")));
        record!("invite", call!("click", QString::from("inviteItem")));
        // Ada: chat with her exists, her page.
        record!("tap-ada", call!("click", QString::from("memberRow10")));
        true
    });
    plan.until(1, 20, move |last| unsafe {
        let log = (*stack_ptr).pinned().borrow().log.to_string();
        if !log.contains("ContactPage") && !last {
            return false;
        }
        // member0: none, menu instead.
        record!(
            "tap-stranger",
            call!("click", QString::from("memberRow1000"))
        );
        true
    });
    plan.until(1, 20, move |last| unsafe {
        let open = get!("memberRow1000", "menuOpen") == "true";
        if !open && !last {
            return false;
        }
        record!("stranger-menu", get!("memberRow1000", "menuOpen"));
        record!("ada-menu", get!("memberRow10", "menuOpen"));
        record!(
            "send-message",
            call!(
                "clickIn",
                QString::from("memberRow1000"),
                QString::from("sendMessage")
            )
        );
        record!(
            "remove",
            call!(
                "clickIn",
                QString::from("memberRow1001"),
                QString::from("removeItem")
            )
        );
        record!(
            "countdown",
            call!("countdownOn", QString::from("memberRow1001"))
        );
        record!("count-while-waiting", get!("memberList", "count"));
        true
    });
    plan.until(1, 20, move |last| unsafe {
        let removed = common::calls(&journal_path)
            .iter()
            .any(|(name, _)| name == "remove_contact_from_chat");
        let gone = get!("memberList", "count") == "3";
        let opened = (*stack_ptr)
            .pinned()
            .borrow()
            .log
            .to_string()
            .contains("ConversationPage");
        let done = removed && gone && opened;
        if !done && !last {
            return false;
        }
        record!("count-after", get!("memberList", "count"));
        // Search left behind: shared model must not stay filtered.
        call!(
            "setText",
            QString::from("searchField"),
            QString::from("ada")
        );
        record!("filter-before", call!("filter"));
        record!("unload", call!("unload"));
        record!("filter-after", call!("filter"));
        (*engine_ptr).quit();
        true
    });
    plan.run();
    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_members(&steps, &navigation, &common::calls(&journal));
    let _ = std::fs::remove_dir_all(&temp);
}

#[allow(clippy::too_many_lines)]
fn assert_members(steps: &[(&str, String)], navigation: &str, calls: &[(String, Value)]) {
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
        "search-name",
        "search-address",
        "search-clear",
        "add",
        "invite",
        "tap-ada",
        "tap-stranger",
        "send-message",
        "remove",
        "unload",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    for (label, expected, complaint) in [
        (
            "title",
            "4 member(s)",
            "the heading does not count every member",
        ),
        ("count", "4", "the page does not list every member"),
        (
            "add-item",
            "true",
            "the pulley does not offer to add members",
        ),
        ("invite-item", "true", "the pulley does not offer an invite"),
        ("menu", "true", "the page has no pulley"),
        ("self-menu", "null", "the reader's own row has a menu"),
        (
            "found-by-name",
            "1",
            "search by name in another case found the wrong rows",
        ),
        (
            "found-row",
            "memberRow10",
            "search by name did not find Ada",
        ),
        (
            "nobody-yet",
            "false",
            "the page says nobody matches while someone does",
        ),
        ("found-by-address", "0", "search looks in addresses"),
        ("nobody", "true", "the page does not say nobody matches"),
        (
            "nobody-text",
            "Nobody matches",
            "the page says the wrong thing when nobody matches",
        ),
        (
            "all-again",
            "4",
            "clearing search does not bring every member back",
        ),
        (
            "stranger-menu",
            "true",
            "a tap on a member with no chat did not open their menu",
        ),
        (
            "ada-menu",
            "false",
            "a tap on a member with a chat opened their menu",
        ),
        (
            "countdown",
            "true",
            "Remove did not put the platform's countdown over the row",
        ),
        (
            "count-while-waiting",
            "4",
            "the member went before the countdown ended",
        ),
        ("count-after", "3", "the removed member is still listed"),
        ("filter-before", "ada", "the search did not reach the model"),
        (
            "filter-after",
            "",
            "the group page's model stays filtered once the page is gone",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    // Add members, group's own invite; Ada's page on her chat; member0's new chat opened, not
    // their page.
    assert_eq!(
        navigation,
        "push:AddMembersPage.qml(chat=0)|push:QrPage.qml(chat=2,channel=false)|push:ContactPage.qml(chat=1)|\
         push:ConversationPage.qml(chat=501)|",
        "a tap did not open the right page. {context}"
    );
    let asked = |method: &str| -> Vec<Value> {
        calls
            .iter()
            .filter(|(name, _)| name == method)
            .map(|(_, params)| params.clone())
            .collect()
    };
    assert!(
        !asked("create_chat_by_contact_id").contains(&serde_json::json!([1, 10]))
            && asked("create_chat_by_contact_id").contains(&serde_json::json!([1, 1000])),
        "a tap made a chat, or Send message made none. {context}"
    );
    assert_eq!(
        asked("remove_contact_from_chat"),
        vec![serde_json::json!([1, 2, 1001])],
        "core was not asked to remove exactly the member asked for. {context}"
    );
}
