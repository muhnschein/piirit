//! The recording strip, and the recorder under it, on a machine that
//! cannot record.
//!
//! What can be checked here is the half that does not need a
//! microphone: a recorder with no encoder says it is unavailable rather
//! than pretending, the strip offers nothing then and starts nothing when
//! asked, and the page shows the send button rather than a microphone
//! that could not work. That the platform's encoders exist is a device
//! question; docs/HARBOUR.md lists it under what to try on a phone.
//!
//! So is the limit: the longest recording the relay takes follows from
//! the limit and the media quality the strip is handed, and the strip
//! says it beside the time.

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
        function progress(elapsed, longest) {
            return '' + loader.item.progress(elapsed, longest)
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_machine_that_cannot_record_is_told_so_and_records_nothing() {
    // SAFETY: single-threaded test binary; set before Qt starts.
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
        // The real core's recommendation, (30 - 1) MiB * 3/4, at both
        // bit rates.
        (*steps_ptr).push(("limit-balanced", call!("limit", 22_806_528.0_f64, 0_i32)));
        (*steps_ptr).push(("limit-less-data", call!("limit", 22_806_528.0_f64, 1_i32)));
        (*steps_ptr).push(("progress", call!("progress", 7000_i32, 5_697_536_i32)));
        (*steps_ptr).push((
            "progress-past",
            call!("progress", 5_699_000_i32, 5_697_536_i32),
        ));
        (*steps_ptr).push(("progress-open", call!("progress", 7000_i32, 0_i32)));
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
    // No multimedia backend is installed on the test runner, and the
    // recorder has to say so rather than offer a microphone that records
    // nothing.
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

    // No limit known yet: no end but the reader's.
    assert_eq!(value("limit-unknown"), "0", "{context}");
    // At 32 kbit/s every second is 4000 bytes, and at 24 it is 3000: the
    // limit less a little headroom, as milliseconds of MP3.
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
    assert_eq!(value("progress"), "0:07 / 94:57", "{context}");
    assert_eq!(
        value("progress-past"),
        "94:57 / 94:57",
        "the time ran past the limit while the recorder stopped. {context}"
    );
    assert_eq!(value("progress-open"), "0:07", "{context}");
}
