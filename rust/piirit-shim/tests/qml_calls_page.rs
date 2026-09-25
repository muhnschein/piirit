//! The calls with one contact, behind the Calls tile on their page: the
//! chat's calls, newest first, each saying which way it went and what
//! became of it, and a first row to call them from.
//!
//! Against the fake core, with a call ringing in from the contact before
//! the page is opened. What is checked is the wiring: the tile is there
//! only while calls are on, the page lists the call the chat holds in the
//! chat's own words, the row to call from names the contact as plain
//! text and places a call through the window, and a tap on a call still
//! ringing takes it up rather than calling back.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    Item {
        id: probe
        property string asked: ''

        // The window, as the page reaches it: what it was asked to do.
        QtObject {
            id: appWindow
            function placeCall(accountId, chatId) {
                probe.asked += 'place:' + accountId + ':' + chatId + ';'
                return true
            }
            function showCall() { probe.asked += 'show;' }
            function pickUpCall(accountId, chatId, messageId) {
                probe.asked += 'pickUp:' + accountId + ':' + chatId + ':' + messageId + ';'
            }
        }

        Loader { id: tiles; width: 540 }
        Loader { id: holder; width: 540; height: 960 }

        function setSetting(dir, name, value) {
            var writer = Qt.createQmlObject('import QtQuick 2.0; import "' + dir + '"; '
                + 'QtObject { function set(n, v) { Settings[n] = v } }', probe)
            writer.set(name, value)
            return 'ok'
        }
        function findIn(node, test) {
            if (!node) { return null }
            if (test(node)) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], test)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, test)
            }
            return null
        }
        function named(root, name) {
            return findIn(root, function (node) { return node.objectName === name })
        }
        function tileKinds(url, calls) {
            tiles.setSource(url, { callsAvailable: calls })
            return tiles.item.kinds.join(',')
        }
        function load(url) {
            holder.setSource(url, { accountId: 1, chatId: 1, contactName: 'Ada <b>Lovelace</b>' })
            return holder.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function get(name, property) {
            var hit = named(holder.item, name)
            return hit ? '' + hit[property] : 'missing:' + name
        }
        function tap(name) {
            var hit = named(holder.item, name)
            if (!hit) { return 'missing:' + name }
            hit.clicked()
            return 'ok'
        }
        // The first call's row: what it says, and a tap on it.
        function firstRow() {
            return findIn(holder.item, function (node) {
                return node.objectName.indexOf('callRow') === 0
            })
        }
        function rowSays() {
            var row = firstRow()
            if (!row) { return 'no row' }
            return named(row, 'callTitle').text + '|' + named(row, 'callDetail').text
                   + '|' + (named(row, 'callWhen').text.length > 0)
        }
        function tapRow() {
            var row = firstRow()
            if (!row) { return 'no row' }
            row.clicked()
            return asked
        }
    }
"#;

#[test]
#[allow(clippy::too_many_lines)]
fn a_contacts_calls_are_listed_and_called_from_their_own_page() {
    let temp = std::env::temp_dir().join(format!("piirit-calls-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_INCOMING_CALL_MS", "300");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        let tiles = common::component_url("MediaKinds.qml");
        record!(
            "tiles-off",
            call!("tileKinds", QString::from(tiles.clone()), false)
        );
        record!(
            "tiles-on",
            call!("tileKinds", QString::from(tiles.clone()), true)
        );
        let dir = tiles.trim_end_matches("MediaKinds.qml").to_string();
        call!(
            "setSetting",
            QString::from(dir),
            QString::from("callsEnabled"),
            true
        );
        record!(
            "load",
            call!("load", QString::from(common::page_url("CallsPage.qml")))
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "title",
            call!("get", QString::from("callList"), QString::from("count"))
        );
        record!("row", call!("rowSays"));
        record!(
            "action",
            call!(
                "get",
                QString::from("callActionLabel"),
                QString::from("text")
            )
        );
        record!(
            "action-format",
            call!(
                "get",
                QString::from("callActionLabel"),
                QString::from("textFormat")
            )
        );
        record!(
            "action-enabled",
            call!("get", QString::from("callAction"), QString::from("enabled"))
        );
        record!("call", call!("tap", QString::from("callAction")));
        record!("after-call", call!("tapRow"));
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
        value("tiles-off"),
        "gallery,audio,files",
        "a Calls tile stands where calls are not on. {context}"
    );
    assert_eq!(
        value("tiles-on"),
        "gallery,audio,files,calls",
        "no Calls tile where calls are on. {context}"
    );
    assert_eq!(
        value("load"),
        "ok",
        "the Calls page did not load. {context}"
    );
    assert_eq!(
        value("title"),
        "1",
        "the chat's one call is not listed. {context}"
    );
    let row = value("row");
    assert!(
        row.starts_with("Incoming call|Ringing…|true"),
        "the call's row does not say which way it went, how it stands, \
         and when: {row}. {context}"
    );
    assert_eq!(
        value("action"),
        "Call Ada <b>Lovelace</b>",
        "the row to call from does not name the contact. {context}"
    );
    assert_eq!(
        value("action-format"),
        "0",
        "the contact's name is drawn as markup. {context}"
    );
    assert_eq!(
        value("action-enabled"),
        "true",
        "a contact a call can be placed with cannot be called. {context}"
    );
    assert_eq!(value("call"), "ok", "{context}");
    let asked = value("after-call");
    assert!(
        asked.starts_with("place:1:1;") && asked.contains(";pickUp:1:1:"),
        "calling placed no call, or a tap on a call still ringing did not \
         take it up: {asked}. {context}"
    );
}
