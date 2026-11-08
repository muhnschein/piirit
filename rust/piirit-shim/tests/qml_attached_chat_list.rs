//! Attached chat list (after backup, handover), swiped to: stack pops at once to window's
//! list of same profile; finished pages can't be swiped back into (#201).

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe

        // Stack as far as chat list reads it: each pop logged with target and how.
        property QtObject pageStack: QtObject {
            property bool busy: false
            property var currentPage: null
            property string log: ''
            function pop(target, operation) {
                var whom = target === windowLoader.item ? 'window-list'
                         : target === undefined ? 'top' : 'other'
                var how = operation === PageStackAction.Immediate ? 'immediate' : 'animated'
                log += 'pop:' + whom + ':' + how + '|'
            }
            function push(page, properties) { log += 'push|'; return null }
            function replaceAbove(target, page, properties) { log += 'replaceAbove|' }
        }
        property QtObject appWindow: QtObject {
            property int accountId: 0
            property Item chatList: null
            function activate() {}
        }

        // Window's list, stack bottom; flow pages over it.
        Loader { id: windowLoader }
        // List attached right of finished page, as BackupPage, SecondDevicePage put it.
        Loader { id: attachedLoader }

        function loadWindow(url, properties) {
            windowLoader.setSource(url, JSON.parse(properties))
            return windowLoader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Attached page waits off screen till swiped to.
        function attach(url, properties) {
            var given = JSON.parse(properties)
            given.status = PageStatus.Inactive
            attachedLoader.setSource('', {})
            attachedLoader.setSource(url, given)
            return attachedLoader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Page swiped to: on screen, as stack puts it there.
        function arrive(which) {
            var page = which === 'window' ? windowLoader.item : attachedLoader.item
            page.status = PageStatus.Inactive
            page.status = PageStatus.Active
            return 'ok'
        }
        function busy(on) { pageStack.busy = on === 'true'; return 'ok' }
        // Call page pushed over attached list, then gone.
        function cover() { attachedLoader.item.status = PageStatus.Inactive; return 'ok' }
        function uncover() { attachedLoader.item.status = PageStatus.Active; return 'ok' }
        function backOpen() { return '' + attachedLoader.item.backNavigation }
        function registered() {
            return appWindow.chatList === windowLoader.item ? 'window-list' : 'other'
        }
        function stackLog() {
            var log = pageStack.log
            pageStack.log = ''
            return log
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn swiping_to_the_attached_chat_list_leaves_the_finished_flow_behind() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-attached-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server start. Two profiles: attached list of
    // other one possible.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    let list = common::page_url("ChatListPage.qml");
    let other = list.clone();

    let mut plan = common::Plan::default();

    // SAFETY: callbacks run only inside `exec()` on this thread; all they point at outlives it.
    plan.after(0, move || unsafe {
        let url = || QString::from(list.clone());
        record!(
            "window",
            call!("loadWindow", url(), QString::from(r#"{"accountId": 1}"#))
        );
        record!("registered", call!("registered"));

        // Window's own list back on screen: nothing to end.
        call!("arrive", QString::from("window"));
        record!("window-arrived", call!("stackLog"));

        // Backup done, chats attached right: nothing moves till swiped to.
        record!(
            "attach",
            call!(
                "attach",
                url(),
                QString::from(r#"{"accountId": 1, "attached": true}"#)
            )
        );
        record!("attached-waiting", call!("stackLog"));
        record!("attached-registered", call!("registered"));

        // Swiped to.
        call!("arrive", QString::from("attached"));
        record!("arrived", call!("stackLog"));

        // Archived list (pushed over window's, same profile) back on screen: not a flow end.
        call!(
            "attach",
            url(),
            QString::from(r#"{"accountId": 1, "archived": true}"#)
        );
        call!("arrive", QString::from("attached"));
        record!("archived-arrived", call!("stackLog"));

        // Swiped to while stack still settling: waits, back closed. Call page comes over
        // before it settles: move waits for list on top again.
        call!(
            "attach",
            url(),
            QString::from(r#"{"accountId": 1, "attached": true}"#)
        );
        call!("busy", QString::from("true"));
        call!("arrive", QString::from("attached"));
        record!("busy-arrived", call!("stackLog"));
        record!("busy-back", call!("backOpen"));
        call!("cover");
        call!("busy", QString::from("false"));
    });

    // Retry timer 50 ms: a second covered is many retries.
    plan.after(1, move || unsafe {
        record!("covered", call!("stackLog"));
        call!("uncover");
        record!("uncovered", call!("stackLog"));
        record!("uncovered-back", call!("backOpen"));

        // List of other profile attached: not this flow, stays.
        call!(
            "attach",
            QString::from(other.clone()),
            QString::from(r#"{"accountId": 2, "attached": true}"#)
        );
        call!("arrive", QString::from("attached"));
        record!("other-arrived", call!("stackLog"));
        (*engine_ptr).quit();
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
        value("window"),
        "ok",
        "the chat list did not load. {context}"
    );
    assert_eq!(value("attach"), "ok", "{context}");
    assert_eq!(value("registered"), "window-list", "{context}");
    assert_eq!(
        value("window-arrived"),
        "",
        "the window's own chat list moved the stack on coming back. {context}"
    );
    assert_eq!(
        value("attached-waiting"),
        "",
        "the attached chat list moved the stack before it was swiped to. {context}"
    );
    assert_eq!(
        value("attached-registered"),
        "window-list",
        "the attached chat list took the window's quick actions. {context}"
    );
    assert_eq!(
        value("arrived"),
        "pop:window-list:immediate|",
        "swiping on from a finished backup left it one swipe back: the stack \
         did not drop at once to the window's chat list. {context}"
    );
    assert_eq!(
        value("archived-arrived"),
        "",
        "the archived chat list popped the window's list away on coming back. \
         {context}"
    );
    assert_eq!(
        (value("busy-arrived").as_str(), value("busy-back").as_str()),
        ("", "false"),
        "with the stack still moving, the hand-over was made anyway (Silica \
         drops it) or the way back stayed open meanwhile. {context}"
    );
    assert_eq!(
        value("covered"),
        "",
        "the hand-over was made with a page over the list: it would take \
         that page with it. {context}"
    );
    assert_eq!(
        value("uncovered"),
        "pop:window-list:immediate|",
        "the hand-over was lost once the list was on top again. {context}"
    );
    assert_eq!(value("uncovered-back"), "true", "{context}");
    assert_eq!(
        value("other-arrived"),
        "",
        "a chat list of another profile popped the window's list away. \
         {context}"
    );
}
