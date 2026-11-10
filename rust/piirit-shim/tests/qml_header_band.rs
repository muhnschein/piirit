//! Ambience band behind a page's top block: hard-edged, from page's top to block's last line
//! plus header-to-picture gap (same gap above picture and below last line), none if unset.

// Qt harness: see qml_components.rs.
#![allow(unsafe_code, unused_unsafe, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        Loader { id: loader; width: 540 }
        function load(url) {
            loader.setSource(url, { headerBottom: 110, pictureTop: 140, lastBottom: 420 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadBare(url) {
            loader.setSource(url, { lastBottom: 200 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function band() {
            var item = loader.item
            return [item.y, item.height, item.width, item.color.a.toFixed(2)].join(',')
        }
    }
";

#[test]
fn the_band_leaves_the_same_gap_below_the_block_as_above_the_picture() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));
    let mut call = |name: &str, args: &[QVariant]| -> String {
        QString::from_qvariant(engine.invoke_method(name.into(), args))
            .map(|value| value.to_string())
            .unwrap_or_default()
    };
    let url = QString::from(common::component_url("HeaderBand.qml"));
    assert_eq!(call("load", &[url.into()]), "ok", "the band did not load");
    // Header ends 110, picture starts 140: 30 below last line (420), so 0 to 450.
    assert_eq!(
        call("band", &[]),
        "0,450,540,0.12",
        "the band does not run from the page's top to the same gap below the last line, \
         full width, faint"
    );
    let url = QString::from(common::component_url("HeaderBand.qml"));
    assert_eq!(
        call("loadBare", &[url.into()]),
        "ok",
        "the band did not load"
    );
    assert_eq!(
        call("band", &[]),
        "0,200,540,0.12",
        "without a picture gap the band does not end at the block's last line"
    );
}
