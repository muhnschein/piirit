//! Voice message transcript under its player: every state the States board draws, from the
//! transcript's state plus app-wide model, power and timing. Each shows its line and its one
//! button; button asks for a transcription with or without the language check; a running one
//! stops on tap; a long one folds (six lines and "Show all" in chat, three and tap in Audio
//! list); a voice file not yet on phone shows nothing.
//!
//! `VoiceTranscript.qml` alone vs stub Silica, driven by its plain properties. Wiring to
//! `Transcript`: `qml_transcript_wiring.rs`.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 400
        height: 900
        property string asked: ''
        Loader { id: loader; width: 300 }

        function load(url) {
            loader.setSource(url, { width: 300 })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.transcribeRequested.connect(function (anyLanguage) {
                asked += 'transcribe:' + anyLanguage + ';'
            })
            loader.item.stopRequested.connect(function () { asked += 'stop;' })
            return 'ok'
        }
        // Fresh state, then the case's own values.
        function show(json) {
            var item = loader.item
            var base = {
                transcriptState: 0, manual: false, permille: 0, text: '',
                language: '', hasFile: true, modelState: 2, modelPermille: 0,
                powerSaving: false, charging: false, mode: 0,
                phoneLanguage: 'en', compact: false, unfolded: false
            }
            var own = JSON.parse(json)
            for (var key in base) {
                item[key] = own[key] !== undefined ? own[key] : base[key]
            }
            asked = ''
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
        function shownText(name) {
            var node = findIn(loader.item, name)
            if (!node) { return 'missing:' + name }
            return node.visible && node.height > 0 ? '' + node.text : '-'
        }
        function shown(name) {
            var node = findIn(loader.item, name)
            if (!node) { return 'missing:' + name }
            return node.visible && node.height > 0 ? 'yes' : 'no'
        }
        // What the bubble says: drawn state, line, button, text, footer, bar, Show all.
        function drawn() {
            var item = loader.item
            return [item.drawn, shownText('transcriptStatus'),
                    shownText('transcriptButton'), shownText('transcriptText'),
                    shownText('transcriptFooter'), shown('transcriptModelBar'),
                    shown('transcriptShowAll'), item.visible ? 'visible' : 'hidden']
                .join('|')
        }
        function click(name) {
            var node = findIn(loader.item, name)
            if (!node) { return 'missing:' + name }
            node.clicked()
            return 'ok'
        }
        function busy() { return '' + findIn(loader.item, 'transcriptBusy').running }
        function tappable(name) { return '' + findIn(loader.item, name).enabled }
        function heard() { return asked }
        // Lines drawn, and whether more were cut.
        function fold() {
            var label = findIn(loader.item, 'transcriptText')
            return label.lineCount + ':' + label.truncated
        }
    }
";

const LONG: &str = "So the plan for the weekend is that we meet at the harbour at half past \
    seven, Bob brings the tent and the gas stove, Carol said she can take the car but only one \
    way, so somebody has to figure out how we get back on Sunday evening, and if it rains we go \
    to the museum instead, which opens at ten and has a café, and after that we can still take \
    the late ferry home if everyone agrees, otherwise we stay one more night at the camp site.";

