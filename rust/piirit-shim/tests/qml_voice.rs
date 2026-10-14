//! Recording strip and recorder, on a machine that cannot record.
//! Checked (no mic needed): recorder without encoder says unavailable, not pretending; strip
//! offers and starts nothing; page shows send button, not a mic. Encoders existing: device
//! question (docs/HARBOUR.md). Limit too: longest recording relay takes follows limit and
//! media quality strip is handed; strip time says nothing of it.

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
        property string heard: ''
        Loader { id: loader; width: 540 }
        function load(url) {
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.recorded.connect(function(path) { heard = 'recorded:' + path })
            loader.item.failed.connect(function(message) { heard = 'failed:' + message })
            return 'ok'
        }
        function get(property) { return '' + loader.item[property] }
        function start() { loader.item.start(); return get('recording') }
        function heardText() { return heard }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function extension() {
            var recorder = findIn(loader.item, 'recorder')
            return recorder ? '' + recorder.extension : 'missing:recorder'
        }
        function limit(bytes, quality) {
            loader.item.limitBytes = bytes
            loader.item.mediaQuality = quality
            var recorder = findIn(loader.item, 'recorder')
            return recorder ? '' + recorder.limit_ms : 'missing:recorder'
        }
        function timeText() {
            var label = findIn(loader.item, 'recordingTime')
            return label ? '' + label.text : 'missing:recordingTime'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_machine_that_cannot_record_is_told_so_and_records_nothing() {
    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    piirit_shim::register_qml_types();

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
            call!("load", QString::from(common::component_url("VoiceBar.qml"))),
        ));
        (*steps_ptr).push(("available", call!("get", QString::from("available"))));
        (*steps_ptr).push(("extension", call!("extension")));
        (*steps_ptr).push(("hidden", call!("get", QString::from("visible"))));
        (*steps_ptr).push(("height", call!("get", QString::from("height"))));
        (*steps_ptr).push(("start", call!("start")));
        (*steps_ptr).push(("limit-unknown", call!("limit", 0.0_f64, 0_i32)));
        // Core recommends (30 - 1) MiB * 3/4, at both bit rates.
        (*steps_ptr).push(("limit-balanced", call!("limit", 22_806_528.0_f64, 0_i32)));
        (*steps_ptr).push(("limit-less-data", call!("limit", 22_806_528.0_f64, 1_i32)));
        (*steps_ptr).push(("time", call!("timeText")));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("heard", call!("heardText")));
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

    assert_eq!(value("load"), "ok", "the strip did not load. {context}");
    // No multimedia backend on test runner: must say so, not offer a mic.
    assert_eq!(
        value("available"),
        "false",
        "a recorder with no encoder claims it can record. {context}"
    );
    assert_eq!(
        value("extension"),
        "",
        "a recorder with no encoder names an extension. {context}"
    );
    assert_eq!(
        value("hidden"),
        "false",
        "the strip is showing with nothing recording. {context}"
    );
    assert_eq!(
        value("height"),
        "0",
        "the strip takes room with nothing recording. {context}"
    );
    assert_eq!(
        value("start"),
        "false",
        "starting on a machine that cannot record started something. {context}"
    );
    assert_eq!(
        value("heard"),
        "",
        "a start that could not happen was reported as something. {context}"
    );

    // No limit known: only reader's end.
    assert_eq!(value("limit-unknown"), "0", "{context}");
    // 32 kbit/s = 4000 bytes/s, 24 = 3000: limit less headroom, in MP3 ms.
    assert_eq!(
        value("limit-balanced"),
        "5697536",
        "the longest recording at the balanced bit rate is not what the \
         relay's limit holds. {context}"
    );
    assert_eq!(
        value("limit-less-data"),
        "7596714",
        "the lower media quality does not give a longer recording. {context}"
    );
    // Over 1.5h, which nobody records: recording stops and is sent there; time doesn't say.
    assert_eq!(
        value("time"),
        "Recording 0:00",
        "the time on the strip says something besides the time. {context}"
    );
}
