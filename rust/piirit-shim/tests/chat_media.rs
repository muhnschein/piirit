//! Media pages' model: one kind per chat, newest first, from core's index.
//!
//! Gallery asks core for its three kinds in one call. Empty kind = loaded, not loading.
//! Voice sent while open lands on audio list without re-reading.

// Qt harness allows: unsafe `env::set_var`, borrow_as_ptr, whole-second single_shot.
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
use serde_json::Value;

mod common;

// Model answers, not clock: each step waits for last load.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piirit 1.0
    Item {
        property string log: ''
        property int step: 0
        ChatMedia {
            id: gallery
            account_id: 1
            chat_id: 2
            kind: 'gallery'
            onError: log += 'error:' + message + ';'
        }
        ChatMedia {
            id: sounds
            account_id: 1
            chat_id: 2
            kind: 'audio'
            onError: log += 'error:' + message + ';'
        }
        ChatMedia {
            id: nothing
            account_id: 1
            chat_id: 1
            kind: 'apps'
            onError: log += 'error:' + message + ';'
        }
        ChatMessages {
            id: chat
            account_id: 1
            chat_id: 2
        }
        Connections {
            target: core
            onStatus_changed: {
                if (core.status === 'ready') {
                    gallery.reload()
                    sounds.reload()
                    nothing.reload()
                    chat.reload()
                }
            }
            onCore_event: {
                gallery.handle_event(context_id, kind, payload_json)
                sounds.handle_event(context_id, kind, payload_json)
                nothing.handle_event(context_id, kind, payload_json)
                chat.handle_event(context_id, kind, payload_json)
            }
        }
        Repeater {
            id: pictures
            model: gallery.rows
            Item {
                property int mid: model.message_id
                property bool filled: model.loaded
                property string kind: model.view_type
                property string file: model.file_path
            }
        }
        Repeater {
            id: recordings
            model: sounds.rows
            Item {
                property int mid: model.message_id
                property bool filled: model.loaded
                property string kind: model.view_type
                property string file: model.file_path
            }
        }
        function rowsOf(repeater) {
            var out = ''
            for (var i = 0; i < repeater.count; i++) {
                var row = repeater.itemAt(i)
                out += row.mid + ':' + row.kind + ':' + (row.filled ? row.file : '?') + ','
            }
            return out
        }
        function settled(model) { return model.loaded && !model.loading }
        Timer {
            interval: 50
            repeat: true
            running: true
            onTriggered: {
                if (step === 0 && settled(gallery) && settled(sounds)
                        && settled(nothing) && chat.loaded) {
                    step = 1
                    log += 'gallery:' + rowsOf(pictures) + ';'
                    log += 'audio-before:' + sounds.count + ';'
                    log += 'nothing:' + nothing.count + '/' + nothing.loaded + ';'
                    // A voice message, sent while the page is open.
                    chat.send_voice('/tmp/piirit-fake/reply.aac')
                } else if (step === 1 && sounds.count === 2 && settled(sounds)) {
                    step = 2
                    log += 'audio-after:' + rowsOf(recordings) + ';'
                    log += 'gallery-after:' + rowsOf(pictures) + ';'
                }
            }
        }
        function report() { return step + '#' + log }
    }
";

#[test]
fn a_chats_media_is_read_by_kind_newest_first() {
    let temp = std::env::temp_dir().join(format!("piirit-chat-media-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_MEDIA", "1");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut report = String::new();
    let report_ptr: *mut String = std::ptr::addr_of_mut!(report);

    // SAFETY for every block: fire only inside `exec()`; `engine` outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // Backstop: reads reached steps, quits.
    single_shot(Duration::from_secs(8), move || unsafe {
        let value = (*engine_ptr).invoke_method("report".into(), &[]);
        *report_ptr = QString::from_qvariant(value)
            .map(|text| text.to_string())
            .unwrap_or_default();
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_media(&common::calls(&journal), &report);
}

/// Each kind asked for in one core call; rows newest first; arrivals reach their list.
fn assert_media(calls: &[(String, Value)], report: &str) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("report: {report}\ncalls: {names:?}");

    let (step, log) = report.split_once('#').unwrap_or_default();
    assert_eq!(step, "2", "the scenario did not run to the end. {context}");
    assert!(
        !log.contains("error:"),
        "a model reported a failure. {context}"
    );

    // Core index: gallery and audio kinds, one named, two optional.
    let asked: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "get_chat_media")
        .map(|(_, params)| params.clone())
        .collect();
    assert!(
        asked.contains(&serde_json::json!([1, 2, "Image", "Gif", "Video"])),
        "the gallery did not ask for pictures, GIFs and videos of the chat in one call. {context}"
    );
    assert!(
        asked.contains(&serde_json::json!([1, 2, "Audio", "Voice", null])),
        "the audio page did not ask for music and voice messages of the chat. {context}"
    );
    assert!(
        asked.contains(&serde_json::json!([1, 1, "Webxdc", null, null])),
        "the apps page did not ask for the chat's apps. {context}"
    );

    // Newest first, whatever core's order: later video stands above picture.
    assert!(
        log.contains(
            "gallery:15:Video:/tmp/piirit-fake/clip.mp4,10:Image:/tmp/piirit-fake/photo.jpg,;"
        ),
        "the gallery does not hold the chat's picture and video, newest first. {context}"
    );
    // Once: arrival elsewhere rereads ids, not rows already here.
    assert_eq!(
        calls
            .iter()
            .filter(|(name, params)| name == "get_messages"
                && *params == serde_json::json!([1, [15, 10]]))
            .count(),
        1,
        "the gallery's rows were not read in one batch, newest first, or were read again \
         when a voice message arrived. {context}"
    );

    // Empty kind loaded: lets a page say so rather than wait.
    assert!(
        log.contains("nothing:0/true;"),
        "an empty kind did not report itself loaded and empty. {context}"
    );

    // Voice reached audio list only; gallery untouched.
    assert!(
        log.contains("audio-before:1;"),
        "the audio page does not hold the chat's voice message. {context}"
    );
    let after = log
        .split("audio-after:")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .unwrap_or_default();
    assert!(
        after.starts_with("101:Voice:") && after.ends_with(",13:Voice:/tmp/piirit-fake/voice.aac,"),
        "the voice message sent while the page was open did not land at the top of the audio list: {after:?}. {context}"
    );
    assert!(
        log.contains(
            "gallery-after:15:Video:/tmp/piirit-fake/clip.mp4,10:Image:/tmp/piirit-fake/photo.jpg,;"
        ),
        "a voice message arriving changed the gallery. {context}"
    );
    // Gallery rows not re-read: audio's second batch is the new message alone.
    assert!(
        calls.iter().any(
            |(name, params)| name == "get_messages" && *params == serde_json::json!([1, [101]])
        ),
        "the new message was not read on its own; what was already here was read again. {context}"
    );
}
