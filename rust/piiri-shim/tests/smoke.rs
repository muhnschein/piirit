//! The async round trip, end to end: a `qt_method` starts work on the
//! background runtime and the result returns through `queued_callback` to
//! mutate `qt_property`s on the Qt thread.

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
fn health_check_round_trips_through_qt_event_loop() {
    // SAFETY: single-threaded test binary, and Qt needs its platform before
    // the first QGuiApplication.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());

    let mut engine = QmlEngine::new();
    // Creates the backing C++ QObject, which `QPointer::from` requires.
    engine.set_object_property("core".into(), core_box.pinned());
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let server_path = QString::from(env!("CARGO_BIN_EXE_fake-health-server"));
    core_box.pinned().borrow_mut().start(server_path);

    let health_check_ptr = core_ptr.clone();
    // Whole seconds only; see clippy.toml.
    single_shot(Duration::from_secs(1), move || {
        if let Some(this) = health_check_ptr.as_pinned() {
            this.borrow_mut().check_health();
        }
    });

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(3), move || {
        // SAFETY: fires only while `exec()` is running on this thread, and
        // `engine` outlives it.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let status = core_box.pinned().borrow().status.to_string();
    let system_info = core_box.pinned().borrow().system_info.to_string();

    assert_eq!(status, "ready", "system_info was: {system_info}");
    assert!(
        system_info.contains("fake-health-server"),
        "system_info was: {system_info}"
    );
}
