//! A relay given up on that answers after all.
//!
//! Cancel stops the core's process on the profile and stops waiting for
//! the answer. A relay that answers anyway -- the double's `deaf` relay,
//! which is what a stop that came too late looks like -- has added
//! itself to the profile, and the profile is the reader's: the relay is
//! kept, for the profile page to list and its remove to take off again,
//! rather than the profile being removed for it as a cancelled signup's
//! is. Nothing is signalled for it either way.

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

/// The page, and beside it the profile's relays as the shim lists them,
/// and a count of what the shim signalled.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        id: probe

        // Silica's `pageStack`, recorded rather than performed. In QML
        // rather than as a QObject on the Rust side, since a page loaded
        // by a Loader reads `pageStack` off the file the Loader was
        // declared in.
        property QtObject pageStack: QtObject {
            property string log: ''
            function pop() { log = log + 'pop|' }
        }

        Loader { id: loader }
        Transports { id: transports; account_id: 1 }
        property int added: 0
        property string errors: ''
        Connections {
            target: core
            onRelay_added: added = added + 1
            onRelay_error: errors = errors + message + '|'
        }
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
        // Made before the core was up, the list has to be asked for.
        function refresh() { transports.reload(); return 'ok' }
        function listed() { return transports.count + ':' + transports.primary }
        function signalled() { return added + '/' + errors }
        function navigation() { return probe.pageStack.log }
        // No profile to add to: refused here, before the core is asked.
        function nowhere() { core.add_relay(0, 'dcaccount:nine.testrun.org'); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_relay_added_after_cancel_is_kept_and_not_announced() {
    let temp = std::env::temp_dir().join(format!("piirit-relay-late-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        // Three seconds, then an answer whatever was said meanwhile.
        std::env::set_var("PIIRIT_FAKE_SLOW_MS", "3000");
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("AddRelayPage.qml")),
                1
            )
        );
        record!("refresh", call!("refresh"));
    });

    // 2s: the deaf relay asked, and given up on at once.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("before", call!("listed"));
        record!(
            "typed",
            call!(
                "setText",
                QString::from("customField"),
                QString::from("slow.deaf.example")
            )
        );
        record!("add", call!("click", QString::from("addButton")));
        record!("busy", call!("pageProperty", QString::from("busy")));
        record!("cancel", call!("click", QString::from("cancelButton")));
        record!("cancelled", call!("pageProperty", QString::from("busy")));
        record!("nowhere", call!("nowhere"));
    });

    // 7s: the relay answered at five, to nobody.
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("late-busy", call!("pageProperty", QString::from("busy")));
        record!(
            "late-error",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!("signalled", call!("signalled"));
        record!("reload", call!("refresh"));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("after", call!("listed"));
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
    assert_eq!(value("before"), "1:account1@example.org", "{context}");
    assert_eq!(value("add"), "ok", "{context}");
    assert_eq!(value("busy"), "true", "{context}");
    assert_eq!(value("cancel"), "ok", "{context}");
    assert_eq!(
        value("cancelled"),
        "false",
        "Cancel left the page waiting. {context}"
    );
    assert!(
        calls
            .iter()
            .any(|(name, params)| name == "stop_ongoing_process"
                && params.pointer("/0").and_then(serde_json::Value::as_u64) == Some(1)),
        "Cancel did not stop the core's process on the profile. {context}"
    );
    assert_eq!(value("late-busy"), "false", "{context}");
    assert_eq!(
        value("late-error"),
        "",
        "an answer nobody was waiting for reached the page. {context}"
    );
    assert_eq!(
        value("signalled"),
        "0/no profile to add a relay to|",
        "the late answer was announced, or the refusal for no profile was \
         not. {context}"
    );
    assert!(
        !calls.iter().any(|(name, _)| name == "remove_account"),
        "a relay answering after Cancel cost the reader the profile. {context}"
    );
    assert_eq!(
        value("after"),
        "2:account1@example.org",
        "the relay the late answer added is not on the profile, or the \
         profile's own address moved. {context}"
    );
    assert_eq!(
        value("navigation"),
        "",
        "the page went away without a relay having been added while it \
         waited. {context}"
    );
}
