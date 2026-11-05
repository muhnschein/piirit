//! Quote of a voice message shows its done transcript, else core's words; a text quote never
//! asks for one, nor anything while transcription is off. Replying to a voice message hands
//! the reply bar its transcript.
//!
//! `ConversationList.qml` vs stub Silica, stand-ins for `Transcriber` and `Transcript`.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 1600

        property string made: ''
        property string replied: ''

        QtObject {
            id: transcriber
            property bool enabled: true
            property int model_state: 2
            property int model_permille: 0
            property bool power_saving: false
            property bool charging: false
            property int mode: 0
        }

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
                Component.onCompleted: made += account_id + '/' + message_id + ';'
                function transcribe(anyLanguage) {}
                function cancel() {}
            }
        }

        function row(id, viewType, file, quoted, quoteVoice, quoteText) {
            return {
                message_id: id, text: '', styled_text: '',
                is_outgoing: false, is_info: false, show_padlock: true,
                state: 16, timestamp: 1700000000 + id, day_number: 19675,
                sender_name: 'Ada', sender_color: '#00875a',
                is_forwarded: false, quote_text: quoteText, quote_author: 'Grace',
                quote_message_id: quoted, quote_is_voice: quoteVoice,
                file_path: file, file_name: file.split('/').pop(),
                file_mime: 'audio/mpeg', file_bytes: 34000,
                view_type: viewType, image_width: 0, image_height: 0,
                is_new: false, has_html: false, is_edited: false,
                download_state: 'Done', vcard_name: '', vcard_addr: '',
                vcard_color: '', webxdc_name: '', webxdc_document: '',
                webxdc_summary: '', webxdc_icon: '', reactions: '',
                my_reaction: '', loaded: true, duration_ms: 9000,
                call_state: '', call_has_video: false, call_duration: 0
            }
        }

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 1600 }
        function load(url) {
            rows.append(row(21, 'Voice', '/tmp/piirit-voice/voice.mp3', 0, false, ''))
            rows.append(row(30, 'Text', '', 21, true, '🎤 Voice message'))
            rows.append(row(31, 'Text', '', 20, false, 'see you'))
            list.setSource(url, {
                model: rows, accountId: 5, transcriber: transcriber,
                transcriptMaker: fakeTranscript
            })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.replyRequested.connect(function (messageId, body, author) {
                replied += messageId + ':' + body + ';'
            })
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
        function quoteOf(index) {
            var quote = findIn(rowsInOrder()[index], 'quoteRow')
            return quote && quote.item ? quote.item : null
        }
        // What quote says, and whose transcript it holds.
        function quote(index) {
            var q = quoteOf(index)
            if (!q) { return 'none' }
            var t = q.transcript
            return q.said + '|' + (t ? t.account_id + '/' + t.message_id : 'no transcript')
        }
        function setQuoted(index, json) {
            var t = quoteOf(index).transcript
            if (!t) { return 'none' }
            var values = JSON.parse(json)
            for (var key in values) { t[key] = values[key] }
            return 'ok'
        }
        function setVoice(json) {
            var t = rowsInOrder()[0].transcript
            if (!t) { return 'none' }
            var values = JSON.parse(json)
            for (var key in values) { t[key] = values[key] }
            return 'ok'
        }
        function reply(index) {
            var item = findIn(rowsInOrder()[index].menu, 'replyItem')
            if (!item) { return 'missing' }
            item.clicked()
            var was = replied
            replied = ''
            return was
        }
        function setEnabled(on) { transcriber.enabled = on; return 'ok' }
        function heardMade() { var was = made; made = ''; return was }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_quoted_voice_message_shows_its_transcript() {
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

    let mut plan = common::Plan::default();
    let s = steps.clone();
    plan.after(1, move || unsafe {
        let url = QString::from(common::component_url("ConversationList.qml"));
        common::record(&s, "load", call!("load", url));
    });
    let s = steps.clone();
    plan.after(1, move || unsafe {
        common::record(&s, "made", call!("heardMade"));
        common::record(&s, "voice-quote", call!("quote", 1));
        common::record(&s, "text-quote", call!("quote", 2));
        call!(
            "setQuoted",
            1,
            QString::from(r#"{"state": 2, "text": "Running"}"#)
        );
        common::record(&s, "running-quote", call!("quote", 1));
        call!(
            "setQuoted",
            1,
            QString::from(r#"{"state": 3, "text": "Running late."}"#)
        );
        common::record(&s, "done-quote", call!("quote", 1));

        call!(
            "setVoice",
            QString::from(r#"{"state": 2, "text": "Running"}"#)
        );
        common::record(&s, "reply-before", call!("reply", 0));
        call!(
            "setVoice",
            QString::from(r#"{"state": 3, "text": "Running late."}"#)
        );
        common::record(&s, "reply-done", call!("reply", 0));

        call!("setEnabled", false);
        common::record(&s, "off-quote", call!("quote", 1));
        call!("setEnabled", true);
        common::record(&s, "on-quote", call!("quote", 1));
        (*engine_ptr).quit();
    });
    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(
        value("made"),
        "5/21;5/21;",
        "transcripts made for other than the voice bubble and its quote. {context}"
    );
    assert_eq!(
        value("voice-quote"),
        "🎤 Voice message|5/21",
        "a voice quote does not follow the quoted message's transcript. {context}"
    );
    assert_eq!(
        value("text-quote"),
        "see you|no transcript",
        "a text quote asked for a transcript. {context}"
    );
    assert_eq!(
        value("running-quote"),
        "🎤 Voice message|5/21",
        "a quote showed text still being written. {context}"
    );
    assert_eq!(
        value("done-quote"),
        "🎤 Running late.|5/21",
        "a quote does not show the done transcript. {context}"
    );
    assert_eq!(
        (value("reply-before"), value("reply-done")),
        ("21:;".to_string(), "21:🎤 Running late.;".to_string()),
        "replying to a voice message hands over other than its done transcript. {context}"
    );
    assert_eq!(
        value("off-quote"),
        "🎤 Voice message|no transcript",
        "switched off, a quote still shows a transcript. {context}"
    );
    assert_eq!(
        value("on-quote"),
        "🎤 Voice message|5/21",
        "switched on again, the quote has no transcript. {context}"
    );
}
