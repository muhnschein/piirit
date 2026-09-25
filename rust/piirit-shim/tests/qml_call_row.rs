//! A call in a chat is a call's row: what kind of call, or what became of
//! it, and how long it lasted -- and a tap on it is the page's to act on.
//!
//! The core writes a sentence into a call's text, in English whatever the
//! phone's language, and rewrites it as the call moves. The row reads the
//! call's state instead (`call_info`, through the model's `call_state`),
//! and says it in Delta Chat's own words, whose translations come with
//! them. Where the core would not say what state a call is in, the row
//! falls back to showing the sentence, which is still true.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 400

        property string raised: ''
        ListModel { id: rows }
        Loader { id: list; width: 540; height: 400 }

        function row(outgoing, state, video, duration) {
            return {
                message_id: 7, text: 'Outgoing audio call', is_outgoing: outgoing,
                is_info: false, show_padlock: true, state: 16,
                timestamp: 1700000000, day_number: 19675,
                sender_name: 'Ada', sender_color: '#00875a',
                quote_text: '', quote_author: '', file_path: '',
                file_name: '', view_type: 'Call',
                image_width: 0, image_height: 0, reactions: '',
                download_state: 'Done', loaded: true,
                call_state: state, call_has_video: video,
                call_duration: duration
            }
        }
        function loadList(url) {
            rows.append(row(true, 'Completed', false, 185))
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.callRequested.connect(function(id, outgoing, state) {
                raised = 'call:' + id + ':' + outgoing + ':' + state
            })
            return 'ok'
        }
        function show(outgoing, state, video, duration) {
            rows.set(0, row(outgoing, state, video, duration))
            raised = ''
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
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        // What the row says: the call's two lines, whether the text is
        // drawn, and the icon.
        function says() {
            var row = findIn(list.item, 'messageRow')
            if (!row) { return 'missing:messageRow' }
            var callRow = findIn(row, 'callRow')
            var title = findIn(row, 'callTitle')
            var detail = findIn(row, 'callDetail')
            var body = findIn(row, 'messageLabel')
            var icon = findIn(row, 'callIcon')
            if (!callRow || !title || !detail || !body || !icon) { return 'missing' }
            return callRow.visible + '|' + title.text + '|' + detail.text + '|'
                   + body.visible + '|' + (('' + icon.source).indexOf('video') >= 0 ? 'video' : 'call')
                   + '|' + (callRow.height > 0)
        }
        function tapRow() {
            var row = findIn(list.item, 'messageRow')
            if (!row) { return 'missing:messageRow' }
            row.clicked()
            return raised
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_call_draws_as_what_became_of_it_and_a_tap_goes_to_the_page() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    macro_rules! show {
        ($outgoing:expr, $state:expr, $video:expr, $duration:expr) => {
            call!("show", $outgoing, QString::from($state), $video, $duration)
        };
    }

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "loadList",
                QString::from(common::component_url("ConversationList.qml"))
            )
        );
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("completed", call!("says"));
        record!("completed-tap", call!("tapRow"));
        show!(true, "Completed", false, 30);
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("short", call!("says"));
        show!(false, "Missed", false, 0);
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("missed", call!("says"));
        record!("missed-tap", call!("tapRow"));
        show!(false, "Alerting", false, 0);
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("ringing-here", call!("says"));
        record!("ringing-tap", call!("tapRow"));
        show!(true, "Alerting", false, 0);
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("ringing-there", call!("says"));
        show!(true, "Declined", true, 0);
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("declined-video", call!("says"));
        show!(true, "Canceled", false, 0);
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("canceled", call!("says"));
        // The core would not say: the sentence it wrote stands.
        show!(true, "", false, 0);
    });
    single_shot(Duration::from_secs(9), move || unsafe {
        record!("unknown", call!("says"));
        record!("unknown-tap", call!("tapRow"));
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

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    // The source forms: this loads no catalog.
    assert_eq!(
        value("completed"),
        "true|Audio call|3 minute(s) duration|false|call|true",
        "a completed call does not say what it was and how long it lasted, \
         in place of the core's sentence. {context}"
    );
    assert_eq!(
        value("completed-tap"),
        "call:7:true:Completed",
        "a tap on a call's row did not go to the page. {context}"
    );
    assert_eq!(
        value("short"),
        "true|Audio call|Less than 1 minute|false|call|true",
        "{context}"
    );
    assert_eq!(
        value("missed"),
        "true|Missed call||false|call|true",
        "{context}"
    );
    assert_eq!(
        value("missed-tap"),
        "call:7:false:Missed",
        "a tap on a missed call did not go to the page, to call back. {context}"
    );
    assert_eq!(
        value("ringing-here"),
        "true|Audio call|Incoming call|false|call|true",
        "{context}"
    );
    assert_eq!(
        value("ringing-tap"),
        "call:7:false:Alerting",
        "a tap on a call still ringing here did not go to the page. {context}"
    );
    assert_eq!(
        value("ringing-there"),
        "true|Audio call|Ringing…|false|call|true",
        "{context}"
    );
    assert_eq!(
        value("declined-video"),
        "true|Declined call||false|video|true",
        "{context}"
    );
    assert_eq!(
        value("canceled"),
        "true|Canceled call||false|call|true",
        "{context}"
    );
    assert_eq!(
        value("unknown"),
        "false|Audio call||true|call|false",
        "a call the core said nothing about did not fall back to its \
         sentence. {context}"
    );
    assert_eq!(
        value("unknown-tap"),
        "",
        "a call the core said nothing about was acted on. {context}"
    );
}
