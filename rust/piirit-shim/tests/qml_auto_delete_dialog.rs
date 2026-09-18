//! The page that asks before old messages start going.
//!
//! It says how many go now and for what period, and accept means nothing
//! until the reader has turned the switch that says they understand --
//! the checkbox the reference clients put on the same question. What it
//! does when accepted is the settings page's business (`qml_general_
//! settings.rs`); this is the page on its own.

// Qt harness: see qml_pages.rs.
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
        property int accepted: 0
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { seconds: 3600, periodLabel: 'After 1 hour', count: 4 })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.accepted.connect(function() { accepted += 1 })
            return 'ok'
        }
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function canAccept() { return '' + loader.item.canAccept }
        // The stub switch does not flip itself on a tap, as Silica's
        // does; the reader's tap is the flipped value.
        function confirm() {
            var item = findIn(loader.item, 'confirmSwitch')
            if (!item) { return 'missing' }
            item.checked = true
            return 'ok'
        }
        function accept() { loader.item.accept(); return '' + accepted }
    }
";

#[test]
fn the_dialog_says_what_goes_and_accepts_only_once_the_reader_has_agreed() {
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("AutoDeleteDialog.qml"))
            )
        );
        record!(
            "period",
            call!("get", QString::from("periodLabel"), QString::from("text"))
        );
        record!(
            "count",
            call!("get", QString::from("countLabel"), QString::from("text"))
        );
        record!("before", call!("canAccept"));
        // Accept before agreeing does nothing.
        record!("accepted-early", call!("accept"));
        record!("confirm", call!("confirm"));
        record!("after", call!("canAccept"));
        record!("accepted", call!("accept"));
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

    assert_eq!(value("load"), "ok", "the dialog did not load. {context}");
    assert_eq!(
        value("period"),
        "After 1 hour",
        "the dialog does not say which period was picked. {context}"
    );
    assert!(
        value("count").contains('4'),
        "the dialog does not say how many messages go now, got {:?}. {context}",
        value("count")
    );
    assert_eq!(
        value("before"),
        "false",
        "the dialog can be accepted before the reader has agreed. {context}"
    );
    assert_eq!(
        value("accepted-early"),
        "0",
        "accepting before agreeing counted. {context}"
    );
    assert_eq!(
        value("after"),
        "true",
        "the dialog cannot be accepted once the reader has agreed. {context}"
    );
    assert_eq!(
        value("accepted"),
        "1",
        "accepting after agreeing did not say so. {context}"
    );
}
