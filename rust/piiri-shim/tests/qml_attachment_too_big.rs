//! A file the profile's relay will not take.
//!
//! The conversation asks the core what this profile's relay recommends as
//! the largest attachment (`sys.msgsize_max_recommended`, see `media.rs`)
//! and holds the picked file to it: a bar above the field says so while
//! the file is sitting there, and the send button is off until it is
//! taken away. A picture is never refused, whatever it weighs, because
//! the core recodes one on its way out.
//!
//! The fake core is told a ceiling of a kilobyte
//! (`PIIRI_FAKE_MSGSIZE_MAX`), so the files here are small enough to
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

use piiri_shim::DeltaChatCore;
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
fn a_file_the_relay_will_not_take_is_said_so_and_cannot_be_sent() {
    let temp = std::env::temp_dir().join(format!("piiri-qml-too-big-{}", std::process::id()));
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
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRI_FAKE_MSGSIZE_MAX", "1024");
    }

    piiri_shim::register_qml_types();

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
        probe!("bar-before", "tooBigBar", "visible");
        (*steps_ptr).push((
            "attach-video",
            call!("attach", QString::from(video_path.as_str())),
        ));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        probe!("bar-after", "tooBigBar", "visible");
        probe!("bar-text", "tooBigLabel", "text");
        probe!("send-enabled", "sendButton", "enabled");
        // A caption does not make it sendable either: the caption belongs
        // to the file, and sending it alone would drop the file silently.
        (*steps_ptr).push(("type", call!("type", QString::from("look at this"))));
        probe!("send-enabled-with-caption", "sendButton", "enabled");
        (*steps_ptr).push(("send-anyway", call!("send")));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        probe!("still-pending", "pendingAttachmentLabel", "text");
        (*steps_ptr).push((
            "attach-picture",
            call!("attach", QString::from(picture_path.as_str())),
        ));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        probe!("bar-for-picture", "tooBigBar", "visible");
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
        "the page says a file is too big before one has been picked. {context}"
    );
    assert_eq!(
        value("bar-after"),
        "true",
        "nothing says the picked file is bigger than the relay takes, so \
         the reader finds out from a send that fails. {context}"
    );
    assert!(
        value("bar-text").contains("4.1 kB") && value("bar-text").contains("1.0 kB"),
        "the notice does not say how big the file is and how big the relay \
         takes: {:?}. {context}",
        value("bar-text")
    );
    assert_eq!(
        value("send-enabled"),
        "false",
        "the send button is live over a file the relay will not take. {context}"
    );
    assert_eq!(
        value("send-enabled-with-caption"),
        "false",
        "a caption made the message sendable, which would send the words \
         and drop the file. {context}"
    );
    assert!(
        !value("still-pending").is_empty(),
        "the file left the bar although nothing was sent. {context}"
    );

    let sends = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .count();
    assert_eq!(sends, 0, "the send reached the core anyway. {context}");

    // The core recodes a picture before it sends it, so what it weighs on
    // the phone says nothing about what leaves.
    assert_eq!(
        value("bar-for-picture"),
        "false",
        "a picture was refused for its size, which the core is about to \
         change. {context}"
    );
    assert_eq!(
        value("send-enabled-for-picture"),
        "true",
        "a picture cannot be sent although the core would shrink it. {context}"
    );
}
