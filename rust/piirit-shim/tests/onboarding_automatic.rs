//! A profile made with no relay named is left to the core's own
//! onboarding, which is what puts it on several relays.
//!
//! Since core 2.61 `init_transports` is the one way in that does: it
//! makes the profile on whichever of the core's relays answers first and
//! turns its background additions on. `add_transport_from_qr` with a
//! relay named is that relay alone, so an empty payload must not become
//! one -- neither a default relay nor a refusal.

// Qt harness: see onboarding.rs.
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
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
        import QtQuick 2.0
        Item {
            property int created: 0
            property int errors: 0
            Connections {
                target: core
                onProfile_created: created = created + 1
                onProfile_error: errors = errors + 1
            }
            function summary() {
                return created + '/' + errors
            }
        }
    ";

#[test]
fn no_relay_named_leaves_the_relays_to_the_core() {
    let temp = std::env::temp_dir().join(format!(
        "piirit-onboarding-automatic-{}",
        std::process::id()
    ));
    let journal = common::fresh_journal(&temp);
    let accounts = temp.join("accounts");
    std::fs::create_dir_all(&accounts).expect("create temp dirs");

    // SAFETY: single-threaded test binary, set before Qt initialises and
    // before the shim spawns the server that inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", &accounts);
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    let server = QString::from(env!("CARGO_BIN_EXE_fake-core-server"));
    core_box.pinned().borrow_mut().start(server);

    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());
    single_shot(Duration::from_secs(1), move || {
        if let Some(this) = core_ptr.as_pinned() {
            this.borrow_mut()
                .create_profile(QString::from("Ada"), QString::default());
        }
    });

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(3), move || {
        // SAFETY: see tests/smoke.rs -- the callback only fires while
        // `exec()` is still running on this thread.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(method, _)| method.as_str()).collect();
    let init = calls
        .iter()
        .find(|(method, _)| method == "init_transports")
        .unwrap_or_else(|| panic!("no relay named did not reach init_transports: {names:?}"));
    assert_eq!(
        init.1.get(1),
        Some(&Value::Null),
        "init_transports was handed a payload; with none the core picks: {names:?}"
    );
    assert!(
        !names.contains(&"add_transport_from_qr"),
        "an empty payload was sent as a relay of its own: {names:?}"
    );
    let name_index = names.iter().position(|method| *method == "set_config");
    let init_index = names.iter().position(|method| *method == "init_transports");
    assert!(
        name_index < init_index,
        "the display name must be set before the core makes the profile: {names:?}"
    );

    let summary = QString::from_qvariant(engine.invoke_method("summary".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    assert_eq!(
        summary, "1/0",
        "the profile the core made was not announced (created/errors)"
    );
}
