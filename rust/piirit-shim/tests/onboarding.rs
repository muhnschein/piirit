//! Onboarding actions as seen by core.
//!
//! Contract tests: Qt event loop vs recording double (`src/bin/fake_core_server.rs`), asserting on
//! its JSON-RPC journal. One process: Qt allows one `QGuiApplication`.

// Qt harness: unsafe set_var pre-Qt; borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use std::path::PathBuf;

mod common;
use serde_json::Value;

/// Records shim's signals, the half journal can't show. Qt 5.6 dialect, `snake_case` names:
/// rename fails here, not on phone.
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

    // SAFETY: single-threaded; all three set before Qt and before shim spawns server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        // Keeps shim's accounts dir out of developer's real XDG data dir.
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", &accounts);
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // QML is part of the assertion: the signals half of contract. Qt 5.6 dialect the device
    // needs.
    engine.load_data(QByteArray::from(PROBE_QML));

    let server = QString::from(env!("CARGO_BIN_EXE_fake-core-server"));
    core_box.pinned().borrow_mut().start(server);

    // One step per tick.
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let mut plan = common::Plan::default();

    let chatmail = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = chatmail.as_pinned() {
            let provider = this.borrow_mut().default_provider_qr();
            this.borrow_mut()
                .create_profile(QString::from("Ada"), provider);
        }
    });

    let failing = core_ptr.clone();
    plan.after(2, move || {
        if let Some(this) = failing.as_pinned() {
            // Double treats `fail` as unreachable server.
            this.borrow_mut().create_profile(
                QString::from("Ada"),
                QString::from("dcaccount:fail.invalid"),
            );
        }
    });

    let email = core_ptr;
    plan.after(2, move || {
        if let Some(this) = email.as_pinned() {
            this.borrow_mut().create_profile_with_email(
                QString::from("Grace"),
                QString::from("grace@example.org"),
                QString::from("hunter2"),
            );
        }
    });

    let engine_ptr: *const QmlEngine = &engine;
    plan.after(2, move || {
        // SAFETY: see tests/smoke.rs; callback fires only while `exec()` runs.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    plan.run();
    engine.exec();

    let calls = read_journal(&journal);
    assert_current_transport_api(&calls);
    assert_chatmail_profile(&calls);
    assert_email_profile(&calls);
    assert_failed_attempt_left_no_orphan(&calls);

    // Signals QML binds to, with their values.
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

/// Deprecated `set_config(mail_pw)` + `configure` path must stay gone.
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

/// Display name, then provider payload.
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

    // Before transport call, so in place when core announces account.
    let name_index = names.iter().position(|method| *method == "set_config");
    let transport_index = names
        .iter()
        .position(|method| *method == "add_transport_from_qr");
    assert!(
        name_index < transport_index,
        "display name must be set before the transport is added: {names:?}"
    );
}

/// Two required fields of `EnteredLoginParam`, as object.
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

/// Failed attempt must not strand an account.
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
