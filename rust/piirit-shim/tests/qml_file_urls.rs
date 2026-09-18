//! A file the other end named becomes a URL through per-segment encoding.
//!
//! `encodeURI` leaves `#` and `?` alone -- to it they are URL syntax --
//! so a blob called `a#b.png` pointed at `a`, and whatever the core wrote
//! the file as never loaded. The path is built one segment at a time with
//! `encodeURIComponent`, which encodes everything that is not a slash.
//! Measured on the two components that build one, not just scanned for.

// Qt harness: see qml_avatar.rs.
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
        function load(url, property, value) {
            loader.setSource('', {})
            var initial = { width: 540 }
            initial[property] = value
            loader.setSource(url, initial)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        function rootProperty(property) { return '' + loader.item[property] }
    }
";

/// Every character a sender could put in a name that a URL would read as
/// syntax: a percent, a space, a fragment, a query.
const AWKWARD: &str = "/home/u/.local/share/piirit/blobs/100% sure#1?.png";

#[test]
fn a_file_url_encodes_every_character_that_is_not_a_slash() {
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
            "attachment-load",
            call!(
                "load",
                QString::from(common::component_url("AttachmentPreview.qml")),
                QString::from("filePath"),
                QString::from(AWKWARD)
            )
        );
        record!(
            "attachment-url",
            call!("rootProperty", QString::from("fileUrl"))
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "avatar-load",
            call!(
                "load",
                QString::from(common::component_url("Avatar.qml")),
                QString::from("picturePath"),
                QString::from(AWKWARD)
            )
        );
        record!(
            "avatar-url",
            call!("get", QString::from("avatarImage"), QString::from("source"))
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
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

    for (component, url) in [
        ("AttachmentPreview", value("attachment-url")),
        ("Avatar", value("avatar-url")),
    ] {
        assert_eq!(
            value(&format!(
                "{}-load",
                component.to_lowercase().replace("preview", "")
            )),
            "ok",
            "{component} did not load. {context}"
        );
        let path = url
            .strip_prefix("file:///")
            .unwrap_or_else(|| panic!("{component} made a URL that is not file:///: {url:?}"));
        // What a URL reads as syntax is what the file name must not be
        // allowed to say. Qt's own string form may decode a space back,
        // so the reserved characters are the assertion.
        assert!(
            !path.contains('#') && !path.contains('?'),
            "{component} left '#' or '?' in the path, so the URL points at a \
             different file: {url}"
        );
        assert!(
            path.contains("100%25") && path.contains("%231%3F.png"),
            "{component} did not encode the name's percent, fragment and \
             query characters: {url}"
        );
        assert!(
            path.contains("/blobs/"),
            "{component} encoded the slashes too, so the path is one segment: {url}"
        );
    }
}
