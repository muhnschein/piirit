//! Someone's own words on a contact or group page: two lines until tapped, all after, two again on
//! a second tap. Short text: tap does nothing.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        width: 540
        Loader { id: loader; width: 540 }
        function load(url, text) {
            loader.setSource(url, { text: text })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function state() {
            var label = loader.item
            return label.lineCount + '/' + label.truncated
        }
        function tap() {
            var label = loader.item
            for (var i = 0; i < label.children.length; i++) {
                var kid = label.children[i]
                if (kid.objectName === 'expandTap') {
                    if (!kid.enabled) { return 'disabled' }
                    kid.clicked(null)
                    return 'ok'
                }
            }
            return 'missing'
        }
    }
";

#[test]
fn long_words_show_two_lines_until_tapped() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));
    let url = common::component_url("ClampedText.qml");
    let call = |engine: &mut QmlEngine, name: &str, args: &[QVariant]| {
        QString::from_qvariant(engine.invoke_method(name.into(), args))
            .map(|value| value.to_string())
            .unwrap_or_default()
    };

    let long = "Line one\nLine two\nLine three\nLine four\nLine five";
    let steps = [
        call(
            &mut engine,
            "load",
            &[
                QString::from(url.as_str()).into(),
                QString::from(long).into(),
            ],
        ),
        call(&mut engine, "state", &[]),
        call(&mut engine, "tap", &[]),
        call(&mut engine, "state", &[]),
        call(&mut engine, "tap", &[]),
        call(&mut engine, "state", &[]),
        call(
            &mut engine,
            "load",
            &[
                QString::from(url.as_str()).into(),
                QString::from("Short").into(),
            ],
        ),
        call(&mut engine, "tap", &[]),
    ];
    assert_eq!(
        steps,
        ["ok", "2/true", "ok", "5/false", "ok", "2/true", "ok", "disabled"].map(String::from),
        "long words are not clamped to two lines, a tap does not show them all and a second \
         put them back, or a short one can be tapped: load, clamped, tap, open, tap, closed, \
         short, tap"
    );
}
