//! Pins the one QML convention that silently breaks on device: Sailfish
//! runs **Qt 5.6**, where `Connections` only recognises `onFoo:` script
//! bindings. The `function onFoo() {}` form is Qt 5.15+, and on 5.6 it is
//! not a syntax error -- it is just an ordinary function declaration that
//! never gets connected, so handlers stop firing with no diagnostic at
//! all. (That shipped once: the app launched, rendered, and sat on a
//! spinner forever because every `Connections` handler was dead.)
//!
//! It also pins how signal *parameters* reach QML: qmetaobject writes the
//! Rust identifiers into the metaobject verbatim, so handlers see
//! `snake_case` names (`context_id`, not `contextId`).

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

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

#[test]
fn old_style_handlers_receive_snake_case_signal_parameters() {
    // SAFETY: see the same justification in tests/smoke.rs.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // The handler only records anything if every injected parameter name
    // resolves *and* the handler is genuinely connected. A
    // `function onAccount_error(...)` declaration here, or camelCase
    // parameter names, would leave `seen` empty.
    let qml = r#"
        import QtQuick 2.0
        Item {
            // What the handler was actually passed, so the assertion reads
            // the parameter rather than some side effect further on.
            property string seen: ""
            Connections {
                target: core
                onAccount_error: {
                    // `message` is the parameter name the shim declares;
                    // camelCase, or a `function onAccount_error(msg)` form,
                    // would leave this handler dead or throw a
                    // ReferenceError.
                    seen = message
                }
            }
            function report() { return seen }
        }
    "#;
    engine.load_data(QByteArray::from(qml));

    // With no core running this emits `account_error("not started")`,
    // which is the signal the QML above listens for.
    core_box.pinned().borrow_mut().refresh_accounts();

    let seen = QString::from_qvariant(engine.invoke_method("report".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    assert_eq!(
        seen, "not started",
        "the handler never fired, or `message` did not resolve: qmetaobject \
         injects the shim's own parameter names, so a camelCase guess throws \
         a ReferenceError and leaves this empty. (Whether the handler is \
         written in the Qt 5.6 form is a separate question, and one host Qt \
         cannot answer -- tests/qml_syntax.rs scans for that.)"
    );
}
