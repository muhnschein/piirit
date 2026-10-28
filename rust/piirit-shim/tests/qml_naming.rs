//! `qmetaobject` exposes Rust names to QML verbatim, `snake_case` too. Every `.qml` file depends
//! on it, so pinned with a real load, not assumed.

// Qt harness: `unsafe` for `env::set_var` pre-Qt. Lints: `unused_unsafe`
// (edition 2024+), `borrow_as_ptr`.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

#[test]
fn methods_properties_and_signals_are_exposed_verbatim_snake_case() {
    // SAFETY: same justification as tests/smoke.rs.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    let server_path = QString::from(env!("CARGO_BIN_EXE_fake-health-server"));
    core_box.pinned().borrow_mut().start(server_path);

    // Wrong name: QML unbound.
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

    let mut plan = common::Plan::default();

    let engine_ptr: *const QmlEngine = &engine;
    plan.after(8, move || {
        // SAFETY: same lifetime argument as tests/smoke.rs.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    plan.run();
    engine.exec();

    let system_info = core_box.pinned().borrow().system_info.to_string();
    assert!(
        system_info.contains("fake-health-server"),
        "core.check_health() from QML did not run (system_info: {system_info:?}); \
         snake_case method/property names may not be what's actually exposed to QML"
    );
}
