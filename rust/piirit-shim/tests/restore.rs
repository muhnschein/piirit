//! Taking profile over: from a device or a file.
//!
//! Core's own import, driven like `src/signup.rs` drives signup (Qt loop vs
//! recording double, then journal). Pinned here is the shim's part:
//! - transfer reports itself; imported profile announced with IO started
//!   (imported account has none);
//! - code not from a device offering a profile: refused before transfer, with
//!   this app's reason, not core's protocol words;
//! - failed import takes its account (unlike refused signup): half-written
//!   account is useless, and next attempt would reuse any unconfigured one;
//! - transfer given up answers nobody; core's process stopped on held account.

// Qt harness: `unsafe` `set_var` pre-Qt (`unused_unsafe`: edition 2024 only);
// `borrow_as_ptr`.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Records shim signals; Qt 5.6 dialect, `snake_case` names.
const PROBE_QML: &str = r"
        import QtQuick 2.0
        Item {
            property int restored: 0
            property int lastAccount: 0
            property int steps: 0
            property int dones: 0
            property string refusals: ''
            property string failures: ''
            Connections {
                target: core
                onProfile_restored: {
                    restored = restored + 1
                    lastAccount = account_id
                }
                onRestore_progress: {
                    steps = steps + 1
                    if (permille === 1000) {
                        dones = dones + 1
                    }
                }
                onRestore_refused: refusals = refusals + reason + '|'
                onRestore_failed: failures = failures + 'failed|'
            }
            function summary() {
                return restored + '/' + lastAccount + '/' + steps + '/'
                    + dones + '/' + refusals + '/' + failures
            }
        }
    ";

#[test]
fn a_profile_is_taken_over_from_a_device_or_a_file_and_never_half_kept() {
    let temp = std::env::temp_dir().join(format!("piirit-restore-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    let accounts = temp.join("accounts");
    std::fs::create_dir_all(&accounts).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", &accounts);
        // Slow transfer answers between ticks, not on one.
        std::env::set_var("PIIRIT_FAKE_SLOW_MS", "2500");
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

    // 1s: code another device shows while offering profile.
    let device = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = device.as_pinned() {
            this.borrow_mut()
                .restore_from_device(QString::from("DCBACKUP2:one.example"));
        }
    });

    // 2s: code from newer device than this core reads. Refused before transfer.
    let too_new = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = too_new.as_pinned() {
            this.borrow_mut()
                .restore_from_device(QString::from("DCBACKUP2:toonew.example"));
        }
    });

    // 3s: file core cannot read.
    let bad_file = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = bad_file.as_pinned() {
            this.borrow_mut()
                .restore_from_file(QString::from("/tmp/fail-backup.tar"));
        }
    });

    // 4s: one it can.
    let good_file = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = good_file.as_pinned() {
            this.borrow_mut()
                .restore_from_file(QString::from("/tmp/holiday-backup.tar"));
        }
    });

    // 5s: slow transfer; 6s: reader gives up. Late answer is nobody's.
    let slow = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = slow.as_pinned() {
            this.borrow_mut()
                .restore_from_device(QString::from("DCBACKUP2:slow.example"));
        }
    });
    let cancel = core_ptr;
    plan.after(1, move || {
        if let Some(this) = cancel.as_pinned() {
            this.borrow_mut().cancel_ongoing();
        }
    });

    let engine_ptr: *const QmlEngine = &engine;
    plan.after(4, move || {
        // SAFETY: see tests/smoke.rs; fires only while `exec()` runs.
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
    assert_imports_and_io(&calls, &context);
    assert_nothing_half_kept(&calls, &context);
}

/// Two profiles arrived (second on own account); transfers reported, ending in
/// core's "done"; newer device's code refused; unreadable file failed once.
fn assert_signals(summary: &str, context: &str) {
    assert!(!summary.is_empty(), "the probe QML never loaded. {context}");
    let parts: Vec<&str> = summary.split('/').collect();
    assert_eq!(parts.len(), 6, "unexpected summary shape. {context}");
    assert_eq!(parts[0], "2", "two profiles should have arrived. {context}");
    // Account 3: see `assert_imports_and_io`.
    assert_eq!(
        parts[1], "3",
        "the second profile is not on its own account. {context}"
    );
    assert!(
        parts[2].parse::<u32>().unwrap_or(0) >= 4,
        "the transfers did not report themselves. {context}"
    );
    assert_eq!(
        parts[3], "2",
        "the transfers that finished did not report the core's done. \
         {context}"
    );
    assert_eq!(
        parts[4], "too-new|",
        "the newer device's code was not refused as such. {context}"
    );
    assert_eq!(
        parts[5], "failed|",
        "the unreadable file did not fail exactly once. {context}"
    );
}

/// Device transfers via `get_backup` with code, files via `import_backup` with
/// path; IO started on each arrived profile (imported account has none).
fn assert_imports_and_io(calls: &[(String, Value)], context: &str) {
    let of = |name: &str| -> Vec<(u64, String)> {
        calls
            .iter()
            .filter(|(method, _)| method == name)
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
            .collect()
    };
    // By account: 1 first profile; 2 failed file; 3 arrived; 4 given up. Core
    // never reuses ids.
    assert_eq!(
        of("get_backup"),
        vec![
            (1, "DCBACKUP2:one.example".to_string()),
            (4, "DCBACKUP2:slow.example".to_string()),
        ],
        "the transfers from a device, in order. {context}"
    );
    assert_eq!(
        of("import_backup"),
        vec![
            (2, "/tmp/fail-backup.tar".to_string()),
            (3, "/tmp/holiday-backup.tar".to_string()),
        ],
        "the imports from a file, in order. {context}"
    );
    let started: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "start_io")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        started,
        vec![1, 3],
        "IO was not started on each profile that arrived. {context}"
    );
}

/// Refused code cost no transfer. Failed file took its account. Given-up
/// transfer stopped in core, with its account if late.
fn assert_nothing_half_kept(calls: &[(String, Value)], context: &str) {
    assert!(
        calls
            .iter()
            .filter(|(method, _)| method == "check_qr")
            .any(|(_, params)| {
                params.get(1).and_then(Value::as_str) == Some("DCBACKUP2:toonew.example")
            }),
        "the code was not read before the transfer. {context}"
    );
    let removed: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "remove_account")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        removed,
        vec![2, 4],
        "an account a transfer left behind was kept: the failed import's, \
         and the one the reader gave up on. {context}"
    );
    let stopped: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "stop_ongoing_process")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        stopped,
        vec![4],
        "the transfer given up on was not stopped. {context}"
    );
}
