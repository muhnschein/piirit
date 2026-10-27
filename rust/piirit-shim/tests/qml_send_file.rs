//! Attach flow: picked file -> bar above field; send carries it; bar clears.
//! Page loaded from QML tree copy without `EnterKey` (why: `common::qml_tree_without_enter_key`).
//! Pickers not driven (pushed; no page stack here), but `attach()` is what their handlers
//! call, so all after choosing is covered.

// Qt harness: see qml_conversation_open.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // Created the way pageStack.push does, still on its way in: the
        // page hands the chat to its model on the transition to Active, not
        // from a binding, so a page born Active never fetches anything.
        function load(url, accountId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                chatId: chatId,
                status: PageStatus.Activating
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function settle() { loader.item.status = PageStatus.Active; return 'ok' }
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
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // What the pickers' handlers call once something has been chosen.
        function attach(path) { loader.item.attach(path); return 'ok' }
        function send() { loader.item.sendCurrentText(); return 'ok' }
        function pending() { return '' + loader.item.attachmentPath }
        // The tray is closed until the plus is tapped, and the choices in
        // it must reach the page rather than stopping at the component.
        function toggleTray() {
            var button = findIn(loader.item, 'attachButton')
            if (!button) { return 'missing:attachButton' }
            button.open = !button.open
            return '' + button.open
        }
        // A tap anywhere else while the tray is open lands on the area
        // the page puts under it, and closes it. Its clicked() carries
        // the event, and QML refuses to emit it without one.
        function tapBesideTray() {
            var area = findIn(loader.item, 'trayDismiss')
            if (!area) { return 'missing:trayDismiss' }
            area.clicked(null)
            return 'ok'
        }
    }
";

/// Checks, a second apart from a second after send, before a page still holding the file is
/// recorded: the regression this test exists for. nextest warns at 30s.
const PATIENCE: u32 = 8;

/// Records pending file once cleared; stops engine.
/// Polled once a second, not read once: send delay is wide so earlier reads land mid-send, and a
/// fixed read 2s later left 0.5s margin a busy runner ate (false regression). Never-clearing page
/// recorded on last check and fails same assertion.
unsafe fn settle_pending(
    plan: &mut common::Plan,
    engine: *mut QmlEngine,
    steps: *mut Vec<(&'static str, String)>,
) {
    plan.until(1, PATIENCE, move |last| unsafe {
        let answer = (*engine).invoke_method("pending".into(), &[]);
        let pending = QString::from_qvariant(answer)
            .map(|value| value.to_string())
            .unwrap_or_default();
        if !pending.is_empty() && !last {
            return false;
        }
        (*steps).push(("pending-after-send", pending));
        (*engine).quit();
        true
    });
}

#[test]
fn a_picked_file_shows_above_the_field_and_leaves_with_the_message() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-send-file-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Wide: reads land mid-send, the state send button must refuse in.
        std::env::set_var("PIIRIT_FAKE_SEND_DELAY_MS", "1500");
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

    /// Record one item's property under `label`.
    macro_rules! probe {
        ($label:expr, $name:expr, $property:expr) => {
            (*steps_ptr).push((
                $label,
                call!("get", QString::from($name), QString::from($property)),
            ))
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url_in(&tree, "ConversationPage.qml")),
                1,
                1
            ),
        ));
        probe!("bar-before", "attachmentBar", "visible");
        probe!("send-enabled-before", "sendButton", "enabled");
        probe!("tray-closed", "attachTray", "visible");
        probe!("dismiss-before", "trayDismiss", "visible");
        (*steps_ptr).push(("tray-open", call!("toggleTray")));
        probe!("dismiss-armed", "trayDismiss", "visible");
        (*steps_ptr).push(("tap-beside", call!("tapBesideTray")));
        probe!("tray-after-tap", "attachTray", "visible");
        (*steps_ptr).push(("tray-open-again", call!("toggleTray")));
        (*steps_ptr).push(("settle", call!("settle")));
    });

    plan.after(2, move || unsafe {
        (*steps_ptr).push((
            "attach",
            call!(
                "attach",
                QString::from("/tmp/piirit-fake/holiday photo.png")
            ),
        ));
    });

    plan.after(1, move || unsafe {
        probe!("bar-after", "attachmentBar", "visible");
        probe!("bar-text", "pendingAttachmentLabel", "text");
        // A file with no caption is still a message.
        probe!("send-enabled-after", "sendButton", "enabled");
        probe!("placeholder", "messageField", "placeholderText");
        (*steps_ptr).push(("send", call!("send")));
        // Same callback: core not answered yet.
        probe!("send-enabled-during", "sendButton", "enabled");
        probe!("busy-during", "sendBusy", "running");
    });

    unsafe { settle_pending(&mut plan, engine_ptr, steps_ptr) };

    plan.run();
    engine.exec();

    assert_tray(&steps);
    assert_outcome(&steps, &journal);
}

