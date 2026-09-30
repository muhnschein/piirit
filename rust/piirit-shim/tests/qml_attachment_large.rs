//! A file bigger than the core recommends is said to be large, and sent.
//!
//! The conversation asks the core for the largest attachment it
//! recommends (`sys.msgsize_max_recommended`, see `media.rs`). That is one
//! constant for every relay, and many relays take more -- issue #101 is a
//! relay taking 200 MB that Piirit would not hand a 25 MB file to. So a
//! file past it gets a notice above the field, and the send button stays
//! on and sends it. A picture gets no notice, whatever it weighs, because
//! the core recodes one on its way out.
//!
//! The fake core is told a ceiling of a kilobyte
//! (`PIIRIT_FAKE_MSGSIZE_MAX`), so the files here are small enough to
//! write in a test rather than the twenty-odd megabytes the real
//! recommendation is.

// Qt harness: see qml_send_file.rs.
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
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
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
        function attach(path) { loader.item.attach(path); return 'ok' }
        function send() { loader.item.sendCurrentText(); return 'ok' }
        function type(text) {
            var field = findIn(loader.item, 'messageField')
            if (!field) { return 'missing:messageField' }
            field.text = text
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_file_over_the_recommended_size_is_said_to_be_large_and_is_sent() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-large-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // Both past the ceiling below. One of them is a picture, which the
    // core shrinks on the way out, so its size on the phone decides
    // nothing.
    let video = temp.join("holiday clip.mp4");
    let picture = temp.join("holiday photo.jpg");
    std::fs::write(&video, vec![0_u8; 4096]).expect("write the video");
    std::fs::write(&picture, vec![0_u8; 4096]).expect("write the picture");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_MSGSIZE_MAX", "1024");
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

    macro_rules! probe {
        ($label:expr, $name:expr, $property:expr) => {
            (*steps_ptr).push((
                $label,
                call!("get", QString::from($name), QString::from($property)),
            ))
        };
    }

    let video_path = video.to_string_lossy().into_owned();
    let video_file = video_path.clone();
    let picture_path = picture.to_string_lossy().into_owned();

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
        (*steps_ptr).push(("settle", call!("settle")));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        probe!("bar-before", "largeFileBar", "visible");
        (*steps_ptr).push((
            "attach-video",
            call!("attach", QString::from(video_path.as_str())),
        ));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        probe!("bar-after", "largeFileBar", "visible");
        probe!("bar-text", "largeFileLabel", "text");
        probe!("send-enabled", "sendButton", "enabled");
        (*steps_ptr).push(("type", call!("type", QString::from("look at this"))));
        probe!("send-enabled-with-caption", "sendButton", "enabled");
        (*steps_ptr).push(("send", call!("send")));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        // Sent, so the bar and the notice with it are gone.
        probe!("bar-after-send", "largeFileBar", "visible");
        (*steps_ptr).push((
            "attach-picture",
            call!("attach", QString::from(picture_path.as_str())),
        ));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        probe!("bar-for-picture", "largeFileBar", "visible");
        probe!("send-enabled-for-picture", "sendButton", "enabled");
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
        "the conversation did not load. {context}"
    );
    assert_eq!(
        value("bar-before"),
        "false",
        "the page says a file is large before one has been picked. {context}"
    );
    assert_eq!(
        value("bar-after"),
        "true",
        "nothing says the picked file is bigger than some relays take, so \
         the reader finds out only from a send that fails. {context}"
    );
    assert!(
        value("bar-text").contains("4.1 kB") && value("bar-text").contains("1.0 kB"),
        "the notice does not say how big the file is and how big the core \
         recommends: {:?}. {context}",
        value("bar-text")
    );
    // Issue #101: the recommendation is the core's, the same for every
    // relay, and a relay that takes more is common. It is a warning.
    assert_eq!(
        value("send-enabled"),
        "true",
        "the send button is off over a file bigger than the core \
         recommends, so a relay that takes it is never handed it. {context}"
    );
    assert_eq!(
        value("send-enabled-with-caption"),
        "true",
        "a caption turned the send button off. {context}"
    );

    let sends: Vec<(String, serde_json::Value)> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .collect();
    let sent_file = sends
        .first()
        .filter(|(method, _)| method == "misc_send_msg")
        .and_then(|(_, params)| params.get(3))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert_eq!(
        sent_file, video_file,
        "the large file did not reach the core. {context}. Sends: {sends:?}"
    );
    assert_eq!(
        sends
            .first()
            .and_then(|(_, params)| params.get(2))
            .and_then(serde_json::Value::as_str),
        Some("look at this"),
        "the caption did not go with the file. {context}. Sends: {sends:?}"
    );
    assert_eq!(
        value("bar-after-send"),
        "false",
        "the notice outlived the file it is about. {context}"
    );

    // The core recodes a picture before it sends it, so what it weighs on
    // the phone says nothing about what leaves.
    assert_eq!(
        value("bar-for-picture"),
        "false",
        "a picture was warned about for its size, which the core is about to \
         change. {context}"
    );
    assert_eq!(
        value("send-enabled-for-picture"),
        "true",
        "a picture cannot be sent although the core would shrink it. {context}"
    );
}
