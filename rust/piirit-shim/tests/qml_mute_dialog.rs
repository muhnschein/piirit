//! Page asking how long to mute a chat.
//!
//! Reference clients' five lengths in their order, each saying only how long; no title repeating
//! accept's "Mute". Nothing preset: accept does nothing until one is picked. One at a time; tap again
//! clears. Accept hands over seconds, -1 for forever. Note on mentions only when caller says they
//! still notify. Menus calling it: `qml_mute_flow.rs`.

// Qt harness: see qml_pages.rs.
#![allow(clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property var picked: []
        Loader { id: loader }
        function load(url, mentions) {
            loader.setSource('', {})
            loader.setSource(url, { mentionsStillNotify: mentions })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.picked.connect(function(seconds) { picked.push(seconds) })
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
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// Each option as `seconds=checked`, in the order shown.
        function options() {
            var out = []
            var choices = loader.item.choices
            for (var i = 0; i < choices.length; i++) {
                out.push(choices[i] + '=' + get('muteOption' + choices[i], 'checked'))
            }
            return out.join(',')
        }
        function labels() {
            var out = []
            var choices = loader.item.choices
            for (var i = 0; i < choices.length; i++) {
                out.push(get('muteOption' + choices[i], 'text'))
            }
            return out.join('|')
        }
        function canAccept() { return '' + loader.item.canAccept }
        function accept() { loader.item.accept(); return picked.join(',') }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_dialog_offers_the_reference_lengths_and_hands_over_the_one_picked() {
    // SAFETY: single-threaded; set pre-Qt.
    #[allow(unsafe_code, unused_unsafe)]
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    // Each step synchronous: no event loop needed.
    let mut call = |name: &str, args: &[QVariant]| -> String {
        QString::from_qvariant(engine.invoke_method(name.into(), args))
            .map(|value| value.to_string())
            .unwrap_or_default()
    };
    let option = |name: &str| -> [QVariant; 1] { [QString::from(name).into()] };

    let url = || -> QVariant { QString::from(common::page_url("MuteDialog.qml")).into() };
    let load = call("load", &[url(), false.into()]);
    assert_eq!(load, "ok", "the dialog did not load");
    assert_eq!(
        (
            call(
                "get",
                &[
                    QString::from("muteHeader").into(),
                    QString::from("title").into()
                ]
            ),
            call(
                "get",
                &[
                    QString::from("muteHeader").into(),
                    QString::from("acceptText").into()
                ]
            ),
        ),
        (String::new(), "Mute".to_string()),
        "the dialog repeats its accept text as a title"
    );
    assert_eq!(
        call(
            "get",
            &[
                QString::from("mentionsNote").into(),
                QString::from("visible").into()
            ]
        ),
        "false",
        "the mentions note shows though the caller said nothing still notifies"
    );

    assert_eq!(
        call("labels", &[]),
        "For 1 hour|For 8 hours|For 1 day|For 7 days|Forever",
        "the dialog does not offer the reference clients' lengths, in their \
         order, saying only how long"
    );
    assert_eq!(
        call("options", &[]),
        "3600=false,28800=false,86400=false,604800=false,-1=false",
        "a length is preset: a blind swipe would mute"
    );
    assert_eq!(
        (call("canAccept", &[]), call("accept", &[])),
        ("false".to_string(), String::new()),
        "the dialog accepts with nothing picked"
    );

    assert_eq!(call("click", &option("muteOption28800")), "ok");
    assert_eq!(
        (call("options", &[]), call("canAccept", &[])),
        (
            "3600=false,28800=true,86400=false,604800=false,-1=false".to_string(),
            "true".to_string()
        ),
        "picking 8 hours did not mark it alone, ready to accept"
    );
    // Same one again: back to nothing.
    assert_eq!(call("click", &option("muteOption28800")), "ok");
    assert_eq!(
        (call("options", &[]), call("canAccept", &[])),
        (
            "3600=false,28800=false,86400=false,604800=false,-1=false".to_string(),
            "false".to_string()
        ),
        "a second tap did not clear the pick"
    );
    // Another one moves the mark.
    assert_eq!(call("click", &option("muteOption86400")), "ok");
    assert_eq!(call("click", &option("muteOption-1")), "ok");
    assert_eq!(
        call("options", &[]),
        "3600=false,28800=false,86400=false,604800=false,-1=true",
        "picking another length left the first marked"
    );
    assert_eq!(
        call("accept", &[]),
        "-1",
        "accepting forever did not hand over -1"
    );
    // Stub accepts again; real one once per push. Another length: its own seconds.
    assert_eq!(call("click", &option("muteOption604800")), "ok");
    assert_eq!(
        call("accept", &[]),
        "-1,604800",
        "accepting 7 days did not hand over its seconds"
    );

    // Group with mentions on: note says what still gets through.
    assert_eq!(call("load", &[url(), true.into()]), "ok");
    assert_eq!(
        call(
            "get",
            &[
                QString::from("mentionsNote").into(),
                QString::from("visible").into()
            ]
        ),
        "true",
        "the mentions note is missing though replies still notify"
    );
}
