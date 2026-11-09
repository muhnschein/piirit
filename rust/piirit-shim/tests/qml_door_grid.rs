//! Rows to what a chat holds, on contact and group page: two to a line while every name fits half
//! a line; one per line once one does not (long word, big text), so no name is ever cut. A tap
//! names its kind.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        width: 540
        property string log: ''
        Loader { id: loader }
        Connections {
            target: loader.item
            ignoreUnknownSignals: true
            onOpened: log += kind + '|'
        }
        function load(url, width, kinds) {
            loader.setSource(url, { width: width, kinds: kinds })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function row(kind) {
            var kids = loader.item.children
            for (var i = 0; i < kids.length; i++) {
                if (kids[i].objectName === kind + 'Row') { return kids[i] }
            }
            return null
        }
        // Columns, height, then each row's x,y,width.
        function shape(kinds) {
            var out = loader.item.columns + ' ' + loader.item.height
            for (var i = 0; i < kinds.length; i++) {
                var door = row(kinds[i])
                out += door ? ' ' + door.x + ',' + door.y + ',' + door.width : ' missing'
            }
            return out
        }
        function tap(kind) { row(kind).clicked(); var was = log; log = ''; return was }
    }
";

#[test]
fn rows_pair_up_only_while_every_name_fits() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));
    let url = common::component_url("DoorGrid.qml");
    let call = |engine: &mut QmlEngine, name: &str, args: &[QVariant]| {
        QString::from_qvariant(engine.invoke_method(name.into(), args))
            .map(|value| value.to_string())
            .unwrap_or_default()
    };
    let kinds = |names: &[&str]| -> QVariant {
        names
            .iter()
            .map(|name| QVariant::from(QString::from(*name)))
            .collect::<QVariantList>()
            .into()
    };
    let six = ["gallery", "audio", "files", "apps", "calls", "map"];
    let five = ["gallery", "audio", "files", "apps", "map"];
    let load = |engine: &mut QmlEngine, width: i32, names: &[&str]| {
        let loaded = call(
            engine,
            "load",
            &[
                QString::from(url.as_str()).into(),
                width.into(),
                kinds(names),
            ],
        );
        format!("{loaded} {}", call(engine, "shape", &[kinds(names)]))
    };

    // Stub theme: margin 24, icon 64, gaps 16 and 8, row 60. Half of 540 leaves 158 for a name.
    let wide = load(&mut engine, 540, &six);
    let odd = load(&mut engine, 540, &five);
    let tapped = call(&mut engine, "tap", &[QString::from("files").into()]);
    // Half of 240 leaves 8: no name fits, so one per line.
    let narrow = load(&mut engine, 240, &six);
    assert_eq!(
        wide, "ok 2 180 0,0,270 270,0,270 0,60,270 270,60,270 0,120,270 270,120,270",
        "six rows at full width are not three lines of two"
    );
    assert_eq!(
        odd, "ok 2 180 0,0,270 270,0,270 0,60,270 270,60,270 0,120,270",
        "five rows do not end in a line of one"
    );
    assert_eq!(tapped, "files|", "a tap on Files does not open files");
    assert_eq!(
        narrow, "ok 1 360 0,0,240 0,60,240 0,120,240 0,180,240 0,240,240 0,300,240",
        "names that do not fit half a line still pair up"
    );
}
