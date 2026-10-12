//! The core rings for contacts, as it does by default, until the reader
//! switches ringing off -- and only that tells it "nobody".
//!
//! `who_can_call_me` is per device and never synced, but it lives in the
//! profile's database, and a backup or a second device takes that
//! database whole. The window once wrote "nobody" for every profile
//! whenever calls were off, as they were by default while they were
//! experimental, so a Delta Chat set up from a Piirit profile never rang
//! for anyone without its reader having chosen that anywhere. Calls are
//! not a setting any more (#119); ringing still is.

// Qt harness: see qml_share.rs.
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
use serde_json::json;

mod common;

/// Silica's `pageStack`; see `network_hint.rs`. Nothing here navigates.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
}

/// The window, with the components directory imported so the probe can
/// write the settings.
fn probe_qml(components: &std::path::Path) -> String {
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: window }
        function loadWindow(url) {
            window.setSource(url, {})
            return window.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // What every page that lists the profiles does, and what writes
        // the settings to each profile the core has.
        function refresh() { core.refresh_accounts(); return 'ok' }
        function ring(on) { Settings.callsRing = on === 'true'; return 'ok' }
    }
";

/// Every value `who_can_call_me` was set to, in order, one per account.
fn written(journal: &std::path::Path) -> Vec<String> {
    common::calls(journal)
        .into_iter()
        .filter(|(name, params)| name == "set_config" && params[1] == json!("who_can_call_me"))
        .map(|(_, params)| params[2].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn the_core_rings_for_contacts_until_the_reader_says_nobody() {
    let temp = std::env::temp_dir().join(format!("piirit-calls-ring-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; the window reaches pages that use it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Two profiles, each told.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    common::register_dbus_enum();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(env!("CARGO_BIN_EXE_fake-core-server")).into(),
    );
    engine.load_data(QByteArray::from(
        probe_qml(&tree.join("components")).as_str(),
    ));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let window = format!("file://{}", tree.join("piirit.qml").display());
    let mut steps: Vec<(&str, Vec<String>)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, Vec<String>)> = std::ptr::addr_of_mut!(steps);
    let seen = journal.clone();

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

    // SAFETY for every block: these fire only while `exec()` runs on this
    // thread, and everything they point at outlives it.
    single_shot(Duration::from_secs(1), move || unsafe {
        assert_eq!(call!("loadWindow", QString::from(window.clone())), "ok");
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        call!("refresh");
    });

    // The core is up and has been told, nothing having been set.
    let fresh = seen.clone();
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("fresh", written(&fresh)));
        call!("ring", QString::from("false"));
    });

    // Ringing switched off; then on again.
    let silent = seen.clone();
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("silent", written(&silent)));
        call!("ring", QString::from("true"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("ringing-again", written(&seen)));
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

    let fresh = value("fresh");
    assert!(
        !fresh.is_empty() && fresh.iter().all(|who| who == "1"),
        "a phone that has never been asked told the core something other \
         than its own default, which goes with the profile to wherever it \
         is copied. {context}"
    );
    let silent = value("silent");
    assert!(
        silent.len() > fresh.len() && silent[fresh.len()..].iter().all(|who| who == "2"),
        "the reader switching ringing off did not tell the core. {context}"
    );
    let again = value("ringing-again");
    assert!(
        again.len() > silent.len() && again[silent.len()..].iter().all(|who| who == "1"),
        "ringing switched back on left the reader's \"nobody\" behind. {context}"
    );
}
