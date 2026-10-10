//! A picked video bigger than it needs to be is made smaller, and that
//! is what is sent (issue #111).
//!
//! The core recodes a picture on its way out and sends a video as it is,
//! so the conversation does it: it plans a smaller file as the video is
//! picked, says so on the bar while it makes it, and hands the core the
//! smaller file under the picked file's name. The ✕ on the bar stops it,
//! and then nothing is sent and the file stays.
//!
//! The clip is made here, as a phone's camera would leave one, by
//! `piirit_video::synth`. The fake core is told a ceiling between the
//! clip's size and the size it is planned at, so the bar says nothing
//! about a file that will fit once it is smaller.

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
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
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
fn a_picked_video_is_made_smaller_before_it_is_sent() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-video-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // Three seconds at 4 Mbit/s, about 1.5 MB; planned at 1.5 Mbit/s and
    // 64 kbit/s, about 590 kB. The ceiling is between the two.
    let clip = temp.join("lake at dusk.mov");
    piirit_video::synth(&clip, (640, 360), 3, 4_000_000, 90, true).expect("make a clip");
    let clip_bytes = std::fs::metadata(&clip).expect("measure the clip").len();
    assert!(
        clip_bytes > 1_000_000,
        "the clip came out at {clip_bytes} bytes"
    );
    let cache = temp.join("cache");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_MSGSIZE_MAX", "1000000");
        std::env::set_var("XDG_CACHE_HOME", &cache);
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

    let clip_path = clip.to_string_lossy().into_owned();
    let again = clip_path.clone();

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
        (*steps_ptr).push(("attach", call!("attach", QString::from(clip_path.as_str()))));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        probe!("large", "largeFileBar", "visible");
        probe!("planned-bytes", "messages", "attachment_bytes");
        (*steps_ptr).push(("send", call!("send")));
        // At once: the send has started making the video smaller.
        probe!("preparing", "messages", "preparing");
        probe!("label-preparing", "pendingAttachmentLabel", "text");
        probe!("send-enabled-preparing", "sendButton", "enabled");
    });

    // Sent by now: a few seconds of small video is quick even unoptimised.
    single_shot(Duration::from_secs(12), move || unsafe {
        probe!("preparing-after", "messages", "preparing");
        probe!("bar-after", "attachmentBar", "visible");
        // Again, and stopped straight away with the bar's button.
        (*steps_ptr).push((
            "attach-again",
            call!("attach", QString::from(again.as_str())),
        ));
        (*steps_ptr).push(("send-again", call!("send")));
        probe!("preparing-again", "messages", "preparing");
        (*steps_ptr).push((
            "stop",
            call!("click", QString::from("cancelAttachmentButton")),
        ));
    });

    single_shot(Duration::from_secs(16), move || unsafe {
        probe!("preparing-stopped", "messages", "preparing");
        probe!("sending-stopped", "messages", "sending");
        probe!("bar-stopped", "attachmentBar", "visible");
        probe!("label-stopped", "pendingAttachmentLabel", "text");
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

    // Measured as it will be sent, not as it is on the phone.
    assert_eq!(
        value("large"),
        "false",
        "the bar says a video is too large that will fit once it is made \
         smaller. {context}"
    );
    let planned: f64 = value("planned-bytes").parse().unwrap_or(0.0);
    assert!(
        planned > 0.0 && planned < 1_000_000.0,
        "the bar does not weigh the video at the size it is planned at. {context}"
    );

    assert_eq!(
        value("preparing"),
        "true",
        "the send did not start making the video smaller. {context}"
    );
    assert_eq!(
        value("label-preparing"),
        "Making video smaller for sending: 0%",
        "the bar does not say the video is being made smaller. {context}"
    );
    assert_eq!(
        value("send-enabled-preparing"),
        "false",
        "the send button is on while the video is made smaller, so a second \
         tap sends it twice. {context}"
    );
    assert_eq!(
        value("preparing-after"),
        "false",
        "it never finished. {context}"
    );
    assert_eq!(
        value("bar-after"),
        "false",
        "the bar still holds the video after it was sent. {context}"
    );

    let sends: Vec<(String, serde_json::Value)> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .collect();
    assert_eq!(
        sends.len(),
        1,
        "one send, and none for the stopped one. {context}. Sends: {sends:?}"
    );
    let params = &sends[0].1;
    let sent = params
        .get(3)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(
        sent.starts_with(&cache.to_string_lossy().into_owned())
            && std::path::Path::new(sent).extension() == Some("mp4".as_ref()),
        "the core was not handed the smaller video, but {sent}. {context}"
    );
    assert_eq!(
        params.get(4).and_then(serde_json::Value::as_str),
        Some("lake at dusk.mp4"),
        "the recipient does not see the file named as it was picked. Sends: {sends:?}"
    );
    assert!(
        !std::path::Path::new(sent).exists(),
        "the smaller video was left in the cache after the core took it"
    );
    assert!(clip.exists(), "the reader's own video was removed");

    assert_eq!(
        value("preparing-again"),
        "true",
        "the second send did not start. {context}"
    );
    assert_eq!(
        value("preparing-stopped"),
        "false",
        "the bar's button did not stop it. {context}"
    );
    assert_eq!(
        value("sending-stopped"),
        "false",
        "a stopped send still holds the send button. {context}"
    );
    assert_eq!(
        value("bar-stopped"),
        "true",
        "stopping it dropped the file the reader picked. {context}"
    );
    assert_eq!(
        value("label-stopped"),
        "Sending lake at dusk.mov",
        "the bar did not go back to the file. {context}"
    );
    let left = std::fs::read_dir(cache.join("piirit/piirit/recoded"))
        .map(|dir| {
            dir.flatten()
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|s| s.eq_ignore_ascii_case("mp4"))
                })
                .count()
        })
        .unwrap_or(0);
    assert_eq!(left, 0, "a stopped recoding left its file behind");

    let _ = std::fs::remove_dir_all(&temp);
}
