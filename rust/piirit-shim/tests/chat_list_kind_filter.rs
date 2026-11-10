//! Chats and Channels filters: one kind at a time, either with Unread. Offered only while the
//! list holds a channel; last one gone turns them off.

// Qt harness: see chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        ChatList { id: chats; account_id: 1 }
        Repeater {
            id: rows
            model: chats.rows
            Item { property int cid: model.chat_id }
        }
        function shown() {
            var ids = []
            for (var i = 0; i < rows.count; i++) { ids.push(rows.itemAt(i).cid) }
            return ids.join(',')
        }
        function filters() {
            return (chats.unread_only ? 'unread ' : '') + (chats.chats_only ? 'chats ' : '')
                + (chats.channels_only ? 'channels ' : '') + (chats.has_channels ? 'offered' : 'hidden')
        }
        function chatsOnly(on) { chats.chats_only = on; return 'ok' }
        function channelsOnly(on) { chats.channels_only = on; return 'ok' }
        function unreadOnly(on) { chats.unread_only = on; return 'ok' }
        function markUnread(chatId) { chats.mark_unread(chatId); return 'ok' }
        function remove(chatId) { chats.delete_chat(chatId); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn chats_and_channels_split_the_list_and_combine_with_unread() {
    let temp = std::env::temp_dir().join(format!("piirit-kind-filter-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: one thread; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Chat 2 a joined channel, chat 1 a 1:1.
        std::env::set_var("PIIRIT_FAKE_CHANNELS", "2");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
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
        ($label:expr) => {
            (*steps_ptr).push(($label, format!("{} | {}", call!("shown"), call!("filters"))))
        };
    }

    let core_ptr = std::ptr::addr_of!(core_box);
    // Next step once `$what` returns `$want` (last try: as it is).
    macro_rules! once {
        ($what:expr, $want:expr, $step:expr) => {
            move |last| unsafe {
                if call!($what) != $want && !last {
                    return false;
                }
                $step;
                true
            }
        };
    }

    let mut plan = common::Plan::default();

    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    plan.after(0, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    plan.until(
        1,
        30,
        once!("shown", "1,2", {
            record!("all");
            call!("channelsOnly", true);
        }),
    );
    plan.until(
        1,
        30,
        once!("shown", "2", {
            record!("channels");
            call!("chatsOnly", true);
        }),
    );
    plan.until(
        1,
        30,
        once!("shown", "1", {
            record!("chats");
            call!("channelsOnly", true);
            call!("unreadOnly", true);
        }),
    );
    // Nothing unread yet; then both turn unread.
    plan.until(
        1,
        30,
        once!("shown", "", {
            record!("unread-channels-none");
            call!("markUnread", 1u32);
            call!("markUnread", 2u32);
        }),
    );
    plan.until(
        1,
        30,
        once!("shown", "2", {
            call!("chatsOnly", true);
        }),
    );
    plan.until(
        1,
        30,
        once!("shown", "1", {
            record!("unread-chats");
            call!("channelsOnly", true);
        }),
    );
    // Both unread now: the 1:1 must stay out.
    plan.until(
        1,
        30,
        once!("shown", "2", {
            record!("unread-channels");
            call!("remove", 2u32);
        }),
    );
    // Last channel gone: no pill to turn the filter off, so it turns itself off.
    plan.until(
        1,
        30,
        once!("filters", "unread hidden", {
            record!("channel-gone");
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
        value("all"),
        "1,2 | offered",
        "a list with a channel did not offer the kind filters. {context}"
    );
    assert_eq!(
        value("channels"),
        "2 | channels offered",
        "the channels filter did not show only the channel. {context}"
    );
    assert_eq!(
        value("chats"),
        "1 | chats offered",
        "chats did not replace channels, or showed the channel. {context}"
    );
    assert_eq!(
        value("unread-channels-none"),
        " | unread channels offered",
        "unread and channels did not combine. {context}"
    );
    assert_eq!(
        value("unread-channels"),
        "2 | unread channels offered",
        "an unread channel was missing, or an unread chat showed under channels. {context}"
    );
    assert_eq!(
        value("unread-chats"),
        "1 | unread chats offered",
        "unread and chats did not combine. {context}"
    );
    assert_eq!(
        value("channel-gone"),
        "1 | unread hidden",
        "with no channel left the kind filter stayed on, hiding every chat behind a pill \
         that is gone. {context}"
    );

    let _ = std::fs::remove_dir_all(&temp);
}

const RESTATE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string restated: ''
        ChatList {
            id: chats
            account_id: 1
            onMessage_restated: restated += chat_id + '|'
        }
        function shown() { return '' + chats.count }
        function channelsOnly() { chats.channels_only = true; return 'ok' }
        function edit(chatId) {
            chats.handle_event(1, 'MsgsChanged', JSON.stringify({ chatId: chatId, msgId: 9 }))
            return 'ok'
        }
        function heard() { return restated }
    }
";

#[test]
fn an_edit_in_a_chat_the_filter_hides_is_still_restated() {
    let temp = std::env::temp_dir().join(format!("piirit-kind-restate-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: one thread; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_CHANNELS", "2");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut heard = String::new();
    let heard_ptr: *mut String = std::ptr::addr_of_mut!(heard);

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

    let core_ptr = std::ptr::addr_of!(core_box);
    let mut plan = common::Plan::default();

    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    plan.after(0, move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(RESTATE_QML));
    });
    plan.until(1, 30, move |last| unsafe {
        if call!("shown") != "2" && !last {
            return false;
        }
        call!("channelsOnly");
        true
    });
    // Chat 1, a 1:1, now hidden; its notification still wants the edit.
    plan.until(1, 30, move |last| unsafe {
        if call!("shown") != "1" && !last {
            return false;
        }
        call!("edit", 1u32);
        true
    });
    plan.until(1, 30, move |last| unsafe {
        *heard_ptr = call!("heard");
        if heard_ptr.as_ref().is_some_and(String::is_empty) && !last {
            return false;
        }
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    assert_eq!(
        heard, "1|",
        "an edit in a chat the Channels filter hides was not restated, so its notification \
         keeps the old text"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
