//! A message whose download has not finished offers to try again, and
//! says when the server has nothing to give.
//!
//! Issue 107: the row said "Downloading…" and took no tap, for as long as
//! the core left the message `InProgress` -- for good, when the rest of
//! it never reached the server. `download_stuck.rs` is the shim's half:
//! what a tap asks of the core, and where `downloadMissing` comes from.

// Qt harness: see qml_conversation.rs.
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
        property int asked: 0
        Loader { id: loader }
        Connections {
            target: loader.item
            ignoreUnknownSignals: true
            onDownloadRequested: asked += 1
        }
        function load(url) {
            loader.setSource(url, { width: 540, messageText: '[Video - 55.04 MiB]' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The label's own tap, the way a finger reaches it.
        function tapLabel() {
            var label = findIn(loader.item, 'downloadButton')
            if (!label) { return 'missing:downloadButton' }
            var area = label.children[0]
            if (!area.enabled) { return 'disabled' }
            area.clicked(null)
            return 'ok'
        }
        // The bubble's tap, which a message with nothing to open passes
        // on to the download.
        function tapRow() { loader.item.tapped(); return 'ok' }
        function count() { return '' + asked }
    }
";

#[test]
fn a_download_under_way_can_be_tapped_and_says_when_nothing_is_there() {
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
    macro_rules! set {
        ($property:expr, $value:expr) => {
            call!("set", QString::from($property), $value)
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
        set!("downloadState", QString::from("InProgress"));
        record!(
            "under-way-text",
            call!(
                "get",
                QString::from("downloadButton"),
                QString::from("text")
            )
        );
        record!("label-tap", call!("tapLabel"));
        record!("row-tap", call!("tapRow"));
        record!("asked", call!("count"));

        set!("downloadMissing", true);
        record!(
            "missing-text",
            call!(
                "get",
                QString::from("downloadButton"),
                QString::from("text")
            )
        );
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
    assert_eq!(value("load"), "ok", "the row did not load. {context}");
    assert!(
        value("under-way-text").contains("try again"),
        "a download under way does not say it can be tried again. {context}"
    );
    assert_eq!(
        value("label-tap"),
        "ok",
        "a download under way takes no tap, so one that stalled offers \
         nothing at all. {context}"
    );
    assert_eq!(
        value("asked"),
        "2",
        "tapping a download under way did not ask for it again. {context}"
    );
    assert!(
        value("missing-text").contains("Not on the server"),
        "a download with nothing on the server reads as one under way. {context}"
    );
    assert!(
        value("missing-text").contains("check again"),
        "a download with nothing on the server offers no way to look \
         again. {context}"
    );
}
