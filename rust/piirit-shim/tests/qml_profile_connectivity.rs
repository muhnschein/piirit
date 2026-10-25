//! Profile page re-reads connection once a `ConnectivityChanged` burst passes.
//!
//! A read is four round trips: band, core report, on-phone profile size (sizes every
//! file), address. Core reports several changes per fetch. Page reads on open, then at
//! most once a second. `get_account_file_size` is the one counted: most expensive, and only
//! this read asks for it.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        // Silica's `pageStack`, for the page to find; nothing here
        // pushes anything. See relays.rs.
        property QtObject pageStack: QtObject {
            function push(page, properties) {}
            function pop() {}
        }
        Loader { id: loader }
        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // What the core says while it fetches: the connection changed,
        // again and again. And once for another profile, which this
        // page is not about.
        function burst() {
            for (var i = 0; i < 5; i++) {
                core.core_event(1, 'ConnectivityChanged', '{}')
            }
            core.core_event(2, 'ConnectivityChanged', '{}')
            return 'ok'
        }
    }
";

/// Times profile's on-phone size was asked for.
fn sizings(journal: &std::path::Path) -> usize {
    common::methods(journal)
        .iter()
        .filter(|method| *method == "get_account_file_size")
        .count()
}

#[test]
fn a_burst_of_connection_changes_is_one_read() {
    let temp = std::env::temp_dir().join(format!(
        "piirit-profile-connectivity-{}",
        std::process::id()
    ));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
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

    let steps: common::Steps = common::Steps::default();

    let mut plan = common::Plan::default();

    // SAFETY: callbacks run only inside `exec()` on this thread; all they point at outlives it.
    let loading = steps.clone();
    plan.after(1, move || unsafe {
        common::record(
            &loading,
            "load",
            call!(
                "load",
                QString::from(common::page_url("ProfilePage.qml")),
                1
            )
            .into(),
        );
    });

    let bursting = steps.clone();
    let opened = journal.clone();
    plan.after(2, move || unsafe {
        common::record(&bursting, "on-open", sizings(&opened).to_string().into());
        common::record(&bursting, "burst", call!("burst").into());
    });

    // Long enough after burst for the owed read to return.
    let settled = steps.clone();
    let after = journal.clone();
    plan.after(3, move || unsafe {
        common::record(&settled, "after-burst", sizings(&after).to_string().into());
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    let methods = common::methods(&journal);
    assert_eq!(
        common::value_of(&steps, "load"),
        "ok",
        "the profile page did not load: {steps:?}"
    );
    assert_eq!(
        common::value_of(&steps, "burst"),
        "ok",
        "the burst was not sent: {steps:?}"
    );
    assert_eq!(
        common::value_of(&steps, "on-open"),
        "1",
        "the page did not read the connection once, straight away, as it \
         opened: {steps:?}, calls: {methods:?}"
    );
    assert_eq!(
        common::value_of(&steps, "after-burst"),
        "2",
        "five `ConnectivityChanged` for this profile, and one for another, \
         should be one more read: {steps:?}, calls: {methods:?}"
    );
}
