//! The window's half of the cover's quick actions: whether the cover may
//! offer them, and where a tap goes.
//!
//! Not before there is a chat list to land on, and not while a page is up
//! that must not be jumped away from -- a backup, a restore, a code being
//! shown or read, anywhere in the stack. A tap is read against the
//! settings and handed to the chat list with what it needs; one of a kind
//! this version does not know goes nowhere.

// Qt harness: see qml_startup.rs.
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

/// The window, loaded the way `qml_last_profile` loads it, with the
/// components directory imported so the probe can write the settings.
fn probe_qml(components: &std::path::Path) -> String {
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        id: probe

        // The stack as far as the window reads it: the page on top, and
        // the one under each page.
        property QtObject pageStack: QtObject {
            property var currentPage: null
            property int depth: 0
            property var below: ({})
            function previousPage(page) {
                return page && page.objectName in below ? below[page.objectName] : null
            }
        }
        QtObject { id: backup; objectName: 'backup'; property bool pausesQuickActions: true }
        QtObject { id: conversation; objectName: 'conversation' }
        QtObject { id: chats; objectName: 'chats' }

        // Where the window hands a tap: what the chat list was asked to do.
        Item {
            id: fakeList
            property string asked: ''
            function quickAction(kind, accountId, chatId) {
                asked += kind + '|' + accountId + '|' + chatId + ';'
            }
        }

        Loader { id: loader }
        property var cover: null
        function load(url) {
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            cover = loader.item.cover.createObject(probe, { width: 240, height: 360 })
            return cover ? 'ok' : 'no-cover'
        }
        function allowed() { return '' + cover.quickActionsAllowed }
        function land(there) {
            loader.item.chatList = there === 'true' ? fakeList : null
            return 'ok'
        }
        function count(n) { Settings.quickActionCount = n; return 'ok' }
        function set(side, kind, account, chat) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            Settings[prefix] = kind
            Settings[prefix + 'Account'] = account
            Settings[prefix + 'Chat'] = chat
            return 'ok'
        }
        function tap(side) {
            loader.item.quickAction(side)
            return fakeList.asked
        }
        // A tap on the cover itself, which the window is listening to.
        function tapCover(side) {
            cover.quickAction(side)
            return fakeList.asked
        }
        // What is on the stack, top first, by name.
        function stack(names) {
            var pages = { backup: backup, conversation: conversation, chats: chats }
            var list = names.length > 0 ? names.split(',') : []
            var below = {}
            for (var i = 0; i + 1 < list.length; i++) {
                below[list[i]] = pages[list[i + 1]]
            }
            pageStack.below = below
            pageStack.currentPage = list.length > 0 ? pages[list[0]] : null
            pageStack.depth = list.length
            return '' + loader.item.quickActionsPaused + '|' + cover.quickActionsAllowed
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_window_offers_quick_actions_when_it_can_take_them_and_passes_them_on() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-quick-window-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; see common::qml_tree_without_enter_key.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();
    common::register_cover_enum();
    common::register_dbus_enum();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    // The real one is handed in by main.rs; the window starts the core
    // with it.
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(env!("CARGO_BIN_EXE_fake-core-server")).into(),
    );
    engine.load_data(QByteArray::from(
        probe_qml(&tree.join("components")).as_str(),
    ));

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
    macro_rules! set {
        ($side:expr, $kind:expr, $account:expr, $chat:expr) => {
            call!(
                "set",
                QString::from($side),
                QString::from($kind),
                $account,
                $chat
            )
        };
    }

    let root = format!("file://{}", tree.join("piirit.qml").display());

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!("room-for-two", call!("count", 2));
        record!("set-left", set!("left", "search", 0, 0));
        record!("set-right", set!("right", "chat", 2, 7));
        record!("load", call!("load", QString::from(root.clone())));

        // Nothing to land on yet: a phone still on its way to a profile.
        record!("no-list", call!("allowed"));
        record!("no-list-tap", call!("tap", QString::from("left")));

        record!("land", call!("land", QString::from("true")));
        record!("listed", call!("allowed"));
        record!("tap-left", call!("tap", QString::from("left")));
        record!("tap-right", call!("tap", QString::from("right")));
        record!("tap-cover", call!("tapCover", QString::from("left")));

        // With room for one, the right one is not there to be tapped.
        record!("room-for-one", call!("count", 1));
        record!("tap-held", call!("tap", QString::from("right")));
        record!("back-to-two", call!("count", 2));

        // A kind this version does not know goes nowhere.
        record!("set-odd", set!("left", "teapot", 0, 0));
        record!("tap-odd", call!("tap", QString::from("left")));

        // The stack decides, wherever in it the page that pauses is.
        record!("plain", call!("stack", QString::from("conversation,chats")));
        record!("paused-top", call!("stack", QString::from("backup,chats")));
        record!(
            "paused-below",
            call!("stack", QString::from("conversation,backup,chats"))
        );
        record!("resumed", call!("stack", QString::from("chats")));

        // And the list going takes the actions with it.
        record!("unland", call!("land", QString::from("false")));
        record!("unlisted", call!("allowed"));
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
        value("load"),
        "ok",
        "the window or its cover did not load. {context}"
    );
    assert_eq!(
        value("no-list"),
        "false",
        "the cover offers quick actions with no chat list to land on. {context}"
    );
    assert_eq!(
        value("no-list-tap"),
        "",
        "a tap with no chat list went somewhere. {context}"
    );
    assert_eq!(
        value("listed"),
        "true",
        "the cover offers no quick actions with a chat list there. {context}"
    );
    assert_eq!(
        value("tap-left"),
        "search|0|0;",
        "the left action is not handed to the chat list. {context}"
    );
    assert_eq!(
        value("tap-right"),
        "search|0|0;chat|2|7;",
        "a chat's action is not handed over with its profile and chat. \
         {context}"
    );
    assert_eq!(
        value("tap-cover"),
        "search|0|0;chat|2|7;search|0|0;",
        "the window does not act on a tap on the cover. {context}"
    );
    assert_eq!(
        value("tap-held"),
        "search|0|0;chat|2|7;search|0|0;",
        "the right action was handed on with room for one. {context}"
    );
    assert_eq!(
        value("tap-odd"),
        "search|0|0;chat|2|7;search|0|0;",
        "an action of a kind this version does not know was handed on. \
         {context}"
    );
    assert_eq!(
        value("plain"),
        "false|true",
        "an ordinary stack pauses the quick actions. {context}"
    );
    assert_eq!(
        value("paused-top"),
        "true|false",
        "a page that pauses the quick actions does not, on top. {context}"
    );
    assert_eq!(
        value("paused-below"),
        "true|false",
        "a page that pauses the quick actions does not, under another. \
         {context}"
    );
    assert_eq!(
        value("resumed"),
        "false|true",
        "the quick actions do not come back once the page has gone. {context}"
    );
    assert_eq!(
        value("unlisted"),
        "false",
        "the cover still offers quick actions once the chat list has gone. \
         {context}"
    );
}
