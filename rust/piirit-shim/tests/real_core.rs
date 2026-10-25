//! Does real core accept the shapes this shim sends?
//!
//! `tests/onboarding.rs`'s double shares the author's reading of the API, so a
//! wrong reading passes both. This drives shim vs pinned binary; separates a
//! request core can't decode from one it can't deliver. Offline: reserved TLD.
//!
//! Gated on `DELTACHAT_RPC_SERVER`.

// Qt harness: `unsafe` `set_var` pre-Qt (`unused_unsafe`: edition 2024 only);
// `borrow_as_ptr`.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Serde's message for wrong param shape.
const SHAPE_ERRORS: &[&str] = &[
    "invalid type",
    "missing field",
    "unknown field",
    "unknown variant",
    "invalid params",
    "expected struct",
    "Invalid params",
];

/// Resolve gate value; relative path is relative to repo root, not working dir.
///
/// Cargo runs tests from the package root, so `DELTACHAT_RPC_SERVER=../vendor/...`
/// (as README, Makefile, CI write) would look under `rust/`. Failure shows only
/// where the var is set (CI) and reads as a missing download, not a wrong path.
fn resolve(path: &str) -> String {
    let path = std::path::Path::new(path);
    if path.is_absolute() {
        return path.to_string_lossy().into_owned();
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
        .to_string_lossy()
        .into_owned()
}

fn real_server() -> Option<String> {
    match std::env::var("DELTACHAT_RPC_SERVER") {
        Ok(path) if !path.is_empty() => Some(resolve(&path)),
        _ => {
            eprintln!(
                "skipping: set DELTACHAT_RPC_SERVER to a real deltachat-rpc-server binary to run"
            );
            None
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_real_core_accepts_the_shapes_we_send() {
    let Some(server) = real_server() else {
        return;
    };

    let temp = std::env::temp_dir().join(format!("piirit-real-core-{}", std::process::id()));
    // Removed first: dir keyed by pid; a recycled one hands core an old account db.
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp accounts dir");

    // SAFETY: single-threaded; both set before Qt and before server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", &temp);
    }

    // Second-device offer is its own object, not a core method; QML makes one.
    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.set_object_property("core".into(), core_box.pinned());

    // Errors reported; plus how core classified a `dcaccount:` payload.
    let qml = r"
        import QtQuick 2.0
        import Piirit 1.0
        Item {
            property string errors: ''
            property int created: 0
            property string qrKind: ''
            Connections {
                target: core
                onProfile_error: errors = errors + '|' + message
                onProfile_created: created = created + 1
                onQr_checked: qrKind = kind
                // Taking a profile over uses the same API reading; its failures
                // belong in the same report.
                onRestore_failed: errors = errors + '|' + message
                onRestore_refused: errors = errors + '|refused=' + reason
                // Adding a relay is the same transport call on an existing
                // account, so its failure belongs here too.
                onRelay_error: errors = errors + '|relay=' + message
                onRelay_added: created = created + 1
                // 'not started' from every call says only that core never came
                // up; the status says why.
                onStatus_changed: errors = errors + '|status=' + core.status
            }
            // The other end of taking a profile over: this phone offering one.
            // Two calls of its own (`provide_backup`, `get_backup_qr`), so their
            // shapes belong in this report too.
            SecondDevice {
                id: device
                account_id: 1
                onError: errors = errors + '|offer=' + message
                onStalled: errors = errors + '|offer=stalled'
                onTaken: created = created + 1
            }
            function offerToASecondDevice() { device.offer() }
            function report() { return created + '#' + qrKind + '#' + errors }
        }
    ";
    engine.load_data(QByteArray::from(qml));

    core_box.pinned().borrow_mut().start(QString::from(server));
    let core_ptr: QPointer<DeltaChatCore> = QPointer::from(core_box.pinned().borrow());

    let mut plan = common::Plan::default();

    // Whole seconds only (qmetaobject 0.2.10; clippy.toml).
    let email = core_ptr.clone();
    plan.after(1, move || {
        if let Some(this) = email.as_pinned() {
            this.borrow_mut().create_profile_with_email(
                QString::from("Ada"),
                // RFC 2606 reserved: fails at connect, not parse.
                QString::from("ada@piirit-test.invalid"),
                QString::from("not-a-real-password"),
            );
        }
    });

    let qr = core_ptr.clone();
    plan.after(8, move || {
        if let Some(this) = qr.as_pinned() {
            // Account 1 exists by now.
            this.borrow_mut()
                .check_qr(1, QString::from("dcaccount:piirit-test.invalid"));
        }
    });

    // Import shape on missing file: core says it can't read it, a delivery
    // failure like the unreachable server, not a params complaint. `get_backup`
    // needs a second device offering a profile, so fake core and the code check
    // cover it.
    let backup = core_ptr.clone();
    plan.after(2, move || {
        if let Some(this) = backup.as_pinned() {
            this.borrow_mut()
                .restore_from_file(QString::from("/nonexistent/piirit-test.tar"));
        }
    });

    // Another relay for account 1, same unreachable host: core fails to reach
    // it, not complaining about shape.
    let relay = core_ptr;
    plan.after(2, move || {
        if let Some(this) = relay.as_pinned() {
            this.borrow_mut()
                .add_relay(1, QString::from("dcaccount:piirit-test.invalid"));
        }
    });

    // Offer account 1 to second device. Core can't: account whose configure
    // failed has no key to back up; delivery failure, not params. Real provider's
    // behavior is fake core's (`qml_second_device.rs`); this checks what real core
    // makes of the call.
    let offer_ptr = std::ptr::addr_of_mut!(engine);
    plan.after(2, move || {
        // SAFETY: see tests/smoke.rs.
        unsafe {
            (*offer_ptr).invoke_method("offerToASecondDevice".into(), &[]);
        }
    });

    let engine_ptr: *const QmlEngine = &engine;
    plan.after(5, move || {
        // SAFETY: see tests/smoke.rs.
        unsafe {
            (*engine_ptr).quit();
        }
    });

    plan.run();
    engine.exec();

    let report = QString::from_qvariant(engine.invoke_method("report".into(), &[]))
        .map(|value| value.to_string())
        .unwrap_or_default();
    let mut parts = report.splitn(3, '#');
    let created = parts.next().unwrap_or_default().to_string();
    let qr_kind = parts.next().unwrap_or_default().to_string();
    let errors = parts.next().unwrap_or_default().to_string();

    assert_eq!(
        created, "0",
        "an unreachable server somehow configured successfully: {report}"
    );
    assert!(
        !errors.is_empty(),
        "no result came back within the time budget. Either the core got \
         slower at failing on an unresolvable host, or the call never \
         completed. Report: {report:?}"
    );

    for marker in SHAPE_ERRORS {
        assert!(
            !errors.contains(marker),
            "the real core rejected our request shape ({marker:?}) rather \
             than failing to reach the server -- the params we send do not \
             match its API. Errors were: {errors}"
        );
    }

    assert!(
        errors.contains("|relay="),
        "adding a relay to the account answered nothing within the time \
         budget. Report: {report:?}"
    );

    // Without this, the shape check would pass an offer core never answered:
    // provider's two calls go out, say nothing, nothing notices.
    assert!(
        errors.contains("|offer="),
        "offering the profile to a second device answered nothing within \
         the time budget, so nothing here saw what the core made of \
         `provide_backup`. Report: {report:?}"
    );

    // Wrong `check_qr` shape would land in `qr_error`, leaving this empty.
    assert_eq!(
        qr_kind, "account",
        "check_qr did not classify a dcaccount: payload as an account \
         invite: {report}"
    );
}
