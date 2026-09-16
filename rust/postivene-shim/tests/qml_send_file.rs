//! The attach flow on the page itself: what a picker hands back becomes a
//! bar above the field, a send that carries the file, and a bar that clears
//! itself afterwards.
//!
//! The page is loaded from a copy of the QML tree with `EnterKey` taken
//! out; see `common::qml_tree_without_enter_key` for why the shipped file
//! cannot be loaded headlessly as it stands. The pickers themselves are not
//! driven -- they are `pageStack.push`ed, and there is no page stack here --
//! but `attach()` is the function their handlers call, so everything after
//! the choosing is covered.

// Qt harness: see qml_conversation_open.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
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

/// The last second at which the send is still given a chance to have
/// cleared the pending file. Reached only by a page that never clears it,
/// which is the regression this test exists for; nextest warns at 30s.
const SETTLE_BY: u64 = 12;

/// Record the page's pending file once it clears, and stop the engine.
///
/// Asked for once a second from the sixth, rather than read once at a fixed
/// moment. The send is answered only after `POSTIVENE_FAKE_SEND_DELAY_MS`,
/// which this test makes deliberately wide so the reads before it land while
/// the send is still outstanding -- and reading the result two seconds later
/// left a margin of half a second, which a runner compiling the rest of the
/// suite alongside eats. The file was still armed, and the test reported a
/// regression that was not there.
///
/// A page that clears promptly still finishes at six seconds; one that never
/// clears is recorded as it stands at `SETTLE_BY`, and fails the assertion
/// with the same message and the same step trail as before.
///
/// Whole seconds: qmetaobject 0.2.10 truncates the sub-second part of a
/// Duration to zero, so a finer poll would spin (rust/clippy.toml).
unsafe fn settle_pending(engine: *mut QmlEngine, steps: *mut Vec<(&'static str, String)>) {
    for second in 6..=SETTLE_BY {
        single_shot(Duration::from_secs(second), move || unsafe {
            let done = (*steps)
                .iter()
                .any(|(label, _)| *label == "pending-after-send");
            if done {
                return;
            }
            let answer = (*engine).invoke_method("pending".into(), &[]);
            let pending = QString::from_qvariant(answer)
                .map(|value| value.to_string())
                .unwrap_or_default();
            if pending.is_empty() || second == SETTLE_BY {
                (*steps).push(("pending-after-send", pending));
                (*engine).quit();
            }
        });
    }
}

#[test]
fn a_picked_file_shows_above_the_field_and_leaves_with_the_message() {
    let temp = std::env::temp_dir().join(format!("postivene-qml-send-file-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        // Wide enough that the reads below land while the send is still
        // outstanding, which is the state the button has to refuse in.
        std::env::set_var("POSTIVENE_FAKE_SEND_DELAY_MS", "1500");
    }

    postivene_shim::register_qml_types();

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

    single_shot(Duration::from_secs(1), move || unsafe {
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

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push((
            "attach",
            call!(
                "attach",
                QString::from("/tmp/postivene-fake/holiday photo.png")
            ),
        ));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        probe!("bar-after", "attachmentBar", "visible");
        probe!("bar-text", "pendingAttachmentLabel", "text");
        // A file with no caption is still a message.
        probe!("send-enabled-after", "sendButton", "enabled");
        probe!("placeholder", "messageField", "placeholderText");
        (*steps_ptr).push(("send", call!("send")));
        // Read in the same callback, so the core cannot have answered yet.
        probe!("send-enabled-during", "sendButton", "enabled");
        probe!("busy-during", "sendBusy", "running");
    });

    unsafe { settle_pending(engine_ptr, steps_ptr) };

    engine.exec();

    assert_tray(&steps);
    assert_outcome(&steps, &journal);
}

/// What a run has to show for itself, read back by label.
fn value_of<'a>(steps: &'a [(&str, String)], label: &str) -> &'a str {
    steps
        .iter()
        .find(|(name, _)| *name == label)
        .map_or("", |(_, value)| value.as_str())
}

/// The tray: closed until the plus is tapped, closed again by a tap
/// anywhere beside it, and open again on the next plus.
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

/// Everything the run has to show for itself, out of the test body: what a
/// Qt harness can do in one function is bounded, and the assertions are the
/// part worth reading.
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

    // A picture goes through `send_msg` named `Image`, which is what puts
    // it through the core's own recoding; see `media.rs`.
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
                    "file": "/tmp/postivene-fake/holiday photo.png",
                    "filename": "holiday photo.png",
                    "viewtype": "Image",
                    "quotedMessageId": null,
                }
            ])
        )),
        "the page's send did not carry the picked file. {context}. Sends: {sends:?}"
    );

    // Cleared by `onSent`, with the model's own answer -- not optimistically
    // on the way out, which would drop the file on a send that failed.
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
