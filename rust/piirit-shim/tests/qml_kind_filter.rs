//! Chat list's Chats and Channels pills: offered once a channel exists, one kind at a time,
//! either with Unread. Empty list names filter that emptied it and pill that brings chats back.

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
fn the_chats_and_channels_pills_filter_by_kind() {
    let temp = std::env::temp_dir().join(format!("piirit-kind-pills-{}", std::process::id()));
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
    // Pills' checked marks, '/'-joined: unread/chats/channels.
    macro_rules! marks {
        () => {
            format!(
                "{}/{}/{}",
                get!("unreadFilterButton", "checked"),
                get!("chatsFilterButton", "checked"),
                get!("channelsFilterButton", "checked")
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
            record!("chats-pill", get!("chatsFilterButton", "visible"));
            record!("channels-pill", get!("channelsFilterButton", "visible"));
            click!("channelsFilterButton");
        }),
    );
    plan.until(
        1,
        30,
        at_count!("1", {
            record!("channels", marks!());
            click!("chatsFilterButton");
        }),
    );
    // Chats replaces Channels: count stays 1, marks tell.
    plan.after(1, move || unsafe {
        record!("chats", format!("{} {}", get!("chats", "count"), marks!()));
        click!("unreadFilterButton");
    });
    plan.until(
        1,
        30,
        at_count!("0", {
            record!("unread-chats", format!("{} {}", marks!(), empty!()));
            click!("channelsFilterButton");
        }),
    );
    // Still empty: placeholder words follow rows, so wait on the mark, then the words.
    plan.after(1, move || unsafe {
        record!("unread-channels", format!("{} {}", marks!(), empty!()));
        click!("unreadFilterButton");
    });
    plan.until(
        1,
        30,
        at_count!("1", {
            click!("chatsFilterButton");
            call!("deleteChat", 1u32);
        }),
    );
    // Only the channel left: Chats shows nothing; tapped again, it lets go.
    plan.until(
        1,
        30,
        at_count!("0", {
            record!("only-channels", format!("{} {}", marks!(), empty!()));
            click!("chatsFilterButton");
        }),
    );
    plan.until(
        1,
        30,
        at_count!("1", {
            record!("chats-off", marks!());
            // Marks follow taps at once: setter answers before rows do.
            click!("channelsFilterButton");
            click!("channelsFilterButton");
            record!("channels-off", marks!());
            click!("channelsFilterButton");
            call!("deleteChat", 2u32);
        }),
    );
    plan.until(1, 30, move |last| unsafe {
        if get!("chatsFilterButton", "visible") != "false" && !last {
            return false;
        }
        record!(
            "no-channels",
            format!(
                "{} {} {} {}",
                get!("chatsFilterButton", "visible"),
                get!("channelsFilterButton", "visible"),
                marks!(),
                empty!()
            )
        );
        (*engine_ptr).quit();
        true
    });

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
        (
            value("chats-pill").as_str(),
            value("channels-pill").as_str()
        ),
        ("true", "true"),
        "a list with a channel offers no Chats or Channels pill. {context}"
    );
    assert_eq!(
        value("channels"),
        "false/false/true",
        "Channels did not light up alone. {context}"
    );
    assert_eq!(
        value("chats"),
        "1 false/true/false",
        "tapping Chats left Channels on, or showed both kinds. {context}"
    );
    assert_eq!(
        value("unread-chats"),
        "true/true/false No unread chats | Tap Unread to show all chats",
        "Unread did not combine with Chats, or the empty list says the wrong thing. {context}"
    );
    assert_eq!(
        value("unread-channels"),
        "true/false/true No unread channels | Tap Unread to show all channels",
        "Unread did not combine with Channels, or the empty list says the wrong thing. \
         {context}"
    );
    assert_eq!(
        value("only-channels"),
        "false/true/false No chats yet | Tap Chats to show all chats",
        "with only channels left, Chats' empty list does not point back to all chats. \
         {context}"
    );
    assert_eq!(
        value("chats-off"),
        "false/false/false",
        "tapping a lit Chats did not turn it off. {context}"
    );
    assert_eq!(
        value("channels-off"),
        "false/false/false",
        "tapping a lit Channels did not turn it off. {context}"
    );
    assert_eq!(
        value("no-channels"),
        "false false false/false/false No chats yet | Pull down to start one",
        "with the last channel gone, the kind pills stayed, or their filter stayed on behind \
         them. {context}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
