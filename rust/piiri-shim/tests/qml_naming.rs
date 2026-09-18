//! `qmetaobject` exposes Rust identifiers to QML verbatim, `snake_case` and
//! all. Every `.qml` file depends on that, so it is pinned with a real load
//! rather than assumed.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;

#[test]
fn methods_properties_and_signals_are_exposed_verbatim_snake_case() {
    // SAFETY: see the same justification in tests/smoke.rs.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    let server_path = QString::from(env!("CARGO_BIN_EXE_fake-health-server"));
    core_box.pinned().borrow_mut().start(server_path);

    // A wrong method, property or handler name leaves this QML unbound.
    let qml = r"
        import QtQuick 2.0
        Item {
            property bool sawSystemInfoChanged: false
            Connections {
                target: core
                onSystem_info_changed: sawSystemInfoChanged = true
            }
            // Retried rather than fired once: the server is a process, and
            // under a loaded `make check` it is not always up by the first
            // tick.
            Timer {
                id: poll
                interval: 300; running: true; repeat: true
                onTriggered: {
                    if (core.system_info.length > 0) { poll.running = false }
                    else { core.check_health() }
                }
            }
        }
    ";
    engine.load_data(QByteArray::from(qml));

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(8), move || {
        // SAFETY: see tests/smoke.rs -- same lifetime argument.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let system_info = core_box.pinned().borrow().system_info.to_string();
    assert!(
        system_info.contains("fake-health-server"),
        "core.check_health() from QML did not run (system_info: {system_info:?}); \
         snake_case method/property names may not be what's actually exposed to QML"
    );
}
