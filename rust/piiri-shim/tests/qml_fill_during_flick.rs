//! The rows in front of a flicking reader are asked for while they move.
//!
//! The ask was debounced by a timer restarted on every change to
//! `contentY`, and a flick changes it every frame: the timer did not fire
//! until the flick had stopped, so nothing was fetched for as long as the
//! reader was moving, and a fast scroll up into the history ended on a
//! screen of blanks that filled in a moment later. Reported from a phone as
//! an empty screen for a second.
//!
//! What is pinned here is that a view moving frame after frame for half a
//! second asks for rows several times over while it moves, and once more
//! for where it stops.

// Qt harness: see qml_conversation_list.rs.
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

/// The list over a plain `ListModel` of rows that have not been fetched:
/// no `loaded` role reads as false, which is a placeholder.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 400

        ListModel { id: rows }

        Loader {
            id: loader
            anchors.fill: parent
        }

        property int asksWhileMoving: 0
        property int asksAfter: 0
        property bool moving: false

        function append(count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    message_id: rows.count + 1, text: '',
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + rows.count,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '', quote_author: '',
                    file_path: '', file_name: '', view_type: 'Text',
                    image_width: 0, image_height: 0
                })
            }
            return '' + rows.count
        }

        function load(url) {
            loader.setSource(url, { model: rows })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            var view = loader.item
            view.messageCount = Qt.binding(function() { return rows.count })
            view.hydrateRequested.connect(function(first, last) {
                if (moving) { asksWhileMoving += 1 } else { asksAfter += 1 }
            })
            return 'ok'
        }
        function toEnd() { loader.item.positionViewAtEnd(); return 'ok' }

        // A flick, frame by frame: the view moves up thirty pixels every
        // sixteen milliseconds for half a second, as an inertial flick
        // does, with no pause in it for a debounce to fire in.
        Timer {
            id: flick
            interval: 16
            repeat: true
            property int ticks: 0
            onTriggered: {
                loader.item.contentY -= 30
                ticks += 1
                if (ticks >= 30) {
                    flick.stop()
                    moving = false
                    loader.item.movementEnded()
                }
            }
        }
        function startFlick() {
            asksWhileMoving = 0
            asksAfter = 0
            moving = true
            flick.ticks = 0
            loader.item.movementStarted()
            flick.start()
            return 'ok'
        }
        function duringCount() { return '' + asksWhileMoving }
        function afterCount() { return '' + asksAfter }
    }
";

#[test]
fn a_flicking_view_asks_for_rows_while_it_moves() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    piiri_shim::register_qml_types();

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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml"))
            ),
        ));
        (*steps_ptr).push(("rows", call!("append", 300)));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("end", call!("toEnd")));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("flick", call!("startFlick")));
    });
    single_shot(Duration::from_millis(4500), move || unsafe {
        (*steps_ptr).push(("during", call!("duringCount")));
        (*steps_ptr).push(("after", call!("afterCount")));
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
    let number = |label: &str| value(label).parse::<u32>().unwrap_or(0);
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    assert_eq!(value("rows"), "300", "the rows were not added. {context}");

    // Half a second of movement is several sixty-millisecond waits: the
    // rows in front of the reader were asked for while they were still on
    // their way there.
    assert!(
        number("during") >= 3,
        "the view asked for rows {} times during half a second of flicking: \
         the ask waited for the flick to stop, and the reader arrived on a \
         screen of blanks. {context}",
        value("during")
    );
    // And once it stops, where it stopped.
    assert!(
        number("after") >= 1,
        "the view did not ask for the rows where the flick ended. {context}"
    );
}
