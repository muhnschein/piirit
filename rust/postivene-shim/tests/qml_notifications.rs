//! Who gets told about an arriving message, what they are told, and what
//! a tap on the telling does.
//!
//! A notification for a chat the reader is already looking at is noise; a
//! notification that outlives the reader arriving is worse, because it
//! trains them to ignore the next one. What one says is the reader's
//! choice, since the lock screen shows it to whoever is looking; and a
//! tap on it has to come back to the chat it stands for.

// Qt harness: see qml_chat_row.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property int opened: 0
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.openRequested.connect(function(chatId) { opened = chatId })
            return 'ok'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function arrived(chatId, name, sender, preview) {
            loader.item.arrived(chatId, name, sender, preview)
            return 'ok'
        }
        function published() {
            return '' + loader.item.publishedCount()
        }
        // What one chat's notification is currently saying.
        function saying(chatId) {
            var note = loader.item.notes[chatId]
            return note ? note.summary + '/' + note.body : 'none'
        }
        function counted(chatId) {
            var note = loader.item.notes[chatId]
            return note ? '' + note.itemCount : 'none'
        }
        function nameOf(chatId) {
            return loader.item.nameOf(chatId)
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
        // What lipstick does with a tap: calls the method the
        // notification's remote action names, on the name it names, with
        // the arguments it carries. The adaptor answering to that name is
        // the notifier's own.
        function tap(chatId) {
            var note = loader.item.notes[chatId]
            if (!note || !note.remoteActions || note.remoteActions.length === 0) {
                return 'no-action'
            }
            var action = note.remoteActions[0]
            var adaptor = findIn(loader.item, 'notifierAdaptor')
            if (!adaptor) { return 'no-adaptor' }
            if (adaptor.service !== action.service || adaptor.iface !== action.iface
                    || adaptor.path !== action.path) {
                return 'wrong-name:' + action.service + action.path + action.iface
            }
            if (typeof adaptor[action.method] !== 'function') {
                return 'no-method:' + action.method
            }
            adaptor[action.method](action.arguments[0])
            return '' + opened
        }
        function identity(chatId) {
            var note = loader.item.notes[chatId]
            return note ? note.appName + '/' + note.appIcon + '/' + note.category : 'none'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_message_is_announced_unless_the_reader_is_already_in_that_chat() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! arrived {
        ($chat:expr, $name:expr, $sender:expr, $preview:expr) => {
            call!(
                "arrived",
                $chat,
                QString::from($name),
                QString::from($sender),
                QString::from($preview)
            )
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::component_url("Notifier.qml")))
        );
        call!("set", QString::from("appActive"), true);
        call!("set", QString::from("viewingChatId"), 0);
        call!("set", QString::from("detail"), 0);

        // Nobody is reading anything: both chats speak up. In the group
        // the message is Bob's, and the notification says so.
        arrived!(7, "Ada", "", "see you there");
        arrived!(9, "Hikers", "Bob", "ping");
        record!("two-chats", call!("published"));
        record!("ada-says", call!("saying", 7));
        record!("hikers-say", call!("saying", 9));
        record!("identity", call!("identity", 7));
        record!("one-counted", call!("counted", 7));

        // A second message in the same chat counts up on the one
        // notification rather than raising another.
        arrived!(7, "Ada", "", "and another");
        record!("still-two-chats", call!("published"));
        record!("two-counted", call!("counted", 7));

        // Walking into Ada's chat takes Ada's notification down, and
        // leaves Bob's alone. The count starts over with it.
        call!("set", QString::from("viewingChatId"), 7);
        record!("after-opening-ada", call!("published"));

        // A second message from Ada, while Ada is on screen, is not news.
        arrived!(7, "Ada", "", "still here");
        record!("while-reading-ada", call!("published"));

        // The same message with the app in the background is news again:
        // on screen is not the same as seen. And it is the first since
        // the notification came down.
        call!("set", QString::from("appActive"), false);
        arrived!(7, "Ada", "", "and again");
        record!("while-backgrounded", call!("published"));
        record!("counted-again", call!("counted", 7));

        // Less said, by the reader's choice: the chat only, then only
        // that something arrived. The name is kept aside either way, for
        // the page to open the chat under.
        call!("set", QString::from("detail"), 1);
        arrived!(11, "Grace", "", "the secret is");
        record!("name-only", call!("saying", 11));
        call!("set", QString::from("detail"), 2);
        arrived!(13, "Eve", "", "psst");
        record!("arrival-only", call!("saying", 13));
        record!("name-kept", call!("nameOf", 13));

        // A tap comes back to the chat, through the name the adaptor owns.
        record!("tap", call!("tap", 13));

        // Notifications off: whatever is up comes down, and nothing
        // arriving goes up, however loudly. On again, arrivals are news
        // as before.
        record!("before-off", call!("published"));
        call!("set", QString::from("enabled"), false);
        record!("after-off", call!("published"));
        arrived!(17, "Linus", "", "anyone there?");
        record!("while-off", call!("published"));
        record!("nothing-said", call!("saying", 17));
        call!("set", QString::from("enabled"), true);
        arrived!(17, "Linus", "", "hello again");
        record!("after-on", call!("published"));
        record!("said-again", call!("saying", 17));

        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Who was told, who was spared, what they were told, and where a tap
/// went.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the notifier did not load. {context}");
    assert_eq!(
        value("two-chats"),
        "2",
        "two chats talking at once did not each get a notification. {context}"
    );
    assert_eq!(
        value("ada-says"),
        "Ada/see you there",
        "the notification does not carry the chat and the message. {context}"
    );
    assert_eq!(
        value("hikers-say"),
        "Hikers/Bob: ping",
        "a group's notification does not say who in it wrote, or the \
         second chat's notification was overwritten by the first. {context}"
    );
    assert_eq!(
        value("identity"),
        "Postivene/harbour-postivene/x-nemo.messaging.im",
        "the notification does not say which app it is from, with the \
         app's icon, as a message. {context}"
    );
    assert_eq!(
        value("one-counted"),
        "1",
        "a first message is not counted as one. {context}"
    );
    assert_eq!(
        value("still-two-chats"),
        "2",
        "a second message in a chat raised a second notification for it. {context}"
    );
    assert_eq!(
        value("two-counted"),
        "2",
        "a second message in a chat was not counted on its notification. {context}"
    );
    assert_eq!(
        value("after-opening-ada"),
        "1",
        "opening a chat did not take its notification down, or took the \
         other chat's down with it. {context}"
    );
    assert_eq!(
        value("while-reading-ada"),
        "1",
        "a message was announced into the face of someone already reading \
         that chat. {context}"
    );
    assert_eq!(
        value("while-backgrounded"),
        "2",
        "a message arriving while the app was in the background went \
         unannounced because that chat happened to be the last one open. \
         {context}"
    );
    assert_eq!(
        value("counted-again"),
        "1",
        "the count did not start over after the notification came down. {context}"
    );
    assert_eq!(
        value("name-only"),
        "Grace/1 new message(s)",
        "with the chat only asked for, the notification says more, or \
         less. {context}"
    );
    assert_eq!(
        value("arrival-only"),
        "1 new message(s)/",
        "with only an arrival asked for, the notification names the chat. {context}"
    );
    assert_eq!(
        value("name-kept"),
        "Eve",
        "the chat's name was not kept for the page to open it under. {context}"
    );
    assert_eq!(
        value("tap"),
        "13",
        "a tap on the notification did not come back to its chat. {context}"
    );
    assert_ne!(
        value("before-off"),
        "0",
        "nothing was up to take down, so turning notifications off proves \
         nothing here. {context}"
    );
    assert_eq!(
        value("after-off"),
        "0",
        "notifications turned off left the ones already up. {context}"
    );
    assert_eq!(
        (value("while-off"), value("nothing-said")),
        ("0".to_string(), "none".to_string()),
        "a message arriving with notifications off was announced. {context}"
    );
    // Said the way the detail setting left it: only that something
    // arrived.
    assert_eq!(
        (value("after-on"), value("said-again")),
        ("1".to_string(), "1 new message(s)/".to_string()),
        "notifications turned back on did not announce the next arrival. \
         {context}"
    );
}