/// Run's recorded values, by label.
fn value_of<'a>(steps: &'a [(&str, String)], label: &str) -> &'a str {
    steps
        .iter()
        .find(|(name, _)| *name == label)
        .map_or("", |(_, value)| value.as_str())
}

/// Tray: closed until plus; tap beside closes it; next plus reopens.
fn assert_tray(steps: &[(&str, String)]) {
    let value = |label: &str| value_of(steps, label);
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("tray-closed"),
        "false",
        "the attach tray is open before the plus was tapped, over the conversation. {context}"
    );
    assert_eq!(
        value("tray-open"),
        "true",
        "the plus does not open the tray. {context}"
    );
    assert_eq!(
        value("dismiss-before"),
        "false",
        "the area that closes the tray is there with the tray closed, and would eat a tap on the conversation. {context}"
    );
    assert_eq!(
        value("dismiss-armed"),
        "true",
        "nothing under the open tray takes a tap beside it. {context}"
    );
    assert_eq!(
        value("tray-after-tap"),
        "false",
        "a tap beside the open tray does not close it. {context}"
    );
    assert_eq!(
        value("tray-open-again"),
        "true",
        "the plus does not open the tray a second time. {context}"
    );
}

/// Run's assertions, out of test body (Qt harness limit).
fn assert_outcome(steps: &[(&str, String)], journal: &std::path::Path) {
    let value = |label: &str| value_of(steps, label);
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("bar-before"),
        "false",
        "the attachment bar is showing before anything has been picked. {context}"
    );
    assert_eq!(
        value("send-enabled-before"),
        "false",
        "an empty field with nothing attached still offers to send. {context}"
    );
    assert_eq!(
        value("bar-after"),
        "true",
        "a picked file left no sign of itself above the field. {context}"
    );
    assert!(
        value("bar-text").contains("holiday photo.png"),
        "the bar does not name the picked file. {context}"
    );
    assert!(
        !value("bar-text").contains('/'),
        "the bar shows the whole path rather than the file's name. {context}"
    );
    assert_eq!(
        value("send-enabled-after"),
        "true",
        "a file with no caption cannot be sent. {context}"
    );
    assert_eq!(
        value("placeholder"),
        "Caption",
        "the field still asks for a message when it is asking for a caption. {context}"
    );

    // Picture sent as `Image`: core recodes it (`media.rs`).
    let sends: Vec<(String, Value)> = common::calls(journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .collect();
    assert_eq!(
        sends.first(),
        Some(&(
            "send_msg".to_string(),
            serde_json::json!([
                1,
                1,
                {
                    "text": null,
                    "file": "/tmp/piirit-fake/holiday photo.png",
                    "filename": "holiday photo.png",
                    "viewtype": "Image",
                    "quotedMessageId": null,
                }
            ])
        )),
        "the page's send did not carry the picked file. {context}. Sends: {sends:?}"
    );

    // Cleared by `onSent` with model's answer, not optimistically on way out (would drop file
    // on failed send).
    assert_eq!(
        value("send-enabled-during"),
        "false",
        "the send button is still live while the send is in flight, so a \
         second tap sends the whole thing again -- which is what an \
         impatient thumb does to a large video. {context}"
    );
    assert_eq!(
        value("busy-during"),
        "true",
        "nothing tells the reader the send is under way, which is why they \
         tap again. {context}"
    );
    assert_eq!(
        value("pending-after-send"),
        "",
        "the file stayed armed after it was sent, and the next message would \
         have carried it too. {context}"
    );
}