#[test]
#[allow(clippy::too_many_lines)]
fn each_state_shows_its_line_and_its_button() {
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
    macro_rules! show {
        ($json:expr) => {
            call!("show", QString::from($json))
        };
    }

    let mut plan = common::Plan::default();

    let s = steps.clone();
    plan.after(1, move || unsafe {
        let url = QString::from(common::component_url("VoiceTranscript.qml"));
        common::record(&s, "load", call!("load", url));

        show!(r#"{"transcriptState": 0}"#);
        common::record(&s, "1-ready", call!("drawn"));
        common::record(
            &s,
            "1-tap",
            call!("click", QString::from("transcriptButton")),
        );
        common::record(&s, "1-asked", call!("heard"));

        show!(r#"{"transcriptState": 1, "mode": 1, "charging": false}"#);
        common::record(&s, "2-charger", call!("drawn"));
        common::record(
            &s,
            "2-tap",
            call!("click", QString::from("transcriptButton")),
        );
        common::record(&s, "2-asked", call!("heard"));
        // Charger in: queued behind others, not waiting for it.
        show!(r#"{"transcriptState": 1, "mode": 1, "charging": true}"#);
        common::record(&s, "2-charging", call!("drawn"));
        // Only backlog queues on its own under "Never"; backlog waits for charger.
        show!(r#"{"transcriptState": 1, "mode": 0, "charging": false}"#);
        common::record(&s, "2-backlog", call!("drawn"));

        show!(r#"{"transcriptState": 1, "mode": 2, "powerSaving": true, "charging": true}"#);
        common::record(&s, "3-saving", call!("drawn"));
        common::record(
            &s,
            "3-tap",
            call!("click", QString::from("transcriptButton")),
        );
        common::record(&s, "3-asked", call!("heard"));

        show!(r#"{"transcriptState": 1, "mode": 2}"#);
        common::record(&s, "4-waiting", call!("drawn"));
        // Tapped: charger and battery saving hold it back no more.
        show!(r#"{"transcriptState": 1, "mode": 1, "manual": true, "powerSaving": true}"#);
        common::record(&s, "4-manual", call!("drawn"));

        show!(r#"{"transcriptState": 1, "manual": true, "modelState": 1, "modelPermille": 345}"#);
        common::record(&s, "5-model", call!("drawn"));

        show!(r#"{"transcriptState": 2, "permille": 404, "text": "Okay so about Saturday"}"#);
        common::record(&s, "6-running", call!("drawn"));
        common::record(&s, "6-busy", call!("busy"));
        common::record(
            &s,
            "6-tap",
            call!("click", QString::from("transcriptStatusArea")),
        );
        common::record(&s, "6-asked", call!("heard"));

        show!(r#"{"transcriptState": 3, "text": "Moi! Tuletko saunaan?", "language": "fi"}"#);
        common::record(&s, "7-done", call!("drawn"));
        common::record(
            &s,
            "7-line-tappable",
            call!("tappable", QString::from("transcriptStatusArea")),
        );
        show!(r#"{"transcriptState": 3, "text": "Running late.", "language": "en"}"#);
        common::record(&s, "7-done-own", call!("drawn"));

        show!(format!(
            r#"{{"transcriptState": 3, "text": "{LONG}", "language": "en"}}"#
        ));
        common::record(&s, "8-long", call!("drawn"));
        common::record(&s, "8-fold", call!("fold"));
        common::record(
            &s,
            "8-unfold",
            call!("click", QString::from("transcriptShowAll")),
        );
        common::record(&s, "8-unfolded", call!("drawn"));
        common::record(&s, "8-unfold-lines", call!("fold"));

        show!(r#"{"transcriptState": 5, "language": "es"}"#);
        common::record(&s, "9-other", call!("drawn"));
        common::record(
            &s,
            "9-tap",
            call!("click", QString::from("transcriptButton")),
        );
        common::record(&s, "9-asked", call!("heard"));
        // Greyed language: nothing to pick, so no button.
        show!(r#"{"transcriptState": 5, "language": "lt"}"#);
        common::record(&s, "9-greyed", call!("drawn"));

        show!(r#"{"transcriptState": 4}"#);
        common::record(&s, "10-nospeech", call!("drawn"));

        show!(r#"{"transcriptState": 6}"#);
        common::record(&s, "11-failed", call!("drawn"));
        common::record(
            &s,
            "11-tap",
            call!("click", QString::from("transcriptButton")),
        );
        common::record(&s, "11-asked", call!("heard"));

        show!(r#"{"transcriptState": 3, "text": "Hidden.", "hasFile": false}"#);
        common::record(&s, "12-no-file", call!("drawn"));

        // Audio list: three lines, no "Show all", tap on text unfolds.
        show!(format!(
            r#"{{"transcriptState": 3, "text": "{LONG}", "language": "en", "compact": true}}"#
        ));
        common::record(&s, "list-long", call!("drawn"));
        common::record(&s, "list-fold", call!("fold"));
        common::record(
            &s,
            "list-tappable",
            call!("tappable", QString::from("transcriptTextArea")),
        );
        common::record(
            &s,
            "list-tap",
            call!("click", QString::from("transcriptTextArea")),
        );
        common::record(&s, "list-unfolded", call!("fold"));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the transcript did not load. {context}"
    );

    let cases = [
        ("1-ready", "ready|-|Transcribe|-|-|no|no|visible"),
        (
            "2-charger",
            "charger|This voice message is transcribed when the phone is charging.|\
             Transcribe now|-|-|no|no|visible",
        ),
        (
            "2-charging",
            "waiting|Waiting for other voice messages to be transcribed.|-|-|-|no|no|visible",
        ),
        (
            "2-backlog",
            "charger|This voice message is transcribed when the phone is charging.|\
             Transcribe now|-|-|no|no|visible",
        ),
        (
            "3-saving",
            "saving|Transcription is paused while battery saving mode is on.|\
             Transcribe now|-|-|no|no|visible",
        ),
        (
            "4-waiting",
            "waiting|Waiting for other voice messages to be transcribed.|-|-|-|no|no|visible",
        ),
        (
            "4-manual",
            "waiting|Waiting for other voice messages to be transcribed.|-|-|-|no|no|visible",
        ),
        (
            "5-model",
            "model|Downloading the speech model… 34%|-|-|-|yes|no|visible",
        ),
        (
            "6-running",
            "running|Transcribing… 40%|-|Okay so about Saturday|-|no|no|visible",
        ),
        (
            "7-done",
            "done|-|-|Moi! Tuletko saunaan?|Transcribed on this phone · Suomi|no|no|visible",
        ),
        (
            "7-done-own",
            "done|-|-|Running late.|Transcribed on this phone|no|no|visible",
        ),
        (
            "9-other",
            "other|This voice message seems to be in Español, which is not selected.|\
             Transcribe anyway|-|-|no|no|visible",
        ),
        (
            "9-greyed",
            "other|This voice message seems to be in Lietuvių, which is not selected.|\
             -|-|-|no|no|visible",
        ),
        (
            "10-nospeech",
            "nospeech|No speech was found in this voice message.|-|-|-|no|no|visible",
        ),
        (
            "11-failed",
            "failed|This voice message could not be transcribed.|Try again|-|-|no|no|visible",
        ),
        ("12-no-file", "hidden|-|-|-|-|no|no|hidden"),
    ];
    for (label, expected) in cases {
        assert_eq!(
            value(label),
            expected,
            "state {label} does not draw what the States board shows \
             (drawn|line|button|text|footer|bar|show all|visible). {context}"
        );
    }

    assert_eq!(
        value("1-asked"),
        "transcribe:false;",
        "Transcribe did not ask for a transcription in the chosen languages. {context}"
    );
    assert_eq!(
        (value("2-asked"), value("3-asked")),
        (
            "transcribe:false;".to_string(),
            "transcribe:false;".to_string()
        ),
        "Transcribe now did not ask to run it ahead of the charger or battery saving. {context}"
    );
    assert_eq!(
        value("6-busy"),
        "true",
        "nothing turns while transcribing. {context}"
    );
    assert_eq!(
        value("6-asked"),
        "stop;",
        "a tap on the running line does not stop it. {context}"
    );
    assert_eq!(
        value("7-line-tappable"),
        "false",
        "a finished transcript's line still stops something on tap. {context}"
    );
    assert_eq!(
        value("9-asked"),
        "transcribe:true;",
        "Transcribe anyway did not skip the language check. {context}"
    );
    assert_eq!(
        value("11-asked"),
        "transcribe:false;",
        "Try again did not ask again. {context}"
    );

    let long = value("8-long");
    assert!(
        long.starts_with("done|-|-|") && long.ends_with("|yes|visible"),
        "a long transcript in chat does not fold with Show all: {long}. {context}"
    );
    assert_eq!(
        value("8-fold"),
        "6:true",
        "a long transcript in chat is not cut at six lines. {context}"
    );
    let unfolded = value("8-unfolded");
    assert!(
        unfolded.ends_with("|no|visible") && value("8-unfold-lines").ends_with(":false"),
        "Show all did not open the whole transcript in place: {unfolded}, {}. {context}",
        value("8-unfold-lines")
    );
    assert!(
        value("list-long").ends_with("|no|visible"),
        "the Audio list offers Show all instead of a tap. {context}"
    );
    assert_eq!(
        value("list-fold"),
        "3:true",
        "a long transcript in the Audio list is not cut at three lines. {context}"
    );
    assert_eq!(
        value("list-tappable"),
        "true",
        "a folded transcript in the Audio list does not unfold on tap. {context}"
    );
    assert_eq!(
        value("list-tap"),
        "ok",
        "the Audio list's transcript could not be tapped. {context}"
    );
    assert!(
        value("list-unfolded").ends_with(":false"),
        "tapping a folded transcript in the Audio list did not unfold it. {context}"
    );
}
