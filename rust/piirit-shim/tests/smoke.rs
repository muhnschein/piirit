//! Round trip: `qt_method` runs on background runtime; `queued_callback` sets
//! `qt_property`s on the Qt thread.

// Qt harness: `unsafe` `set_var` pre-Qt; `borrow_as_ptr`.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

#[test]
fn health_check_round_trips_through_qt_event_loop() {
    // SAFETY: single-threaded; platform set before Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());

    let mut engine = QmlEngine::new();
    // Backing C++ QObject; `QPointer::from` needs it.
    engine.set_object_property("core".into(), core_box.pinned());
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let server_path = QString::from(env!("CARGO_BIN_EXE_fake-health-server"));
    core_box.pinned().borrow_mut().start(server_path);

    let mut plan = common::Plan::default();

    let health_check_ptr = core_ptr;
    // Whole seconds only (clippy.toml).
    plan.after(1, move || {
        if let Some(this) = health_check_ptr.as_pinned() {
            this.borrow_mut().check_health();
        }
    });

    let engine_ptr: *const QmlEngine = &engine;
    plan.after(2, move || {
        // SAFETY: fires only in `exec()`; `engine` outlives it.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    plan.run();
    engine.exec();

    let status = core_box.pinned().borrow().status.to_string();
    let system_info = core_box.pinned().borrow().system_info.to_string();

    assert_eq!(status, "ready", "system_info was: {system_info}");
    assert!(
        system_info.contains("fake-health-server"),
        "system_info was: {system_info}"
    );
}
