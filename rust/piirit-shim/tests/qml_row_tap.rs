//! Message is one surface: tap opens whatever it must, long press anywhere (picture
//! included) opens its menu.
//!
//! Nothing in row takes press but few small controls needing one; those pass long
//! press to row. A picture taking press itself would block row menu on long press.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

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

        function row(viewType, path, downloadState) {
            return {
                message_id: 7, text: '', is_outgoing: false,
                is_info: false, show_padlock: true, state: 16,
                timestamp: 1700000000, day_number: 19675,
                sender_name: 'Ada', sender_color: '#00875a',
                quote_text: '', quote_author: '', file_path: path,
                file_name: 'holiday.png', view_type: viewType,
                image_width: 0, image_height: 0, reactions: '',
                download_state: downloadState, loaded: true,
                has_location: false, starts_location: false, sender_id: 0
            }
        }
        function loadList(url) {
            rows.append(row('Image', '/tmp/holiday.png', 'Done'))
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.openRequested.connect(
                function(fileUrl, fileName, viewType, previewWidth, messageId) {
                    // Id: gallery opens at this message.
                    raised = 'open:' + viewType + ':' + fileName + ':' + (previewWidth > 0)
                             + ':' + messageId
                })
            list.item.downloadRequested.connect(function(id) {
                raised = 'download:' + id
            })
            list.item.appRequested.connect(function(id) {
                raised = 'app:' + id
            })
            list.item.locationRequested.connect(function(id, sender, name) {
                raised = 'location:' + id + ':' + sender + ':' + name
            })
            return 'ok'
        }
        function setApps(on) { list.item.appsEnabled = on; return 'ok' }
        // Text with a point, or core's notice that sender shares live.
        function showLocated(notice) {
            var located = row('Text', '', 'Done')
            located.text = notice ? 'Location streaming enabled by Ada.' : 'https://osm.org'
            located.is_info = notice
            located.has_location = !notice
            located.starts_location = notice
            located.sender_id = 10
            rows.set(0, located)
            raised = ''
            return 'ok'
        }
        function show(viewType, path, downloadState) {
            rows.set(0, row(viewType, path, downloadState))
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
        // The row's own tap, as Silica raises it for a press anywhere on
        // the row.
        function tapRow() {
            var row = findIn(list.item, 'messageRow')
            if (!row) { return 'missing:messageRow' }
            row.clicked()
            return raised
        }
        // A long press on a control that takes presses for itself.
        function holdControl(name) {
            var row = findIn(list.item, 'messageRow')
            if (!row) { return 'missing:messageRow' }
            var control = findIn(row, name)
            if (!control) { return 'missing:' + name }
            control.pressAndHold()
            return '' + row.menuOpen
        }
        function menuOpen() {
            var row = findIn(list.item, 'messageRow')
            return row ? '' + row.menuOpen : 'missing:messageRow'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_tap_opens_the_attachment_and_a_long_press_anywhere_opens_the_menu() {
    // SAFETY: single-threaded; set before Qt starts.
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
        ($kind:expr, $path:expr, $state:expr) => {
            call!(
                "show",
                QString::from($kind),
                QString::from($path),
                QString::from($state)
            )
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "load",
            call!(
                "loadList",
                QString::from(common::component_url("ConversationList.qml"))
            )
        );
    });
    plan.after(1, move || unsafe {
        record!("picture-tap", call!("tapRow"));
        record!("menu-before", call!("menuOpen"));
        show!("Video", "/tmp/clip.mp4", "Done");
    });
    plan.after(1, move || unsafe {
        record!("video-tap", call!("tapRow"));
        show!("File", "/tmp/report.pdf", "Done");
    });
    plan.after(1, move || unsafe {
        record!("file-tap", call!("tapRow"));
        show!("Voice", "/tmp/note.ogg", "Done");
    });
    plan.after(1, move || unsafe {
        // Sound plays where it sits: tap on row opens nothing;
        // long press on its play button is still the menu.
        record!("voice-tap", call!("tapRow"));
        record!(
            "voice-hold",
            call!("holdControl", QString::from("audioPlayButton"))
        );
        show!("Text", "", "Available");
    });
    plan.after(1, move || unsafe {
        // Message limit held back: whole row asks for rest.
        record!("held-tap", call!("tapRow"));
        show!("Text", "", "Done");
    });
    plan.after(1, move || unsafe {
        record!("text-tap", call!("tapRow"));
        // Webxdc app, apps off: nothing to run, so row hands the .xdc on like any file. Every
        // probe row is called holiday.png, this one too.
        show!("Webxdc", "/tmp/checkers.xdc", "Done");
    });
    plan.after(1, move || unsafe {
        record!("xdc-tap-off", call!("tapRow"));
        record!("apps-on", call!("setApps", true));
    });
    plan.after(1, move || unsafe {
        // Apps on: run here, not handed to whatever system thinks opens .xdc.
        record!("xdc-tap-on", call!("tapRow"));
        call!("showLocated", false);
    });
    plan.after(1, move || unsafe {
        // Map of row's own point; then of sender's newest, from their notice.
        record!("point-tap", call!("tapRow"));
        call!("showLocated", true);
    });
    plan.after(1, move || unsafe {
        record!("notice-tap", call!("tapRow"));
        (*engine_ptr).quit();
    });

    plan.run();
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
    assert_eq!(
        value("picture-tap"),
        "open:Image:holiday.png:true:7",
        "a tap on a picture's row did not open the picture, at the width \
         the row drew it, with the message it belongs to (where the \
         gallery opens). {context}"
    );
    assert_eq!(
        value("menu-before"),
        "false",
        "a tap opened the menu. {context}"
    );
    assert_eq!(
        value("video-tap"),
        "open:Video:holiday.png:true:7",
        "a tap on a video's row did not open the video. {context}"
    );
    assert_eq!(
        value("file-tap"),
        "open:File:holiday.png:true:7",
        "a tap on a file's row did not open the file. {context}"
    );
    assert_eq!(
        value("voice-tap"),
        "",
        "a tap on a voice message's row opened something: it plays where \
         it sits. {context}"
    );
    assert_eq!(
        value("voice-hold"),
        "true",
        "a long press on the play button did not reach the row's menu. {context}"
    );
    assert_eq!(
        value("held-tap"),
        "download:7",
        "a tap on a held-back message did not ask for the rest of it. {context}"
    );
    assert_eq!(
        value("text-tap"),
        "",
        "a tap on a plain text message did something. {context}"
    );
    assert_eq!(
        value("xdc-tap-off"),
        "open:Webxdc:holiday.png:true:7",
        "a tap on a .xdc did not hand it on where apps are off, so the \
         reader has a row that does nothing. {context}"
    );
    assert_eq!(value("apps-on"), "ok", "apps were not turned on. {context}");
    assert_eq!(
        value("xdc-tap-on"),
        "app:7",
        "a tap on a webxdc did not ask to run it. {context}"
    );
    assert_eq!(
        value("point-tap"),
        "location:7:10:Ada",
        "a tap on a row with a point did not ask for its map. {context}"
    );
    assert_eq!(
        value("notice-tap"),
        "location:7:10:Ada",
        "a tap on live sharing's notice did not ask for its sender's map. {context}"
    );
}
