//! Giving up on a relay, and trying another.
//!
//! Double's `slow` relay holds transport call as a down relay holds real core. Pinned
//! (`src/signup.rs`): cancelled attempt answers nobody, and retry doesn't take the account core
//! still configures (core refuses "There is already another ongoing process running"). Attempt past
//! deadline: process stopped, page told how long it waited, late profile removed.
//!
//! Contract tests as `onboarding.rs`.

// Qt harness: unsafe set_var pre-Qt; borrow_as_ptr.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Records shim's signals, Qt 5.6 dialect, `snake_case` names.
const PROBE_QML: &str = r"
        import QtQuick 2.0
        Item {
            property int created: 0
            property int lastAccount: 0
            property int errors: 0
            property int timedOut: 0
            property int lastSeconds: 0
            Connections {
                target: core
                onProfile_created: {
                    created = created + 1
                    lastAccount = account_id
                }
                onProfile_error: errors = errors + 1
                onProfile_timed_out: {
                    timedOut = timedOut + 1
                    lastSeconds = seconds
                }
            }
            function summary() {
                return created + '/' + lastAccount + '/' + errors + '/'
                    + timedOut + '/' + lastSeconds
            }
        }
    ";

#[test]
fn a_relay_given_up_on_is_neither_reused_nor_kept() {
    let temp = std::env::temp_dir().join(format!("piirit-profile-retry-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    let accounts = temp.join("accounts");
    std::fs::create_dir_all(&accounts).expect("create temp dirs");

    // SAFETY: single-threaded; all set pre-Qt and before shim spawns server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", &accounts);
        // Slow relay answers 3.5s after asked: between ticks, never on one.
        std::env::set_var("PIIRIT_FAKE_SLOW_MS", "3500");
    }

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    let server = QString::from(env!("CARGO_BIN_EXE_fake-core-server"));
    core_box.pinned().borrow_mut().start(server);

    // Whole seconds only (clippy.toml); one step per tick.
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let mut plan = common::Plan::default();

    // 1s: a relay that will not answer for a while.
    let slow = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = slow.as_pinned() {
            this.borrow_mut().create_profile(
                QString::from("Ada"),
                QString::from("dcaccount:slow.example"),
            );
        }
    });

    // 2s: reader gives up on it.
    let cancel = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = cancel.as_pinned() {
            this.borrow_mut().cancel_ongoing();
        }
    });

    // 3s: tries another, while core still on first.
    let retry = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = retry.as_pinned() {
            let provider = this.borrow_mut().default_provider_qr();
            this.borrow_mut()
                .create_profile(QString::from("Ada"), provider);
        }
    });

    // 6s: relay answers late, after the two seconds given, deaf to the stop.
    let deaf = core_ptr;
    plan.after(3, move || {
        if let Some(this) = deaf.as_pinned() {
            this.borrow_mut().profile_timeout = 2;
            this.borrow_mut().create_profile(
                QString::from("Ada"),
                QString::from("dcaccount:slow.deaf.example"),
            );
        }
    });

    let engine_ptr: *const QmlEngine = &engine;
    plan.after(5, move || {
        // SAFETY: see tests/smoke.rs; callback fires only while `exec()` runs.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    plan.run();
    engine.exec();

    let summary = QString::from_qvariant(engine.invoke_method("summary".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    let calls = common::calls(&journal);
    let context = format!("signals: {summary}\ncalls: {calls:?}");

    assert_signals(&summary, &context);
    assert_retry_took_a_fresh_account(&calls, &context);
    assert_late_profile_was_removed(&calls, &context);
}

/// One profile, on retry's account; no error for cancelled attempt; one time-out with its wait.
fn assert_signals(summary: &str, context: &str) {
    assert!(!summary.is_empty(), "the probe QML never loaded. {context}");
    assert_eq!(
        summary, "1/2/0/1/2",
        "expected created/account/errors/timed-out/seconds. {context}"
    );
}

/// Retry didn't take account core still held: transport calls went to accounts 1, 2, then 1
/// again once let go; first stopped when cancelled.
fn assert_retry_took_a_fresh_account(calls: &[(String, Value)], context: &str) {
    let transports: Vec<(u64, String)> = calls
        .iter()
        .filter(|(method, _)| method == "add_transport_from_qr")
        .map(|(_, params)| {
            (
                params.get(0).and_then(Value::as_u64).unwrap_or(0),
                params
                    .get(1)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    assert_eq!(
        transports,
        vec![
            (1, "dcaccount:slow.example".to_string()),
            (2, "dcaccount:nine.testrun.org".to_string()),
            (1, "dcaccount:slow.deaf.example".to_string()),
        ],
        "the transport calls, in order. {context}"
    );
    let stops: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "stop_ongoing_process")
        .map(|(_, params)| params.get(0).and_then(Value::as_u64).unwrap_or(0))
        .collect();
    assert_eq!(
        stops,
        vec![1, 1],
        "the cancel and the time-out each stop the process on the account \
         the attempt was holding. {context}"
    );
}

/// Late relay's profile removed after answer, not before.
fn assert_late_profile_was_removed(calls: &[(String, Value)], context: &str) {
    let last_transport = calls
        .iter()
        .rposition(|(method, _)| method == "add_transport_from_qr")
        .expect("three transport calls");
    let removed = calls
        .iter()
        .position(|(method, params)| {
            method == "remove_account" && params.get(0).and_then(Value::as_u64) == Some(1)
        })
        .unwrap_or_else(|| panic!("the late profile was not removed. {context}"));
    assert!(
        removed > last_transport,
        "the profile was removed before the relay had answered. {context}"
    );
    assert_eq!(
        calls
            .iter()
            .filter(|(method, _)| method == "remove_account")
            .count(),
        1,
        "only the late profile is removed. {context}"
    );
}
