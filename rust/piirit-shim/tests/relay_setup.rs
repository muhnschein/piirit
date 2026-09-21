//! The page that adds a relay, against a relay that keeps the reader
//! waiting.
//!
//! `relays.rs` drives it against a relay that answers at once. This is
//! the other case, against the double's `slow` relay: the hint about
//! volunteers' relays appears under Cancel at the fourth second and not
//! before; at the deadline the page shows the time-out with the relay's
//! name and the fields back; a second attempt while the core is still on
//! the first is refused in the core's own words, since a profile has one
//! ongoing process at a time; and a relay that cannot be reached is
//! refused in the core's words too.

// Qt harness: see qml_chat_list.rs.
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

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe

        // Silica's `pageStack`, recorded rather than performed. The page
        // pops on success only, which nothing here reaches. In QML rather
        // than as a QObject on the Rust side, since a page loaded by a
        // Loader reads `pageStack` off the file the Loader was declared
        // in.
        property QtObject pageStack: QtObject {
            property string log: ''
            function pop() { log = log + 'pop|' }
        }

        Loader { id: loader }
        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
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
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function pageProperty(property) { return '' + loader.item[property] }
        function navigation() { return probe.pageStack.log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_slow_relay_is_explained_and_given_up_on() {
    let temp = std::env::temp_dir().join(format!("piirit-relay-setup-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        // A relay that does not answer while anyone here is watching.
        std::env::set_var("PIIRIT_FAKE_SLOW_MS", "20000");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    // Six seconds rather than the built-in thirty; the page is what is
    // under test, not the wait.
    core_box.pinned().borrow_mut().profile_timeout = 6;
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // 1s: a typed relay, and the core asked for it.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("AddRelayPage.qml")),
                1
            )
        );
        record!("dim", get!("addButton", "enabled"));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("customField"),
                QString::from(" slow.example ")
            )
        );
        record!("list-off", get!("relayCombo", "enabled"));
        record!("lit", get!("addButton", "enabled"));
        record!("add", call!("click", QString::from("addButton")));
        record!("busy", call!("pageProperty", QString::from("busy")));
        record!("field-held", get!("customField", "readOnly"));
        record!("add-hidden", get!("addButton", "visible"));
    });

    // 2s: waiting, with nothing said yet.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("early-hint", get!("slowHint", "visible"));
        record!("early-label", get!("progressBar", "label"));
        record!("early-cancel", get!("cancelButton", "visible"));
    });

    // 6s: the fourth second has passed.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("hint", get!("slowHint", "visible"));
        record!("still-busy", call!("pageProperty", QString::from("busy")));
    });

    // 8s: the deadline passed at seven.
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("late-busy", call!("pageProperty", QString::from("busy")));
        record!(
            "late-error",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!("late-hint", get!("slowHint", "visible"));
        record!("late-add", get!("addButton", "visible"));
        record!("late-field", get!("customField", "readOnly"));
        // A second attempt, while the core is still on the first.
        record!("again", call!("click", QString::from("addButton")));
    });

    // 10s: refused, since the core allows one process per profile; then
    // a relay that cannot be reached.
    single_shot(Duration::from_secs(10), move || unsafe {
        record!("again-busy", call!("pageProperty", QString::from("busy")));
        record!(
            "again-error",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!(
            "retyped",
            call!(
                "setText",
                QString::from("customField"),
                QString::from("fail.example")
            )
        );
        record!("unreachable", call!("click", QString::from("addButton")));
    });

    single_shot(Duration::from_secs(12), move || unsafe {
        record!(
            "unreachable-busy",
            call!("pageProperty", QString::from("busy"))
        );
        record!(
            "unreachable-error",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!("navigation", call!("navigation"));
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
    let calls = common::calls(&journal);
    let context = format!("steps: {steps:?}\ncalls: {calls:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("dim"),
        "false",
        "a relay can be added before one is chosen. {context}"
    );
    assert_eq!(value("typed"), "ok", "{context}");
    assert_eq!(
        value("list-off"),
        "false",
        "a typed server did not take over from the list. {context}"
    );
    assert_eq!(value("lit"), "true", "{context}");
    assert_eq!(value("add"), "ok", "{context}");
    assert_eq!(value("busy"), "true", "the core was not asked. {context}");
    assert_eq!(
        value("field-held"),
        "true",
        "the relay can be retyped while the core is on it. {context}"
    );
    assert_eq!(value("add-hidden"), "false", "{context}");
    assert_eq!(
        value("early-hint"),
        "false",
        "the hint about slow relays is up before the fourth second. {context}"
    );
    assert_eq!(
        value("early-label"),
        "Contacting slow.example...",
        "the progress bar does not name the relay, trimmed. {context}"
    );
    assert_eq!(value("early-cancel"), "true", "{context}");
    assert_eq!(
        value("hint"),
        "true",
        "the hint about slow relays never appeared. {context}"
    );
    assert_eq!(value("still-busy"), "true", "{context}");
    assert_eq!(
        value("late-busy"),
        "false",
        "the page is still waiting past the deadline. {context}"
    );
    assert_eq!(
        value("late-error"),
        "slow.example did not answer within 6 seconds.",
        "the time-out is not said with the relay's name and the seconds. {context}"
    );
    assert_eq!(
        value("late-hint"),
        "true",
        "the hint went away with the error it explains. {context}"
    );
    assert_eq!(value("late-add"), "true", "{context}");
    assert_eq!(value("late-field"), "false", "{context}");
    assert!(
        calls
            .iter()
            .any(|(name, params)| name == "stop_ongoing_process"
                && params.pointer("/0").and_then(serde_json::Value::as_u64) == Some(1)),
        "giving up did not stop the core's process on the profile. {context}"
    );
    assert_eq!(value("again"), "ok", "{context}");
    assert_eq!(value("again-busy"), "false", "{context}");
    // The core's words, behind the transport's own prefix, as every
    // page shows a refusal.
    assert!(
        value("again-error").ends_with("There is already another ongoing process running."),
        "a second attempt while the core is still on the first was not \
         refused in the core's words. {context}"
    );
    assert_eq!(value("unreachable"), "ok", "{context}");
    assert_eq!(value("unreachable-busy"), "false", "{context}");
    assert!(
        value("unreachable-error").ends_with("cannot resolve chatmail server"),
        "a relay that cannot be reached was not refused in the core's words. {context}"
    );
    assert_eq!(
        value("navigation"),
        "",
        "the page went away without a relay having been added. {context}"
    );
}
