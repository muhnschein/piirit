//! A voice message in the chat gets its own transcript, for its profile and id, and draws it;
//! music never does, nor anything while transcription is off. Bubble's button and row menu ask
//! that transcript; menu offers "Copy transcript" only once there is one, else "Transcribe".
//!
//! `ConversationList.qml` vs stub Silica, with stand-ins for app's `Transcriber` and each
//! message's `Transcript` (shim's are stubs until store lands): drives every state.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 1600

        property string calls: ''
        property string raised: ''

        // App's Transcriber, as far as a bubble reads it.
        QtObject {
            id: transcriber
            property bool enabled: true
            property int model_state: 2
            property int model_permille: 0
            property bool power_saving: false
            property bool charging: false
            property int mode: 0
        }

        // One message's Transcript; test sets state.
        Component {
            id: fakeTranscript
            QtObject {
                property int account_id
                property int message_id
                property int state: 0
                property bool manual: false
                property int permille: 0
                property string text: ''
                property string language: ''
                property string error: ''
                function transcribe(anyLanguage) {
                    calls += 'transcribe:' + message_id + ':' + anyLanguage + ';'
                }
                function cancel() { calls += 'cancel:' + message_id + ';' }
            }
        }

        function row(id, viewType, file) {
            return {
                message_id: id, text: '', styled_text: '',
                is_outgoing: false, is_info: false, show_padlock: true,
                state: 16, timestamp: 1700000000 + id, day_number: 19675,
                sender_name: 'Ada', sender_color: '#00875a',
                is_forwarded: false, quote_text: '', quote_author: '',
                file_path: file, file_name: file.split('/').pop(),
                file_mime: 'audio/mpeg', file_bytes: 34000,
                view_type: viewType, image_width: 0, image_height: 0,
                is_new: false, has_html: false, is_edited: false,
                download_state: 'Done', vcard_name: '', vcard_addr: '',
                vcard_color: '', webxdc_name: '', webxdc_document: '',
                webxdc_summary: '', webxdc_icon: '', reactions: '',
                my_reaction: '', loaded: true, duration_ms: 9000
            }
        }

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 1600 }
        function load(url) {
            rows.append(row(21, 'Voice', '/tmp/piirit-voice/voice.mp3'))
            rows.append(row(22, 'Audio', '/tmp/piirit-voice/song.mp3'))
            list.setSource(url, {
                model: rows, accountId: 5, transcriber: transcriber,
                transcriptMaker: fakeTranscript
            })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.copyRequested.connect(function (body) { raised += 'copy:' + body + ';' })
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                findAll(kids[i], name, out)
            }
            return out
        }
        function rowsInOrder() {
            var found = findAll(list.item, 'messageRow', [])
            var seen = []
            for (var i = 0; i < found.length; i++) {
                if (seen.indexOf(found[i]) === -1) { seen.push(found[i]) }
            }
            seen.sort(function (a, b) { return a.y - b.y })
            return seen
        }
        // Row's transcript: whom it is for, or none.
        function transcriptOf(index) {
            var t = rowsInOrder()[index].transcript
            return t ? t.account_id + '/' + t.message_id : 'none'
        }
        function setState(index, json) {
            var t = rowsInOrder()[index].transcript
            if (!t) { return 'none' }
            var values = JSON.parse(json)
            for (var key in values) { t[key] = values[key] }
            return 'ok'
        }
        // What the bubble draws: status line, then button.
        function bubble(index) {
            var shown = findIn(rowsInOrder()[index], 'voiceTranscript')
            if (!shown || !shown.visible) { return 'none' }
            var status = findIn(shown, 'transcriptStatus')
            var button = findIn(shown, 'transcriptButton')
            return (status.visible ? status.text : '-') + '|'
                   + (button.visible ? button.text : '-')
        }
        function tapButton(index) {
            findIn(rowsInOrder()[index], 'transcriptButton').clicked()
            return 'ok'
        }
        function offered(index, name) {
            var item = findIn(rowsInOrder()[index].menu, name)
            return item ? '' + item.visible : 'missing:' + name
        }
        function pick(index, name) {
            var item = findIn(rowsInOrder()[index].menu, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function setEnabled(on) { transcriber.enabled = on; return 'ok' }
        function heard() { var was = calls + raised; calls = ''; raised = ''; return was }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_voice_message_draws_and_offers_its_own_transcript() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let steps: common::Steps = common::Steps::default();

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }
    macro_rules! menu {
        ($index:expr, $name:expr) => {
            call!("offered", $index, QString::from($name))
        };
    }

    let mut plan = common::Plan::default();

    let s = steps.clone();
    plan.after(1, move || unsafe {
        let url = QString::from(common::component_url("ConversationList.qml"));
        common::record(&s, "load", call!("load", url));
    });

    let s = steps.clone();
    plan.after(1, move || unsafe {
        common::record(&s, "voice-for", call!("transcriptOf", 0));
        common::record(&s, "music-for", call!("transcriptOf", 1));
        common::record(&s, "voice-bubble", call!("bubble", 0));
        common::record(&s, "music-bubble", call!("bubble", 1));
        common::record(&s, "none-copy", menu!(0, "copyTranscriptItem"));
        common::record(&s, "none-transcribe", menu!(0, "transcribeItem"));
        common::record(&s, "music-copy", menu!(1, "copyTranscriptItem"));
        common::record(&s, "music-transcribe", menu!(1, "transcribeItem"));
        common::record(&s, "bubble-tap", call!("tapButton", 0));
        common::record(&s, "bubble-asked", call!("heard"));
        common::record(
            &s,
            "menu-tap",
            call!("pick", 0, QString::from("transcribeItem")),
        );
        common::record(&s, "menu-asked", call!("heard"));

        // Queued by tap: neither offer.
        call!(
            "setState",
            0,
            QString::from(r#"{"state": 1, "manual": true}"#)
        );
        common::record(&s, "manual-copy", menu!(0, "copyTranscriptItem"));
        common::record(&s, "manual-transcribe", menu!(0, "transcribeItem"));
        // Running: neither.
        call!(
            "setState",
            0,
            QString::from(r#"{"state": 2, "manual": false, "text": "Hey, I'm"}"#)
        );
        common::record(&s, "running-copy", menu!(0, "copyTranscriptItem"));
        common::record(&s, "running-transcribe", menu!(0, "transcribeItem"));

        // Other language: bubble skips language check on its button.
        call!(
            "setState",
            0,
            QString::from(r#"{"state": 5, "language": "es"}"#)
        );
        common::record(&s, "other-bubble", call!("bubble", 0));
        call!("tapButton", 0);
        common::record(&s, "other-asked", call!("heard"));
        common::record(&s, "other-copy", menu!(0, "copyTranscriptItem"));

        // Failed: transcribe offered again, nothing to copy.
        call!("setState", 0, QString::from(r#"{"state": 6}"#));
        common::record(&s, "failed-copy", menu!(0, "copyTranscriptItem"));
        common::record(&s, "failed-transcribe", menu!(0, "transcribeItem"));

        // Done: copy offered, transcribe not; copy hands over the text.
        call!(
            "setState",
            0,
            QString::from(r#"{"state": 3, "text": "Running ten minutes late.", "language": "en"}"#)
        );
        common::record(&s, "done-copy", menu!(0, "copyTranscriptItem"));
        common::record(&s, "done-transcribe", menu!(0, "transcribeItem"));
        common::record(
            &s,
            "copy-tap",
            call!("pick", 0, QString::from("copyTranscriptItem")),
        );
        common::record(&s, "copied", call!("heard"));

        // Off: transcript gone, nothing offered.
        call!("setEnabled", false);
        common::record(&s, "off-for", call!("transcriptOf", 0));
        common::record(&s, "off-bubble", call!("bubble", 0));
        common::record(&s, "off-copy", menu!(0, "copyTranscriptItem"));
        common::record(&s, "off-transcribe", menu!(0, "transcribeItem"));
        // On again: made afresh.
        call!("setEnabled", true);
        common::record(&s, "on-for", call!("transcriptOf", 0));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(
        value("voice-for"),
        "5/21",
        "a voice message's transcript is not its own: not this profile and message. {context}"
    );
    assert_eq!(
        value("music-for"),
        "none",
        "a music file got a transcript; only voice messages are transcribed. {context}"
    );
    assert_eq!(
        (value("voice-bubble"), value("music-bubble")),
        ("-|Transcribe".to_string(), "none".to_string()),
        "the voice bubble does not offer Transcribe, or music does. {context}"
    );
    assert_eq!(
        (value("none-copy"), value("none-transcribe")),
        ("false".to_string(), "true".to_string()),
        "before any transcript, the menu should offer Transcribe and not Copy transcript. \
         {context}"
    );
    assert_eq!(
        (value("music-copy"), value("music-transcribe")),
        ("false".to_string(), "false".to_string()),
        "a music file's menu offers transcription. {context}"
    );
    assert_eq!(
        value("bubble-asked"),
        "transcribe:21:false;",
        "the bubble's button did not ask the message's own transcript. {context}"
    );
    assert_eq!(
        value("menu-asked"),
        "transcribe:21:false;",
        "the menu's Transcribe did not ask the message's own transcript. {context}"
    );
    assert_eq!(
        (value("manual-copy"), value("manual-transcribe")),
        ("false".to_string(), "false".to_string()),
        "a voice message already queued by a tap still offers Transcribe. {context}"
    );
    assert_eq!(
        (value("running-copy"), value("running-transcribe")),
        ("false".to_string(), "false".to_string()),
        "a voice message being transcribed offers to copy or start again. {context}"
    );
    assert_eq!(
        value("other-bubble"),
        "This voice message seems to be in Español, which is not selected.|Transcribe anyway",
        "another language is not said, or not offered anyway. {context}"
    );
    assert_eq!(
        value("other-asked"),
        "transcribe:21:true;",
        "Transcribe anyway kept the language check. {context}"
    );
    assert_eq!(
        value("other-copy"),
        "false",
        "a voice message in another language offers Copy transcript. {context}"
    );
    assert_eq!(
        (value("failed-copy"), value("failed-transcribe")),
        ("false".to_string(), "true".to_string()),
        "a failed voice message does not offer Transcribe again, or offers Copy \
         transcript. {context}"
    );
    assert_eq!(
        (value("done-copy"), value("done-transcribe")),
        ("true".to_string(), "false".to_string()),
        "a transcribed voice message does not offer Copy transcript, or still offers \
         Transcribe. {context}"
    );
    assert_eq!(
        value("copied"),
        "copy:Running ten minutes late.;",
        "Copy transcript did not hand over the transcript. {context}"
    );
    assert_eq!(
        (
            value("off-for"),
            value("off-bubble"),
            value("off-copy"),
            value("off-transcribe")
        ),
        (
            "none".to_string(),
            "none".to_string(),
            "false".to_string(),
            "false".to_string()
        ),
        "with transcription off a voice message still shows or offers a transcript. {context}"
    );
    assert_eq!(
        value("on-for"),
        "5/21",
        "turning transcription on again did not bring the transcript back. {context}"
    );
}
