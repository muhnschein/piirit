//! Giving up on a relay, and trying another one.
//!
//! The double's `slow` relay holds the transport call the way a relay
//! that is down holds the real core: for longer than anyone waits. What
//! is pinned here is what the shim does around that (`src/signup.rs`):
//!
//! - a cancelled attempt answers nobody, and the retry after it does not
//!   pick up the account the core is still configuring -- which the real
//!   core refuses with "There is already another ongoing process
//!   running", and which the double refuses in the same words;
//! - an attempt that outlives its deadline is given up on: the process
//!   is stopped, the page is told how long it waited, and a profile the
//!   relay makes after all is removed rather than kept.
//!
//! Contract tests, as `onboarding.rs`: one Qt event loop against the
//! recording double, then its journal.

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

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Records what the shim signalled, in the Qt 5.6 dialect with the
/// shim's `snake_case` names.
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

    // SAFETY: single-threaded test binary, and all of these have to be
    // set before Qt initialises and before the shim spawns the server
    // that inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", &accounts);
        // The slow relay answers three and a half seconds after it is
        // asked: between the ticks below, never on one.
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

    // 1s: a relay that will not answer for a while.
    let slow = core_ptr.clone();
    single_shot(Duration::from_secs(1), move || {
        if let Some(this) = slow.as_pinned() {
            this.borrow_mut().create_profile(
                QString::from("Ada"),
                QString::from("dcaccount:slow.example"),
            );
        }
    });

    // 2s: the reader gives up on it.
    let cancel = core_ptr.clone();
    single_shot(Duration::from_secs(2), move || {
        if let Some(this) = cancel.as_pinned() {
            this.borrow_mut().cancel_ongoing();
        }
    });

    // 3s: and tries another, while the core is still on the first.
    let retry = core_ptr.clone();
    single_shot(Duration::from_secs(3), move || {
        if let Some(this) = retry.as_pinned() {
            let provider = this.borrow_mut().default_provider_qr();
            this.borrow_mut()
                .create_profile(QString::from("Ada"), provider);
        }
    });

    // 6s: a relay that answers late -- after the two seconds it is
    // given here, and deaf to the stop that comes at the end of them.
    let deaf = core_ptr;
    single_shot(Duration::from_secs(6), move || {
        if let Some(this) = deaf.as_pinned() {
            this.borrow_mut().profile_timeout = 2;
            this.borrow_mut().create_profile(
                QString::from("Ada"),
                QString::from("dcaccount:slow.deaf.example"),
            );
        }
    });

    let engine_ptr = &engine as *const QmlEngine;
    single_shot(Duration::from_secs(11), move || {
        // SAFETY: see tests/smoke.rs -- the callback only fires while
        // `exec()` is still running on this thread.
        unsafe {
            (*engine_ptr).quit();
        }
    });

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

/// One profile, on the retry's account; no error for the cancelled
/// attempt; one time-out, saying how long it waited.
fn assert_signals(summary: &str, context: &str) {
    assert!(!summary.is_empty(), "the probe QML never loaded. {context}");
    assert_eq!(
        summary, "1/2/0/1/2",
        "expected created/account/errors/timed-out/seconds. {context}"
    );
}

/// The retry did not pick up the account the core was still holding:
/// the transport calls went to accounts 1, 2 and then -- once the first
/// was let go of -- 1 again, and the first was stopped when cancelled.
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

/// The relay that answered after its time was up made a profile nobody
/// was waiting for, and the shim removed it -- after the answer, not
/// before.
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
