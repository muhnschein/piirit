//! What the onboarding actions say to the core.
//!
//! Contract tests: they drive a Qt event loop against the recording double
//! (`src/bin/fake_core_server.rs`) and assert on its journal of JSON-RPC
//! calls. One process and one event loop, because Qt allows only one
//! `QGuiApplication`.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;
use serde_json::Value;

/// Records what the shim signalled, the half of the contract the journal
/// cannot show. In the Qt 5.6 dialect with `snake_case` names, so a rename
/// fails here rather than on a phone.
const PROBE_QML: &str = r"
        import QtQuick 2.0
        Item {
            property int created: 0
            property int lastAccount: 0
            property int errors: 0
            property int progressEvents: 0
            property int lastPermille: 0
            Connections {
                target: core
                onProfile_created: {
                    created = created + 1
                    lastAccount = account_id
                }
                onProfile_error: errors = errors + 1
                onConfigure_progress: {
                    progressEvents = progressEvents + 1
                    lastPermille = permille
                }
            }
            // Read back after the loop stops: invoke_method is the only way
            // into the root object from Rust.
            function summary() {
                return created + '/' + errors + '/' + progressEvents
                    + '/' + lastPermille + '/' + lastAccount
            }
        }
    ";

/// One recorded JSON-RPC request.
struct Call {
    method: String,
    params: Value,
}

fn read_journal(path: &PathBuf) -> Vec<Call> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .map(|value| Call {
            method: value
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            params: value.get("params").cloned().unwrap_or(Value::Null),
        })
        .collect()
}

fn methods(calls: &[Call]) -> Vec<&str> {
    calls.iter().map(|call| call.method.as_str()).collect()
}

fn find<'a>(calls: &'a [Call], method: &str) -> Option<&'a Call> {
    calls.iter().find(|call| call.method == method)
}

