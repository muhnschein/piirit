//! A picked video bigger than it needs to be is made smaller, and that
//! is what is sent (issue #111).
//!
//! The core recodes a picture on its way out and sends a video as it is,
//! so the conversation does it: it starts making a smaller file as soon as
//! the video is picked, says so on the bar while it does, and holds the
//! send until it is done -- nothing goes out until the reader sends, with
//! whatever caption they wrote meanwhile. The bar then weighs what was
//! made, and the core is handed it under the picked file's name. The ✕ on
//! the bar drops the file and stops the work.
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
        function model() { return findIn(loader.item, 'messages') }
        function sendFile(path) { model().send_file('', path); return 'ok' }
        function stopPreparing() { model().cancel_preparing(); return 'ok' }
        function quality(value) { model().media_quality = value; return 'ok' }
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
fn a_picked_video_is_made_smaller_and_waits_for_the_reader() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-video-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // Three seconds at 4 Mbit/s, about 1.5 MB; planned to fit under the
    // ceiling below, which is between the two.
    let clip = temp.join("lake at dusk.mov");
    piirit_video::synth(&clip, (640, 360), 3, 4_000_000, 90, true).expect("make a clip");
    let clip_bytes = std::fs::metadata(&clip).expect("measure the clip").len();
    // Fifteen seconds, which even at the least balanced rate comes out
    // past the ceiling.
    let long = temp.join("long walk.mov");
    piirit_video::synth(&long, (640, 360), 15, 4_000_000, 0, true).expect("make a long clip");
    let long = long.to_string_lossy().into_owned();
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
    let direct = clip_path.clone();
    // A second video, so the stopped one's "as it is" does not carry
    // over to it.
    let other = temp.join("pond at noon.mov");
    std::fs::copy(&clip, &other).expect("copy the clip");
    let switched = other.to_string_lossy().into_owned();
    let journal_mid = journal.clone();
    let recoded_dir = cache.join("piirit/piirit/recoded");
    let recoded_mid = recoded_dir.clone();
    let sends_in = |journal: &std::path::Path| {
        common::calls(journal)
            .into_iter()
            .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
            .count()
    };

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
        // At once: picking it started making it smaller.
        probe!("preparing", "messages", "preparing");
        probe!("label-preparing", "pendingAttachmentLabel", "text");
        probe!("large", "largeFileBar", "visible");
        probe!("planned-bytes", "messages", "attachment_bytes");
        probe!("sizes-preparing", "attachmentSizesLabel", "text");
        probe!("send-enabled-preparing", "sendButton", "enabled");
        // The caption is written meanwhile, and a send asked for now --
        // the keyboard's, say -- does nothing.
        (*steps_ptr).push(("type", call!("type", QString::from("ice on the lake"))));
        (*steps_ptr).push(("send-early", call!("send")));
    });

    // Made by now: a few seconds of small video is quick even unoptimised.
    single_shot(Duration::from_secs(12), move || unsafe {
        probe!("preparing-done", "messages", "preparing");
        probe!("send-enabled-done", "sendButton", "enabled");
        probe!("bar-done", "attachmentBar", "visible");
        probe!("made-bytes", "messages", "attachment_bytes");
        probe!("original-bytes", "messages", "original_bytes");
        probe!("sizes-done", "attachmentSizesLabel", "text");
        (*steps_ptr).push(("sends-before-tap", sends_in(&journal_mid).to_string()));
        let made: Vec<u64> = std::fs::read_dir(&recoded_mid)
            .map(|dir| {
                dir.flatten()
                    .filter_map(|entry| entry.metadata().ok().map(|meta| meta.len()))
                    .filter(|bytes| *bytes > 0)
                    .collect()
            })
            .unwrap_or_default();
        (*steps_ptr).push(("made-on-disk", format!("{made:?}")));
        // Only now does the reader send it.
        (*steps_ptr).push(("send", call!("send")));
    });

    single_shot(Duration::from_secs(18), move || unsafe {
        probe!("bar-after", "attachmentBar", "visible");
        // Again, and dropped straight away with the bar's button.
        (*steps_ptr).push((
            "attach-again",
            call!("attach", QString::from(again.as_str())),
        ));
        probe!("preparing-again", "messages", "preparing");
        // Stopped straight away with the bar's button: the file stays, to
        // go as it is, and a new quality does not start it again.
        (*steps_ptr).push((
            "stop-bar",
            call!("click", QString::from("cancelAttachmentButton")),
        ));
        probe!("preparing-stopped-bar", "messages", "preparing");
        probe!("sending-stopped-bar", "messages", "sending");
        probe!("bar-kept", "attachmentBar", "visible");
        probe!("label-kept", "pendingAttachmentLabel", "text");
        probe!("sizes-kept", "attachmentSizesLabel", "visible");
        probe!("large-kept", "largeFileLabel", "text");
        (*steps_ptr).push(("quality-kept", call!("quality", 1)));
        probe!("preparing-kept", "messages", "preparing");
        (*steps_ptr).push(("type-kept", call!("type", QString::from("as it was"))));
        (*steps_ptr).push(("send-kept", call!("send")));
    });

    single_shot(Duration::from_secs(24), move || unsafe {
        probe!("bar-sent-kept", "attachmentBar", "visible");
        // Handed a video that is not on the bar, the model makes it
        // smaller and sends nothing; stopped, it puts the work away.
        (*steps_ptr).push((
            "send-file",
            call!("sendFile", QString::from(direct.as_str())),
        ));
        probe!("preparing-direct", "messages", "preparing");
        (*steps_ptr).push(("stop", call!("stopPreparing")));
        probe!("preparing-stopped", "messages", "preparing");
        // Picked again, and the quality moved under it: what was being
        // made is put away and made again to the new plan.
        (*steps_ptr).push((
            "attach-switch",
            call!("attach", QString::from(switched.as_str())),
        ));
        (*steps_ptr).push(("quality", call!("quality", 0)));
        probe!("preparing-switched", "messages", "preparing");
    });

    single_shot(Duration::from_secs(26), move || unsafe {
        // Stopped, then dropped: the same button, twice.
        for step in ["stop-switched", "drop-switched"] {
            (*steps_ptr).push((
                step,
                call!("click", QString::from("cancelAttachmentButton")),
            ));
        }
        // Too long to fit even made smaller: the warning says so.
        (*steps_ptr).push(("attach-long", call!("attach", QString::from(long.as_str()))));
        probe!("preparing-long", "messages", "preparing");
        probe!("large-long", "largeFileLabel", "text");
        for step in ["stop-long", "drop-long"] {
            (*steps_ptr).push((
                step,
                call!("click", QString::from("cancelAttachmentButton")),
            ));
        }
    });

    single_shot(Duration::from_secs(28), move || unsafe {
        probe!("preparing-end", "messages", "preparing");
        probe!("bar-end", "attachmentBar", "visible");
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
        value("preparing"),
        "true",
        "picking the video did not start making it smaller. {context}"
    );
    assert_eq!(
        value("label-preparing"),
        "Making video smaller for sending: 0%",
        "the bar does not say the video is being made smaller. {context}"
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
    // A few megabytes, exact as a real.
    #[allow(clippy::cast_precision_loss)]
    let picked = std::fs::metadata(&clip).expect("measure the clip").len() as f64;
    // How much smaller it is planned to go, said beside the progress.
    assert_eq!(
        value("sizes-preparing"),
        format!("{} → ~{}", readable(picked), readable(planned)),
        "the bar does not say what the video weighs and is planned at. {context}"
    );
    assert_eq!(
        value("send-enabled-preparing"),
        "false",
        "the send button is on while the video is still being made smaller. {context}"
    );

    assert_eq!(
        value("preparing-done"),
        "false",
        "it never finished. {context}"
    );
    assert_eq!(
        value("sends-before-tap"),
        "0",
        "the video went out before the reader sent it. {context}"
    );
    assert_eq!(
        value("bar-done"),
        "true",
        "the bar let go of the video once it was made. {context}"
    );
    assert_eq!(
        value("send-enabled-done"),
        "true",
        "the send button stayed off once the video was ready. {context}"
    );
    let made_bytes: f64 = value("made-bytes").parse().unwrap_or(0.0);
    assert_eq!(
        value("made-on-disk"),
        format!("[{made_bytes}]"),
        "the bar does not weigh the video at the size it came out at. {context}"
    );
    assert_eq!(
        value("original-bytes"),
        picked.to_string(),
        "the bar does not weigh the video as it was picked. {context}"
    );
    assert!(
        made_bytes < picked,
        "the smaller video came out no smaller and was kept. {context}"
    );
    assert_eq!(
        value("sizes-done"),
        format!("{} → {}", readable(picked), readable(made_bytes)),
        "the bar does not say how much smaller the video went. {context}"
    );

    let sends: Vec<(String, serde_json::Value)> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
        .collect();
    assert_eq!(
        sends.len(),
        2,
        "one send of the smaller video, one of the stopped one as it was, \
         and none for the rest. {context}. Sends: {sends:?}"
    );
    let params = &sends[0].1;
    assert_eq!(
        params.get(2).and_then(serde_json::Value::as_str),
        Some("ice on the lake"),
        "the caption written while the video was made is not what was sent. Sends: {sends:?}"
    );
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
        value("bar-after"),
        "false",
        "the bar still holds the video after it was sent. {context}"
    );

    assert_eq!(
        value("preparing-again"),
        "true",
        "picking it again did not start. {context}"
    );
    assert_eq!(
        value("preparing-stopped-bar"),
        "false",
        "the bar's button did not stop it. {context}"
    );
    assert_eq!(
        value("sending-stopped-bar"),
        "false",
        "a stopped video still holds the send button. {context}"
    );
    assert_eq!(
        (value("bar-kept").as_str(), value("label-kept").as_str()),
        ("true", "Sending lake at dusk.mov"),
        "stopping it dropped the file the reader picked. {context}"
    );
    assert_eq!(
        value("sizes-kept"),
        "false",
        "a video going as it was still says it went smaller. {context}"
    );
    // Going as it was, it is warned about at the size it is.
    assert_eq!(
        value("large-kept"),
        format!(
            "At {}, this file is bigger than the 1.0 MB most relays accept. \
             Sending may fail.",
            readable(picked)
        ),
        "a video going as it was is not warned about at its own size. {context}"
    );
    assert_eq!(
        value("preparing-kept"),
        "false",
        "a new quality started a stopped video again. {context}"
    );
    let kept = &sends[1].1;
    assert_eq!(
        (
            kept.get(2).and_then(serde_json::Value::as_str),
            kept.get(3).and_then(serde_json::Value::as_str),
        ),
        (Some("as it was"), Some(clip.to_string_lossy().as_ref())),
        "a stopped video did not go as it was picked. Sends: {sends:?}"
    );
    assert_eq!(
        value("bar-sent-kept"),
        "false",
        "the bar still holds the video after it was sent. {context}"
    );
    assert_eq!(
        value("preparing-direct"),
        "true",
        "a video handed to the model directly was not made smaller. {context}"
    );
    assert_eq!(
        value("preparing-stopped"),
        "false",
        "stopping did not stop it. {context}"
    );
    assert_eq!(
        value("preparing-switched"),
        "true",
        "a new quality did not start the video again. {context}"
    );
    assert_eq!(
        value("preparing-long"),
        "true",
        "the long video was not being made smaller. {context}"
    );
    assert_eq!(
        value("large-long"),
        "Even made smaller, this file is bigger than the 1.0 MB most relays \
         accept. Sending may fail.",
        "a video too big even made smaller is not warned about as such. {context}"
    );
    assert_eq!(
        (value("preparing-end").as_str(), value("bar-end").as_str()),
        ("false", "false"),
        "the button, tapped twice, did not stop and then drop it. {context}"
    );

    let left = std::fs::read_dir(&recoded_dir)
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
    assert_eq!(left, 0, "a dropped or sent video left its file behind");

    let _ = std::fs::remove_dir_all(&temp);
}

/// `Format.readableSize`, which the bar's size line is written in.
fn readable(bytes: f64) -> String {
    let units = ["B", "kB", "MB", "GB"];
    let mut size = bytes;
    let mut step = 0;
    while size >= 1000.0 && step < units.len() - 1 {
        size /= 1000.0;
        step += 1;
    }
    if step == 0 {
        format!("{} {}", size.round(), units[step])
    } else {
        format!("{size:.1} {}", units[step])
    }
}
