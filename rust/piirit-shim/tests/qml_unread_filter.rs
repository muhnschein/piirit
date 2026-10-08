//! The unread filter's switch, beside the chat list's search field.
//!
//! It is there on the ordinary list and not on the archived one, which is
//! a mode of its own. Turning it on says so under the title, and a list
//! with nothing unread says why it is empty and how to get back. While a
//! search is showing it does nothing, since a search covers every chat.

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
fn the_filter_switch_sits_by_the_search_and_says_when_it_is_on() {
    let temp = std::env::temp_dir().join(format!("piirit-unread-switch-{}", std::process::id()));
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "ordinary",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                false
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("switch", get!("unreadFilterButton", "visible"));
        record!("off-checked", get!("unreadFilterButton", "checked"));
        record!("off-description", get!("chatListHeader", "description"));
        record!("count-before", get!("chats", "count"));
        record!(
            "clicked",
            call!("click", QString::from("unreadFilterButton"))
        );
    });

    // The fake's chats have nothing unread, so the filtered list is empty.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("on-checked", get!("unreadFilterButton", "checked"));
        record!("on-description", get!("chatListHeader", "description"));
        record!("on-count", get!("chats", "count"));
        record!("laid-out", call!("layout", QString::from("chatList")));
        record!("placeholder", get!("chatListPlaceholder", "enabled"));
        record!("placeholder-text", get!("chatListPlaceholder", "text"));
        record!("placeholder-hint", get!("chatListPlaceholder", "hintText"));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("a")
            )
        );
        record!("while-searching", get!("unreadFilterButton", "visible"));
        record!(
            "cleared",
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("")
            )
        );
        record!("after-search", get!("unreadFilterButton", "visible"));
        record!(
            "clicked-off",
            call!("click", QString::from("unreadFilterButton"))
        );
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("count-after", get!("chats", "count"));
        record!(
            "archived",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                true
            )
        );
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("archived-switch", get!("unreadFilterButton", "visible"));
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
        value("ordinary"),
        "ok",
        "the chat list did not load. {context}"
    );
    assert_eq!(
        value("switch"),
        "true",
        "the ordinary list has no unread filter switch. {context}"
    );
    assert_eq!(
        value("off-checked"),
        "false",
        "the filter starts on. {context}"
    );
    assert_eq!(
        value("off-description"),
        "",
        "the header has a line under the title before the filter is on. \
         {context}"
    );
    assert_eq!(
        value("count-before"),
        "2",
        "the fake's chats never loaded. {context}"
    );
    assert_eq!(
        value("on-checked"),
        "true",
        "the switch did not turn on. {context}"
    );
    assert_eq!(
        value("on-description"),
        "",
        "the header grew a line when the filter went on, pushing the \
         search row down under the finger that tapped it. {context}"
    );
    assert_eq!(
        value("on-count"),
        "0",
        "the filter let through chats with nothing unread. {context}"
    );
    assert_eq!(
        value("placeholder"),
        "true",
        "an empty filtered list shows nothing at all. {context}"
    );
    assert_eq!(
        value("placeholder-text"),
        "No unread chats",
        "an empty filtered list claims there are no chats. {context}"
    );
    assert_eq!(
        value("placeholder-hint"),
        "Tap Unread to show all chats",
        "an empty filtered list does not say how to get the chats back. \
         {context}"
    );
    assert_eq!(
        value("while-searching"),
        "false",
        "the switch still shows while a search, which ignores it, has \
         the row. {context}"
    );
    assert_eq!(
        value("after-search"),
        "true",
        "the switch stayed hidden after the search was cleared. {context}"
    );
    assert_eq!(
        value("count-after"),
        "2",
        "turning the filter off did not bring the chats back. {context}"
    );
    assert_eq!(
        value("archived"),
        "ok",
        "the archived list did not load. {context}"
    );
    assert_eq!(
        value("archived-switch"),
        "false",
        "the archived list offers an unread filter, though it is a mode of \
         its own. {context}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
