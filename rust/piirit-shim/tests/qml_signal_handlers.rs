//! Pins QML that breaks silently on device: Sailfish Qt 5.6 `Connections` knows only `onFoo:`
//! bindings. `function onFoo() {}` (Qt 5.15+) is on 5.6 a plain unconnected function, no
//! error: handlers dead (shipped once: app sat on spinner forever).
//!
//! Also: qmetaobject writes Rust identifiers verbatim, so handler params are `snake_case`
//! (`context_id`, not `contextId`).

// Qt harness: `unsafe` env::set_var; `borrow_as_ptr`.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

#[test]
fn old_style_handlers_receive_snake_case_signal_parameters() {
    // SAFETY: as in tests/smoke.rs.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // Records only if every param name resolves and handler really connects. A
    // `function onAccount_error(...)` here, or camelCase params, leaves `seen` empty.
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

    // No core: emits `account_error("not started")` for QML.
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
