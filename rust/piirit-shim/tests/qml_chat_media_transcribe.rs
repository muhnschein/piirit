//! A chat's Audio list, reached from contact and group pages: voice rows draw their transcript,
//! folded to three lines; row menu offers Transcribe, then Copy transcript; pulley offers
//! "Transcribe N voice messages" for those without one, counted by the app's transcriber, and
//! is gone at none or with transcription off.
//!
//! Real page vs stub Silica and fake core (group with a voice message, `PIIRIT_FAKE_MEDIA`);
//! stand-ins for `Transcriber` and `Transcript` (shim's are stubs until store lands).

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        width: 540
        height: 960
        property string calls: ''

        QtObject {
            id: transcriber
            property bool enabled: true
            property int model_state: 2
            property int model_permille: 0
            property bool power_saving: false
            property bool charging: false
            property int mode: 0
            // What count_chat answers next.
            property int untranscribed: 0
            signal chat_counted(int account_id, int chat_id, int count)
            signal store_changed()
            function count_chat(accountId, chatId) {
                chat_counted(accountId, chatId, untranscribed)
            }
            function transcribe_chat(accountId, chatId) {
                calls += 'transcribe_chat:' + accountId + '/' + chatId + ';'
            }
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
                function transcribe(anyLanguage) {
                    calls += 'transcribe:' + message_id + ':' + anyLanguage + ';'
                }
                function cancel() {}
            }
        }

        Loader { id: loader; width: 540; height: 960 }
        function load(url) {
            loader.setSource(url, {
                accountId: 1, chatId: 2, kind: 'audio', transcriber: transcriber,
                transcriptMaker: fakeTranscript
            })
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
                var inside = findIn(node.contentItem, name)
                if (inside) { return inside }
            }
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function offered(rowName, itemName) {
            var row = findIn(loader.item, rowName)
            if (!row) { return 'missing:' + rowName }
            var item = findIn(row.menu, itemName)
            return item ? '' + item.visible : 'missing:' + itemName
        }
        function transcriptOf(rowName) {
            var preview = findIn(findIn(loader.item, rowName), 'attachment')
            if (!preview) { return 'missing' }
            var t = preview.transcript
            return t ? t.account_id + '/' + t.message_id : 'none'
        }
        function setDone(rowName) {
            var t = findIn(findIn(loader.item, rowName), 'attachment').transcript
            t.state = 3
            t.text = 'See you at eight.'
            return 'ok'
        }
        function pulley() {
            var menu = findIn(loader.item, 'transcribePulley')
            var item = findIn(loader.item, 'transcribeChatItem')
            if (!menu || !item) { return 'missing' }
            return menu.visible ? item.text : 'hidden'
        }
        function counts(n) { transcriber.untranscribed = n; transcriber.store_changed(); return 'ok' }
        function answerFor(accountId, chatId, n) {
            transcriber.chat_counted(accountId, chatId, n)
            return 'ok'
        }
        // Message arrives or goes: model says so, count not pushed.
        function arrives(n) {
            transcriber.untranscribed = n
            var media = findIn(loader.item, 'media')
            if (!media) { return 'no-media' }
            media.rows_changed()
            return 'ok'
        }
        function setEnabled(on) { transcriber.enabled = on; return 'ok' }
        function heard() { var was = calls; calls = ''; return was }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_audio_list_transcribes_a_chats_voice_messages() {
    let temp = std::env::temp_dir().join(format!("piirit-media-transcribe-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_MEDIA", "1");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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
    macro_rules! q {
        ($text:expr) => {
            QString::from($text)
        };
    }

    let mut plan = common::Plan::default();

    let s = steps.clone();
    plan.after(1, move || unsafe {
        let url = q!(common::page_url("ChatMediaPage.qml"));
        common::record(&s, "load", call!("load", url));
    });

    // Rows arrive from core.
    let s = steps.clone();
    plan.until(1, 20, move |last| unsafe {
        let found = call!("transcriptOf", q!("mediaRow13")).to_string();
        if found == "missing" && !last {
            return false;
        }
        common::record(&s, "voice-for", found.into());
        true
    });

    let s = steps.clone();
    plan.after(1, move || unsafe {
        common::record(&s, "none-left", call!("pulley"));
        common::record(
            &s,
            "voice-compact",
            call!("get", q!("voiceTranscript"), q!("compact")),
        );
        common::record(
            &s,
            "voice-button",
            call!("get", q!("transcriptButton"), q!("text")),
        );
        common::record(
            &s,
            "menu-copy",
            call!("offered", q!("mediaRow13"), q!("copyTranscriptItem")),
        );
        common::record(
            &s,
            "menu-transcribe",
            call!("offered", q!("mediaRow13"), q!("transcribeItem")),
        );
        common::record(
            &s,
            "menu-open",
            call!("offered", q!("mediaRow13"), q!("openItem")),
        );
        common::record(
            &s,
            "menu-save",
            call!("offered", q!("mediaRow13"), q!("saveItem")),
        );

        common::record(&s, "count-two", call!("counts", 2));
        common::record(&s, "two-left", call!("pulley"));
        // Another chat's count: not this page's.
        call!("answerFor", 1, 3, 7);
        common::record(&s, "other-chat", call!("pulley"));
        common::record(&s, "pull", call!("click", q!("transcribeChatItem")));
        common::record(&s, "pulled", call!("heard"));

        common::record(&s, "done", call!("setDone", q!("mediaRow13")));
        common::record(
            &s,
            "done-copy",
            call!("offered", q!("mediaRow13"), q!("copyTranscriptItem")),
        );
        common::record(
            &s,
            "done-transcribe",
            call!("offered", q!("mediaRow13"), q!("transcribeItem")),
        );

        common::record(&s, "count-none", call!("counts", 0));
        common::record(&s, "none-again", call!("pulley"));
        common::record(&s, "arrives", call!("arrives", 3));
        common::record(&s, "arrived", call!("pulley"));

        call!("counts", 2);
        common::record(&s, "off", call!("setEnabled", false));
        common::record(&s, "off-pulley", call!("pulley"));
        common::record(&s, "off-for", call!("transcriptOf", q!("mediaRow13")));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();
    let _ = std::fs::remove_dir_all(&temp);

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the Audio page did not load. {context}"
    );
    assert_eq!(
        value("voice-for"),
        "1/13",
        "the voice row's transcript is not its own message's. {context}"
    );
    assert_eq!(
        value("none-left"),
        "hidden",
        "the pulley offers to transcribe none. {context}"
    );
    assert_eq!(
        (value("voice-compact"), value("voice-button")),
        ("true".to_string(), "Transcribe".to_string()),
        "the Audio list's voice row does not draw a folded transcript with its button. \
         {context}"
    );
    assert_eq!(
        (value("menu-copy"), value("menu-transcribe")),
        ("false".to_string(), "true".to_string()),
        "the row menu should offer Transcribe before there is a transcript. {context}"
    );
    // #297: Open handed voice message to other app, which failed.
    assert_eq!(
        (value("menu-open"), value("menu-save")),
        ("false".to_string(), "true".to_string()),
        "the Audio list offers to open a voice message in another app, or no longer \
         offers to save it. {context}"
    );
    assert_eq!(
        value("two-left"),
        "Transcribe 2 voice message(s)",
        "the pulley does not say how many voice messages it would transcribe. {context}"
    );
    assert_eq!(
        value("other-chat"),
        "Transcribe 2 voice message(s)",
        "another chat's count reached this page's pulley. {context}"
    );
    assert_eq!(
        value("pulled"),
        "transcribe_chat:1/2;",
        "the pulley did not ask to transcribe this chat. {context}"
    );
    assert_eq!(
        (value("done-copy"), value("done-transcribe")),
        ("true".to_string(), "false".to_string()),
        "a transcribed row does not offer Copy transcript. {context}"
    );
    assert_eq!(
        value("none-again"),
        "hidden",
        "the pulley stays with nothing left. {context}"
    );
    assert_eq!(
        (value("arrives"), value("arrived")),
        (
            "ok".to_string(),
            "Transcribe 3 voice message(s)".to_string()
        ),
        "a voice message arriving did not count again. {context}"
    );
    assert_eq!(
        (value("off-pulley"), value("off-for")),
        ("hidden".to_string(), "none".to_string()),
        "with transcription off the Audio list still transcribes. {context}"
    );
}
