//! The page that asks before a switch worth stopping for is turned on.
//!
//! It says the one thing worth stopping for: by default that an
//! experimental feature may not hold up, and otherwise what the switch
//! that opened it says -- a camera quick action's "Send right away",
//! that a chat tapped by mistake gets the photo too. What it does when
//! accepted is the opener's business (`qml_general_settings.rs`,
//! `qml_quick_actions_settings.rs`); this is the page on its own.

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
        Loader { id: loader }
        function load(url, properties) {
            loader.setSource('', {})
            loader.setSource(url, JSON.parse(properties))
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
            return null
        }
        function said() {
            var item = findIn(loader.item, 'experimentalText')
            return item ? '' + item.text : 'missing'
        }
    }
";

#[test]
fn the_dialog_says_the_one_thing_worth_stopping_for() {
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

    let url = common::page_url("ExperimentalFeatureDialog.qml");
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "experimental",
            call!(
                "load",
                QString::from(url.as_str()),
                QString::from(r#"{"title": "Enable webxdc apps (experimental)"}"#)
            )
        );
        record!("experimental-says", call!("said"));
        record!(
            "own",
            call!(
                "load",
                QString::from(url.as_str()),
                QString::from(
                    r#"{"title": "Send right away", "text": "A chat tapped by mistake gets it."}"#
                )
            )
        );
        record!("own-says", call!("said"));
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

    assert_eq!(
        (value("experimental").as_str(), value("own").as_str()),
        ("ok", "ok"),
        "the dialog did not load. {context}"
    );
    assert_eq!(
        value("experimental-says"),
        "This feature may be unstable and may be changed or removed.",
        "an experimental feature's switch is no longer warned about. {context}"
    );
    assert_eq!(
        value("own-says"),
        "A chat tapped by mistake gets it.",
        "the dialog does not say what the switch that opened it asked it to. \
         {context}"
    );
}
