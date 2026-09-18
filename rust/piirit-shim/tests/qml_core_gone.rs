//! What the chat list says while the core is being restarted.
//!
//! The banner has to read the core's state rather than wait for it to
//! change: a page opened while the core is away never saw the transition a
//! handler would have caught, and would otherwise look like an account with
//! no history in it.
//!
//! The server here dies 300ms after every spawn, so the app spends most of
//! its time between attempts rather than connected -- the backoff widens
//! from one second to two to four, which is what makes the window the reads
//! below land in a wide one rather than a race.

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
    Item {
        Loader { id: loader }
        function load(url, accountId) {
            // Cleared first, so loading the same page again really is a new
            // instance rather than the one already there.
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
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
    }
";

#[test]
fn a_core_being_restarted_says_so_on_every_page_that_opens() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-core-gone-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Every server, including each replacement. By the third failure the
        // app waits four seconds before trying again, and the reads below
        // fall inside that.
        std::env::set_var("PIIRIT_FAKE_EXIT_AFTER_MS", "300");
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

    macro_rules! probe {
        ($label:expr, $name:expr, $property:expr) => {
            (*steps_ptr).push((
                $label,
                call!("get", QString::from($name), QString::from($property)),
            ))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1
            ),
        ));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        probe!("away-shown", "errorLabel", "text");
        probe!("away-timeout", "errorBanner", "timeout");
        // A page opened while the core is away never saw it go.
        (*steps_ptr).push((
            "reopen",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1
            ),
        ));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        probe!("reopened-shown", "errorLabel", "text");
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// What the run has to show for itself, out of the test body: what a Qt
/// harness can do in one function is bounded, and the assertions are the
/// part worth reading.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert!(
        value("away-shown").contains("Reconnecting"),
        "the core went away and the page said nothing about it. {context}"
    );
    assert!(
        !value("away-shown").contains("Restart"),
        "the page tells the reader to restart the app while it is already \
         starting the core again. {context}"
    );
    assert_eq!(
        value("away-timeout"),
        "0",
        "a message about a core that is not there must not time out from \
         under the reader. {context}"
    );
    assert_eq!(
        value("reopen"),
        "ok",
        "the page did not load a second time. {context}"
    );
    assert!(
        value("reopened-shown").contains("Reconnecting"),
        "a page opened while the core was away says nothing about it, so an \
         account with history looks like an empty one. {context}"
    );
}
