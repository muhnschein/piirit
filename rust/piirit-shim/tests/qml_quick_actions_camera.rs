//! Camera quick action from cover: chat list comes front, opens camera (still or video,
//! back or front). Taken item is asked of a chat in camera's place; chat opens in picker's
//! place with it on attachment bar, told whether to send at once. Backing out of picker, or
//! another camera action over it, throws it away; a chat that took it keeps it until sent.
//! Reported after camera left: thrown away without asking.

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

/// Probe, components dir filled in; see `qml_chat_list.rs`.
fn probe_qml() -> String {
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    import 'file://__COMPONENTS__'
    Item {
        id: probe

        // What the pages the list opens hand back: a camera that answers
        // with a file, and a picker that answers with a chat.
        Component {
            id: cameraPage
            QtObject { signal picked(string path) }
        }
        Component {
            id: pickerPage
            QtObject { signal chatPicked(int chatId, string chatName) }
        }

        // The page stack and the window, as far as the chat list reads
        // them. Each move is logged with what it was handed.
        property QtObject pageStack: QtObject {
            property var currentPage: null
            property string log: ''
            property var camera: null
            property var picker: null
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                log += 'push:' + name(page) + ':' + JSON.stringify(properties) + '|'
                if (name(page) === 'CapturePage.qml') {
                    camera = cameraPage.createObject(probe)
                    currentPage = camera
                    return camera
                }
                return null
            }
            function replace(page, properties) {
                log += 'replace:' + name(page) + ':' + JSON.stringify(properties) + '|'
                if (name(page) === 'ChatPickerPage.qml') {
                    picker = pickerPage.createObject(probe)
                    currentPage = picker
                    return picker
                }
                return null
            }
            function replaceAbove(target, page, properties) {
                log += 'replaceAbove:' + name(page) + ':' + JSON.stringify(properties) + '|'
            }
            function pop() { log += 'pop|' }
        }
        property QtObject appWindow: QtObject {
            property int accountId: 0
            property Item chatList: null
            property int raised: 0
            property bool quickActionsPaused: false
            function activate() { raised += 1 }
        }

        Captures { id: captures }
        function newCapture() { return captures.new_path('photo', 'jpg') }

        Loader { id: loader }
        function load(url, properties) {
            loader.setSource('', {})
            loader.setSource(url, JSON.parse(properties))
            pageStack.currentPage = loader.item
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // From the list on top, so nothing is popped first.
        function act(kind, sendNow) {
            pageStack.currentPage = loader.item
            loader.item.quickAction(kind, 0, 0, sendNow === 'true')
            return 'ok'
        }
        function take(path) { pageStack.camera.picked(path); return 'ok' }
        function choose(chatId, name) { pageStack.picker.chatPicked(chatId, name); return 'ok' }
        // The list on screen again: the reader came back to it.
        function arrive() {
            pageStack.currentPage = loader.item
            loader.item.status = PageStatus.Inactive
            loader.item.status = PageStatus.Active
            return 'ok'
        }
        function leave() { loader.item.status = PageStatus.Inactive; return 'ok' }
        function stackLog() {
            var log = pageStack.log
            pageStack.log = ''
            return log
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_camera_quick_action_takes_a_picture_and_asks_which_chat_it_is_for() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-quick-camera-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        std::env::set_var("XDG_CACHE_HOME", temp.join("cache"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml().as_str()));

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
    macro_rules! act {
        ($kind:expr, $now:expr) => {
            call!("act", QString::from($kind), QString::from($now))
        };
    }
    // Capture the camera wrote, on disk in captures dir.
    macro_rules! capture {
        () => {{
            let path = call!("newCapture");
            std::fs::write(&path, b"a picture").expect("write a capture");
            path
        }};
    }
    let exists = |path: &str| std::path::Path::new(path).exists().to_string();

    let list = common::page_url("ChatListPage.qml");

    // SAFETY: callbacks run only inside `exec()` on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(list.clone()),
                QString::from(r#"{"accountId": 1}"#)
            )
        );
        call!("arrive");
        call!("stackLog");

        // Each kind opens camera as it says, asking camera not to take itself down: picker goes
        // in its place.
        for (label, kind) in [
            ("photo-stack", "photo"),
            ("video-stack", "video"),
            ("selfie-stack", "selfie"),
            ("selfie-video-stack", "selfie-video"),
        ] {
            act!(kind, "false");
            record!(label, call!("stackLog"));
        }

        // Photo taken: which chat, in camera's place.
        let kept = capture!();
        record!("kept-path", kept.clone());
        act!("photo", "false");
        call!("stackLog");
        call!("leave");
        record!("take", call!("take", QString::from(kept.as_str())));
        record!("picker-stack", call!("stackLog"));
        // Chat picked: opens in picker's place with photo, waiting for send.
        record!("choose", call!("choose", 5, QString::from("Lake house")));
        record!("chat-stack", call!("stackLog"));
        // Back to list from that chat: photo still the chat's.
        call!("arrive");
        record!("kept-after-return", exists(&kept));

        // Set to send at once: chat told.
        let now = capture!();
        act!("selfie", "true");
        call!("stackLog");
        call!("leave");
        call!("take", QString::from(now.as_str()));
        call!("stackLog");
        call!("choose", 6, QString::from("Mum"));
        record!("now-stack", call!("stackLog"));
        call!("arrive");

        // Backed out of picker: nothing sends it, so it goes.
        let dropped = capture!();
        act!("video", "false");
        call!("leave");
        call!("take", QString::from(dropped.as_str()));
        record!("dropped-before-return", exists(&dropped));
        call!("arrive");
        record!("dropped-after-return", exists(&dropped));
        record!("dropped-stack", call!("stackLog"));

        // Camera left mid-video: file reported late; nobody asked, it goes.
        let late = capture!();
        act!("video", "false");
        call!("arrive");
        call!("stackLog");
        call!("take", QString::from(late.as_str()));
        record!("late-stack", call!("stackLog"));
        record!("late-kept", exists(&late));

        // Tapped again while picker asks about last one: that one goes nowhere now.
        let earlier = capture!();
        act!("photo", "false");
        call!("leave");
        call!("take", QString::from(earlier.as_str()));
        call!("stackLog");
        act!("video", "false");
        record!("earlier-kept", exists(&earlier));
        call!("arrive");

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

    assert_eq!(value("load"), "ok", "the chat list did not load. {context}");
    for (label, mode, front) in [
        ("photo-stack", 0, false),
        ("video-stack", 1, false),
        ("selfie-stack", 0, true),
        ("selfie-video-stack", 1, true),
    ] {
        assert_eq!(
            value(label),
            format!(
                r#"push:CapturePage.qml:{{"mode":{mode},"front":{front},"closeOnPick":false}}|"#
            ),
            "{label}: the camera is not opened as the action says. {context}"
        );
    }

    let kept = value("kept-path");
    assert!(
        kept.contains("/captures/"),
        "the capture was not made in the captures directory: {kept}. {context}"
    );
    assert_eq!(
        value("picker-stack"),
        r#"replace:ChatPickerPage.qml:{"accountId":1,"closeOnPick":false,"title":"Send to","emptyText":"No chats yet"}|"#,
        "the photo is not asked a chat for in the camera's place. {context}"
    );
    assert_eq!(
        value("chat-stack"),
        format!(
            r#"replace:ConversationPage.qml:{{"accountId":1,"chatId":5,"chatName":"Lake house","sharedFile":{kept:?},"sendShared":false}}|"#
        ),
        "the chat picked does not open in the picker's place with the photo \
         waiting on its bar. {context}"
    );
    assert_eq!(
        value("kept-after-return"),
        "true",
        "a capture handed to a chat was thrown away before it was sent. \
         {context}"
    );
    let now = value("now-stack");
    assert!(
        now.starts_with("replace:ConversationPage.qml:")
            && now.contains(r#""chatId":6"#)
            && now.contains(r#""sendShared":true"#),
        "a chat picked for an action set to send at once is not told to: \
         {now}. {context}"
    );
    assert_eq!(
        (
            value("dropped-before-return").as_str(),
            value("dropped-after-return").as_str()
        ),
        ("true", "false"),
        "a capture nobody picked a chat for is not thrown away once the \
         reader is back on the list, or is thrown away before. {context}"
    );
    assert!(
        !value("dropped-stack").contains("ConversationPage"),
        "a chat was opened that nobody picked. {context}"
    );
    assert_eq!(
        (value("late-stack").as_str(), value("late-kept").as_str()),
        ("", "false"),
        "a capture reported after the camera had left put a picker in \
         place of whatever was there, or was kept. {context}"
    );
    assert_eq!(
        value("earlier-kept"),
        "false",
        "a capture still waiting for a chat was kept when the camera was \
         opened again over it. {context}"
    );
}
