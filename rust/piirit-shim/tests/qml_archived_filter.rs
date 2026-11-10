//! Chat list's Archived pill: archived chats in place of ordinary, same page. Pulley, title stay;
//! pulley has no archive item. Kind pills follow shown side. Search under it searches everything.
//! Empty archive says so, and how back.
//!
//! Unarchive in row menu not checked: `ContextMenu` is a `Component`, never built headless. Row's
//! `is_archived`, which it reads, is.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url, accountId, archived) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId, archived: archived })
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
            // A row's menu is not among its children.
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
        // A view with no window never lays out on its own, so no
        // delegate -- and no section header -- is ever built. Forcing it
        // is what makes what the list *shows* observable here at all.
        function layout(name) {
            var view = findIn(loader.item, name)
            if (!view) { return 'missing:' + name }
            if (view.forceLayout) { view.forceLayout() }
            return 'ok'
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function hide(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.hideClicked()
            return 'ok'
        }
        // Every text, hint placeholder shows from here on, '|'-joined: stub has no
        // frames, so a flash shows only as a text change.
        property var shown: []
        function watch(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.textChanged.connect(function () { shown.push(item.text) })
            item.hintTextChanged.connect(function () { shown.push(item.hintText) })
            return 'ok'
        }
        function seen() { return shown.join('|') }
        function deleteChat(chatId) {
            var chats = findIn(loader.item, 'chats')
            if (!chats) { return 'missing:chats' }
            chats.delete_chat(chatId)
            return 'ok'
        }
        // Rows as `<chat><a|o>`: archived or ordinary, as row menu reads it.
        Repeater {
            id: peek
            model: null
            Item { property string mark: model.chat_id + (model.is_archived ? 'a' : 'o') }
        }
        function sides() {
            var chats = findIn(loader.item, 'chats')
            if (!chats) { return 'missing:chats' }
            peek.model = chats.rows
            var marks = []
            for (var i = 0; i < peek.count; i++) { marks.push(peek.itemAt(i).mark) }
            peek.model = null
            return marks.join(',')
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_archived_pill_swaps_the_list_in_place() {
    let temp = std::env::temp_dir().join(format!("piirit-archived-pill-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Chat 2 a joined channel, chat 1 a 1:1.
        std::env::set_var("PIIRIT_FAKE_CHANNELS", "2");
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
    macro_rules! click {
        ($name:expr) => {
            call!("click", QString::from($name))
        };
    }
    // What page offers around list: archived lit / kind pills / pulley's New / title / sides.
    macro_rules! chrome {
        () => {
            format!(
                "{}/{}/{}/{}/{}",
                get!("archivedFilterButton", "checked"),
                get!("chatsFilterButton", "visible"),
                get!("newMenuItem", "visible"),
                get!("chatListHeader", "title"),
                call!("sides")
            )
        };
    }
    // Placeholder's words, once list is empty.
    macro_rules! empty {
        () => {
            format!(
                "{} | {}",
                get!("chatListPlaceholder", "text"),
                get!("chatListPlaceholder", "hintText")
            )
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    // Next step once `chats.count` is `$want` (last try: as it is).
    macro_rules! at_count {
        ($want:expr, $step:expr) => {
            move |last| unsafe {
                if get!("chats", "count") != $want && !last {
                    return false;
                }
                $step;
                true
            }
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "loaded",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                false
            )
        );
    });

    plan.until(
        1,
        30,
        at_count!("2", {
            record!("ordinary", chrome!());
            record!("pulley-item", get!("archivedMenuItem", "visible"));
            record!("pill", get!("archivedFilterButton", "text"));
            click!("archivedFilterButton");
        }),
    );
    plan.until(
        1,
        30,
        at_count!("1", {
            record!("archived", chrome!());
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("chat")
            );
            record!(
                "search",
                format!(
                    "{}/{}",
                    get!("searchResults", "visible"),
                    get!("chatList", "visible")
                )
            );
            call!("hide", QString::from("chatSearchField"));
            call!("deleteChat", 3u32);
        }),
    );
    plan.until(
        1,
        30,
        at_count!("0", {
            record!("empty", empty!());
            click!("archivedFilterButton");
        }),
    );
    plan.until(
        1,
        30,
        at_count!("2", {
            record!("back", chrome!());
            (*engine_ptr).quit();
        }),
    );

    plan.run();
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
        value("loaded"),
        "ok",
        "the chat list did not load. {context}"
    );
    assert_eq!(
        value("ordinary"),
        "false/true/true/Chats/1o,2o",
        "the list did not start on ordinary chats with the Archived pill unlit. {context}"
    );
    assert_eq!(
        value("pulley-item"),
        "missing:archivedMenuItem",
        "the pull-down menu still has an archived chats item. {context}"
    );
    assert_eq!(
        value("pill"),
        "Archived",
        "the Archived pill is missing or worded otherwise. {context}"
    );
    assert_eq!(
        value("archived"),
        "true/false/true/Chats/3a",
        "tapping Archived did not show the archived chat alone, marked archived, with the \
         pill lit, the pull-down menu and title kept, and no kind pills over a list \
         without channels. {context}"
    );
    assert_eq!(
        value("search"),
        "true/false",
        "searching under Archived did not show the search results. {context}"
    );
    assert_eq!(
        value("empty"),
        "No archived chats | Tap Archived to show all chats",
        "an empty archive does not say so, or not how to get the chats back. {context}"
    );
    assert_eq!(
        value("back"),
        "false/true/true/Chats/1o,2o",
        "tapping a lit Archived did not bring the ordinary chats back. {context}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
