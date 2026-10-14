//! A chat opened by a camera quick action set to send at once sends what
//! was taken without waiting for the send button (`sendShared`).
//!
//! Not before the core has said how large an attachment it recommends,
//! and not while a video is still being made smaller to fit that: a
//! video sent earlier would go as it was taken, however large. The file
//! goes alone -- whatever is in the field is the reader's, not a caption
//! they chose for it -- and the field keeps it. Once sent, the capture is
//! thrown away, as one sent from the bar is. Opened without being told to
//! send at once, the chat waits for the reader as a share does, and so it
//! does for a file the reader puts on the bar in place of the one taken,
//! for one a reply is begun under before it goes, and for a video whose
//! making smaller the reader stops.
//!
//! The video is made here, as the camera would leave one, by
//! `piirit_video::synth`, and the fake core is told a ceiling it comes
//! near, so it is made smaller first.

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
        function load(url, properties) {
            loader.setSource('', {})
            var given = JSON.parse(properties)
            given.status = PageStatus.Active
            loader.setSource(url, given)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
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
        // A reply begun to a message, as its menu begins one.
        function reply() {
            findIn(loader.item, 'messages').quoted_message_id = 1
            loader.item.replyBody = 'the message replied to'
            return 'ok'
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
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
fn a_capture_set_to_send_at_once_goes_once_it_is_ready() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-at-once-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();
    let cache = temp.join("cache");
    let captures = cache.join("piirit/piirit/captures");
    std::fs::create_dir_all(&captures).expect("create the captures directory");

    // Two pictures and a video, where the camera leaves them.
    let photo = captures.join("photo-20261012-101010.png");
    std::fs::copy(common::a_real_picture(), &photo).expect("copy a picture");
    let waiting = captures.join("photo-20261012-101011.png");
    std::fs::copy(common::a_real_picture(), &waiting).expect("copy a picture");
    // A picture set to send at once that the reader swaps for another
    // before it has gone, and the one they swap it for.
    let swapped = captures.join("photo-20261012-101013.png");
    std::fs::copy(common::a_real_picture(), &swapped).expect("copy a picture");
    let instead = temp.join("from the gallery.png");
    std::fs::copy(common::a_real_picture(), &instead).expect("copy a picture");
    // A picture a reply is begun under before it can go.
    let replied = captures.join("photo-20261012-101014.png");
    std::fs::copy(common::a_real_picture(), &replied).expect("copy a picture");
    // Ten seconds of video, long enough to be stopped while it is being
    // made smaller.
    let stopped = captures.join("video-20261012-101015.mp4");
    piirit_video::synth(&stopped, (640, 360), 10, 4_000_000, 0, true).expect("make a long clip");
    // Three seconds at 4 Mbit/s, about 1.5 MB: past the ceiling below
    // until it is made smaller.
    let video = captures.join("video-20261012-101012.mp4");
    piirit_video::synth(&video, (640, 360), 3, 4_000_000, 90, true).expect("make a clip");

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
    macro_rules! open {
        ($label:expr, $url:expr, $file:expr, $now:expr) => {
            (*steps_ptr).push((
                $label,
                call!(
                    "load",
                    QString::from($url.as_str()),
                    QString::from(
                        serde_json::json!({
                            "accountId": 1,
                            "chatId": 1,
                            "sharedFile": $file,
                            "sendShared": $now,
                        })
                        .to_string()
                    )
                ),
            ))
        };
    }

    let sends_in = |journal: &std::path::Path| {
        common::calls(journal)
            .into_iter()
            .filter(|(method, _)| method == "misc_send_msg" || method == "send_msg")
            .collect::<Vec<_>>()
    };
    let page = common::page_url_in(&tree, "ConversationPage.qml");
    let photo_path = photo.to_string_lossy().into_owned();
    let waiting_path = waiting.to_string_lossy().into_owned();
    let video_path = video.to_string_lossy().into_owned();
    let (journal_b, journal_c) = (journal.clone(), journal.clone());

    // A picture, set to send at once, with the reader's own words for
    // this chat already in the field.
    let page_a = page.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        open!("photo-load", page_a, photo_path, true);
        (*steps_ptr).push(("photo-type", call!("type", QString::from("half a thought"))));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("photo-sends", sends_in(&journal_b).len().to_string()));
        probe!("photo-bar", "attachmentBar", "visible");
        probe!("photo-field", "messageField", "text");
    });

    // A picture, not set to send at once: it waits for the reader.
    let page_b = page.clone();
    single_shot(Duration::from_secs(4), move || unsafe {
        open!("waiting-load", page_b, waiting_path, false);
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("waiting-sends", sends_in(&journal_c).len().to_string()));
        probe!("waiting-bar", "attachmentBar", "visible");
        probe!("waiting-send-enabled", "sendButton", "enabled");
    });

    // Swapped for another file before it could go: only what was taken
    // was to go on its own, and the other waits for the reader.
    let page_d = page.clone();
    let swapped_path = swapped.to_string_lossy().into_owned();
    let instead_path = instead.to_string_lossy().into_owned();
    let journal_e = journal.clone();
    single_shot(Duration::from_secs(7), move || unsafe {
        open!("swapped-load", page_d, swapped_path, true);
        (*steps_ptr).push((
            "swapped-attach",
            call!("attach", QString::from(instead_path.as_str())),
        ));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        (*steps_ptr).push(("swapped-sends", sends_in(&journal_e).len().to_string()));
        probe!("swapped-bar", "attachmentBar", "visible");
    });

    // A reply begun before it could go: the reader is writing, and the
    // picture waits for them rather than going as their reply.
    let page_e = page.clone();
    let replied_path = replied.to_string_lossy().into_owned();
    let journal_f = journal.clone();
    single_shot(Duration::from_secs(10), move || unsafe {
        open!("replied-load", page_e, replied_path, true);
        (*steps_ptr).push(("replied-reply", call!("reply")));
    });

    single_shot(Duration::from_secs(12), move || unsafe {
        (*steps_ptr).push(("replied-sends", sends_in(&journal_f).len().to_string()));
        probe!("replied-bar", "attachmentBar", "visible");
    });

    // A video stopped while it is made smaller goes as it is -- when the
    // reader sends it, not on its own. Stopped once it is being made
    // smaller, asked four times a second for up to three seconds: a
    // machine measuring coverage takes its time to start.
    let page_f = page.clone();
    let stopped_path = stopped.to_string_lossy().into_owned();
    single_shot(Duration::from_secs(13), move || unsafe {
        open!("stopped-load", page_f, stopped_path, true);
    });

    let stopped_done = std::rc::Rc::new(std::cell::Cell::new(false));
    for quarter in 1..=12 {
        let stopped_done = stopped_done.clone();
        single_shot(
            Duration::from_millis(13_000 + quarter * 250),
            move || unsafe {
                if stopped_done.get() {
                    return;
                }
                let preparing = call!("get", QString::from("messages"), QString::from("preparing"));
                if preparing != "true" && quarter < 12 {
                    return;
                }
                stopped_done.set(true);
                (*steps_ptr).push(("stopped-preparing", preparing));
                (*steps_ptr).push((
                    "stopped-stop",
                    call!("click", QString::from("cancelAttachmentButton")),
                ));
            },
        );
    }

    let journal_g = journal.clone();
    single_shot(Duration::from_secs(18), move || unsafe {
        (*steps_ptr).push(("stopped-sends", sends_in(&journal_g).len().to_string()));
        probe!("stopped-bar", "attachmentBar", "visible");
    });

    // A video, set to send at once: made smaller first, then sent. What
    // the core is handed says which: how long the making smaller takes
    // is the machine's.
    let page_c = page;
    single_shot(Duration::from_secs(19), move || unsafe {
        open!("video-load", page_c, video_path, true);
    });

    // Made, and sent: asked once a second until it has gone, or for as
    // long as a slow machine could need to make three seconds of video,
    // and well inside the two minutes nextest gives a test.
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    for second in 20..=75 {
        let (done, journal) = (done.clone(), journal.clone());
        single_shot(Duration::from_secs(second), move || unsafe {
            if done.get() || (sends_in(&journal).len() < 2 && second < 75) {
                return;
            }
            done.set(true);
            probe!("video-preparing-done", "messages", "preparing");
            probe!("video-bar", "attachmentBar", "visible");
            (*engine_ptr).quit();
        });
    }

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");
    let sends = sends_in(&journal);
    let sends_context = format!("{context}. Sends: {sends:?}");

    for step in [
        "photo-load",
        "waiting-load",
        "swapped-load",
        "replied-load",
        "stopped-load",
        "video-load",
    ] {
        assert_eq!(
            value(step),
            "ok",
            "the conversation did not load. {context}"
        );
    }
    assert_eq!(value("photo-type"), "ok", "{context}");
    assert_eq!(
        value("photo-sends"),
        "1",
        "a picture set to send at once was not sent on its own. {sends_context}"
    );
    // A picture goes as a message of its own, `send_msg`'s third
    // parameter, so the core recodes it.
    let message = &sends[0].1[2];
    assert_eq!(
        message.get("file").and_then(serde_json::Value::as_str),
        Some(photo.to_string_lossy().as_ref()),
        "what was sent is not the picture taken. {sends_context}"
    );
    assert!(
        !matches!(message.get("text"), Some(text) if !text.is_null()),
        "the words in the field went out as the picture's caption. {sends_context}"
    );
    assert_eq!(
        value("photo-field"),
        "half a thought",
        "sending the picture on its own emptied the reader's field. {context}"
    );
    assert_eq!(
        value("photo-bar"),
        "false",
        "the picture is still on the bar once sent. {context}"
    );
    assert!(
        !photo.exists(),
        "the picture sent was left in the captures directory"
    );

    assert_eq!(
        value("waiting-sends"),
        "1",
        "a picture not set to send at once was sent anyway. {sends_context}"
    );
    assert_eq!(
        (
            value("waiting-bar").as_str(),
            value("waiting-send-enabled").as_str()
        ),
        ("true", "true"),
        "a picture not set to send at once is not waiting on the bar for \
         the reader. {context}"
    );
    assert!(
        waiting.exists(),
        "a capture waiting to be sent was thrown away"
    );

    assert_eq!(value("swapped-attach"), "ok", "{context}");
    assert_eq!(
        (
            value("swapped-sends").as_str(),
            value("swapped-bar").as_str()
        ),
        ("1", "true"),
        "a file put on the bar in place of the one taken went on its own. \
         {sends_context}"
    );
    assert!(instead.exists(), "the reader's own picture was removed");

    assert_eq!(value("replied-reply"), "ok", "{context}");
    assert_eq!(
        (
            value("replied-sends").as_str(),
            value("replied-bar").as_str()
        ),
        ("1", "true"),
        "a picture went on its own under a reply the reader had begun. \
         {sends_context}"
    );
    assert_eq!(
        (
            value("stopped-preparing").as_str(),
            value("stopped-stop").as_str()
        ),
        ("true", "ok"),
        "the long video was not being made smaller when it was stopped. \
         {context}"
    );
    assert_eq!(
        (
            value("stopped-sends").as_str(),
            value("stopped-bar").as_str()
        ),
        ("1", "true"),
        "a video stopped while it was made smaller went on its own, as \
         large as it was taken. {sends_context}"
    );

    assert_eq!(
        sends.len(),
        2,
        "the video, once made smaller, was not sent on its own. {sends_context}"
    );
    let sent = sends[1]
        .1
        .get(3)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    assert!(
        sent.starts_with(&cache.to_string_lossy().into_owned())
            && !sent.starts_with(&captures.to_string_lossy().into_owned())
            && std::path::Path::new(sent).extension() == Some("mp4".as_ref()),
        "the core was not handed the smaller video, but {sent}. {sends_context}"
    );
    assert_eq!(
        (
            value("video-preparing-done").as_str(),
            value("video-bar").as_str()
        ),
        ("false", "false"),
        "the video is still on the bar after it was sent. {context}"
    );
    assert!(
        !video.exists(),
        "the video taken was left in the captures directory once sent"
    );
}