#[test]
fn onboarding_speaks_the_current_transport_api() {
    let temp = std::env::temp_dir().join(format!("piirit-onboarding-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    let accounts = temp.join("accounts");
    std::fs::create_dir_all(&accounts).expect("create temp dirs");

    // SAFETY: single-threaded test binary, and all three have to be set
    // before Qt initialises and before the shim spawns the server that
    // inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        // Keeps the shim's accounts directory out of the developer's real
        // XDG data dir.
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", &accounts);
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // The QML is part of the assertion: it records what the shim signalled,
    // which is the half of the contract the journal cannot show. Written in
    // the Qt 5.6 dialect the device needs, with the shim's snake_case names.
    engine.load_data(QByteArray::from(PROBE_QML));

    let server = QString::from(env!("CARGO_BIN_EXE_fake-core-server"));
    core_box.pinned().borrow_mut().start(server);

    // Whole seconds only (clippy.toml); one step per tick.
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let chatmail = core_ptr.clone();
    single_shot(Duration::from_secs(1), move || {
        if let Some(this) = chatmail.as_pinned() {
            let provider = this.borrow_mut().default_provider_qr();
            this.borrow_mut()
                .create_profile(QString::from("Ada"), provider);
        }
    });

    let failing = core_ptr.clone();
    single_shot(Duration::from_secs(3), move || {
        if let Some(this) = failing.as_pinned() {
            // The double treats `fail` as an unreachable server.
            this.borrow_mut().create_profile(
                QString::from("Ada"),
                QString::from("dcaccount:fail.invalid"),
            );
        }
    });

    let email = core_ptr;
    single_shot(Duration::from_secs(5), move || {
        if let Some(this) = email.as_pinned() {
            this.borrow_mut().create_profile_with_email(
                QString::from("Grace"),
                QString::from("grace@example.org"),
                QString::from("hunter2"),
            );
        }
    });

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(7), move || {
        // SAFETY: see tests/smoke.rs -- the callback only fires while
        // `exec()` is still running on this thread.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    engine.exec();

    let calls = read_journal(&journal);
    assert_current_transport_api(&calls);
    assert_chatmail_profile(&calls);
    assert_email_profile(&calls);
    assert_failed_attempt_left_no_orphan(&calls);

    // The signals QML binds to fired, with the values it binds to.
    let summary = QString::from_qvariant(engine.invoke_method("summary".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    assert!(
        !summary.is_empty(),
        "the probe QML never loaded, so nothing was recorded. The usual \
         cause is a missing QtQuick runtime plugin (`qml-module-qtquick2` on \
         Debian/Ubuntu); Qt reports it as `module \"QtQuick\" is not \
         installed` above this line."
    );
    let fields: Vec<&str> = summary.split('/').collect();
    assert_eq!(
        fields.len(),
        5,
        "QML summary() returned something unexpected; the handler names may \
         not match the shim's signals. Got {summary:?}"
    );
    assert_eq!(
        fields[0], "2",
        "expected two successful profiles: {summary}"
    );
    assert_eq!(fields[1], "1", "expected one failed profile: {summary}");
    assert!(
        fields[2].parse::<u32>().unwrap_or(0) >= 2,
        "no ConfigureProgress reached QML: {summary}"
    );
    assert_eq!(
        fields[3], "1000",
        "the last progress report should be the core's 1000 = done: {summary}"
    );
}

/// The deprecated `set_config(mail_pw)` + `configure` path must stay gone.
fn assert_current_transport_api(calls: &[Call]) {
    let names = methods(calls);
    assert!(
        !names.is_empty(),
        "the double recorded nothing; did the server fail to start?"
    );
    assert!(
        !names.contains(&"configure"),
        "onboarding called the deprecated `configure`; use \
         add_transport_from_qr / add_or_update_transport (docs/PROJECT.md). \
         Calls were: {names:?}"
    );
    for call in calls {
        if call.method == "set_config" {
            let key = call
                .params
                .get(1)
                .and_then(Value::as_str)
                .unwrap_or_default();
            assert_ne!(
                key, "mail_pw",
                "credentials were pushed through set_config; they belong in \
                 an EnteredLoginParam (docs/PROJECT.md)"
            );
        }
    }
}

/// A display name, then the provider payload.
fn assert_chatmail_profile(calls: &[Call]) {
    let names = methods(calls);
    let display_name = calls
        .iter()
        .find(|call| {
            call.method == "set_config"
                && call.params.get(1).and_then(Value::as_str) == Some("displayname")
        })
        .expect("create_profile must set a display name");
    assert_eq!(
        display_name.params.get(2).and_then(Value::as_str),
        Some("Ada")
    );

    let from_qr = find(calls, "add_transport_from_qr")
        .expect("create_profile must go through add_transport_from_qr");
    assert_eq!(
        from_qr.params.get(1).and_then(Value::as_str),
        Some("dcaccount:nine.testrun.org"),
        "the default provider payload changed"
    );

    // Before the transport call, so it is in place when the core announces
    // the account.
    let name_index = names.iter().position(|method| *method == "set_config");
    let transport_index = names
        .iter()
        .position(|method| *method == "add_transport_from_qr");
    assert!(
        name_index < transport_index,
        "display name must be set before the transport is added: {names:?}"
    );
}

/// The two required fields of `EnteredLoginParam`, as an object.
fn assert_email_profile(calls: &[Call]) {
    let transport = find(calls, "add_or_update_transport")
        .expect("create_profile_with_email must go through add_or_update_transport");
    let param = transport.params.get(1).expect("param object");
    assert_eq!(
        param.get("addr").and_then(Value::as_str),
        Some("grace@example.org")
    );
    assert_eq!(
        param.get("password").and_then(Value::as_str),
        Some("hunter2")
    );
}

/// A failed attempt must not strand an account.
fn assert_failed_attempt_left_no_orphan(calls: &[Call]) {
    let names = methods(calls);
    let added = names
        .iter()
        .filter(|method| **method == "add_account")
        .count();
    assert_eq!(
        added, 2,
        "expected one account for the chatmail profile and one for the email \
         profile -- the failed attempt should have reused the unconfigured \
         account, not added a third. Calls were: {names:?}"
    );
}
