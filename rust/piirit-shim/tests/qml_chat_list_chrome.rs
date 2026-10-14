//! Chat list offers things, hides what it can't offer. Pull-down: profiles, settings, archive, new
//! group, QR, new chat. Row menu: mark read or unread, never both. Hiding profiles with one profile
//! would remove the only route to making one; archive search field over empty archive is dead. It
//! comes back the moment something is typed, or clearing it would be impossible.

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
fn the_list_offers_profiles_and_hides_a_search_with_nothing_to_search() {
    let temp = std::env::temp_dir().join(format!("piirit-chrome-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // One profile, empty archive: state in which both were wrong on a device.
        std::env::set_var("PIIRIT_FAKE_NO_ARCHIVED", "1");
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
        record!("profiles", get!("profilesMenuItem", "visible"));
        record!("settings", get!("settingsMenuItem", "visible"));
        record!("new-group", get!("newGroupMenuItem", "visible"));
        record!("qr", get!("qrMenuItem", "visible"));
        record!("laid-out", call!("layout", QString::from("chatList")));
        // Seeded chats all read: first row offers mark unread.
        record!("mark-unread", get!("markUnreadItem", "visible"));
        record!("mark-read", get!("markReadItem", "visible"));
        record!("ordinary-search", get!("chatSearchPill", "visible"));
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

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("archived-count", get!("chats", "count"));
        record!(
            "archived-laid-out",
            call!("layout", QString::from("chatList"))
        );
        record!("archived-search", get!("chatSearchField", "visible"));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("a")
            )
        );
        record!("search-after-typing", get!("chatSearchField", "visible"));
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
        value("profiles"),
        "true",
        "the pulley does not offer Profiles with a single profile, so there \
         is no way to make a second one. {context}"
    );
    // Settings belonging to no profile: own page, reached from here (a profile's are on its row on
    // profiles page). Making group, adding contact start here too, not a page further in.
    for label in ["settings", "new-group", "qr"] {
        assert_eq!(
            value(label),
            "true",
            "the pulley does not offer {label}. {context}"
        );
    }
    assert_eq!(
        value("mark-unread"),
        "true",
        "a chat with nothing unread is not offered to be marked unread. {context}"
    );
    assert_eq!(
        value("mark-read"),
        "false",
        "a chat with nothing unread is offered to be marked read. {context}"
    );
    assert_eq!(
        value("ordinary-search"),
        "true",
        "the ordinary list lost its Search pill. {context}"
    );
    assert_eq!(
        value("archived"),
        "ok",
        "the archived list did not load. {context}"
    );
    // Guard: with something archived, field shown either way and test proves nothing.
    assert_eq!(
        value("archived-count"),
        "0",
        "the archive is not empty, so nothing here is under test. {context}"
    );
    assert_eq!(
        value("archived-search"),
        "false",
        "an empty archive still shows a search field, which can only ever \
         return nothing. {context}"
    );
    assert_eq!(
        value("search-after-typing"),
        "true",
        "the field vanished with text still in it, so there is no way to \
         clear it and get the list back. {context}"
    );
}
