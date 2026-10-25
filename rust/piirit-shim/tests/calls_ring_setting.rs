//! Core rings for contacts by default; only reader switching ringing off tells it "nobody".
//!
//! `who_can_call_me` is per device, but lives in profile DB that backup or second device takes
//! whole. Window once wrote "nobody" for every profile while calls were off: Delta Chat from
//! Piirit profile never rang. Ringing is a setting; calls are not (#119).

// Qt harness: see qml_share.rs.
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
use serde_json::json;

mod common;

/// Silica's `pageStack`; see `network_hint.rs`. Nothing here navigates.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
}

/// Window, components dir imported so probe can write settings.
fn probe_qml(components: &std::path::Path) -> String {
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    Item {
        Loader { id: window }
        function loadWindow(url) {
            window.setSource(url, {})
            return window.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // What every page that lists the profiles does, and what writes
        // the settings to each profile the core has.
        function refresh() { core.refresh_accounts(); return 'ok' }
        function ring(on) { Settings.callsRing = on === 'true'; return 'ok' }
        function status() { return core.status }
    }
";

/// Label, then ringing set after it; last quits. "fresh": core up and told, nothing set yet.
const PLAN: &[(&str, Option<&str>)] = &[
    ("fresh", Some("false")),
    ("silent", Some("true")),
    ("ringing-again", None),
];

/// `who_can_call_me` values written by each step, labelled.
type Steps = Vec<(&'static str, Vec<String>)>;

/// Probe method's answer as text.
///
/// # Safety
/// `engine` live; called inside `exec()`.
unsafe fn call(engine: *mut QmlEngine, name: &str, args: &[QVariant]) -> String {
    let result = (*engine).invoke_method(name.into(), args);
    QString::from_qvariant(result)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// Once one write per profile past `told` lands: record first step of `plan`, set its
/// ringing, go on to rest.
///
/// # Safety
/// `engine`, `steps` live till `exec()` returns.
unsafe fn step(
    engine: *mut QmlEngine,
    steps: *mut Steps,
    journal: PathBuf,
    plan: &'static [(&'static str, Option<&'static str>)],
    told: usize,
) {
    let probe = journal.clone();
    when(
        move || written(&probe).len() >= told + PROFILES,
        PATIENCE,
        move || unsafe {
            let Some(((label, ring), rest)) = plan.split_first() else {
                return;
            };
            let seen = written(&journal);
            let told = seen.len();
            (*steps).push((label, seen));
            match ring {
                Some(on) => {
                    call(engine, "ring", &[QString::from(*on).into()]);
                    step(engine, steps, journal, rest, told);
                }
                None => (*engine).quit(),
            }
        },
    );
}

/// Profiles fake core lists; each change writes once per profile.
const PROFILES: usize = 2;

/// Checks a step waits for its condition; then runs anyway, asserts say what was missing.
const PATIENCE: u32 = 30;

/// Run `then` once `ready` holds, checked each second, or when `patience` checks run out.
fn when<R, T>(ready: R, patience: u32, then: T)
where
    R: Fn() -> bool + 'static,
    T: FnOnce() + 'static,
{
    if ready() || patience == 0 {
        then();
        return;
    }
    let mut pending = Some((ready, then));
    single_shot(Duration::from_secs(1), move || {
        if let Some((ready, then)) = pending.take() {
            when(ready, patience - 1, then);
        }
    });
}

/// Every `who_can_call_me` value set, in order.
fn written(journal: &std::path::Path) -> Vec<String> {
    common::calls(journal)
        .into_iter()
        .filter(|(name, params)| name == "set_config" && params[1] == json!("who_can_call_me"))
        .map(|(_, params)| params[2].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn the_core_rings_for_contacts_until_the_reader_says_nobody() {
    let temp = std::env::temp_dir().join(format!("piirit-calls-ring-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; window reaches pages using it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Two profiles, each told.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2"); // `PROFILES`
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    common::register_dbus_enum();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.set_property(
        "rpcServerPath".into(),
        QString::from(env!("CARGO_BIN_EXE_fake-core-server")).into(),
    );
    engine.load_data(QByteArray::from(
        probe_qml(&tree.join("components")).as_str(),
    ));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let window = format!("file://{}", tree.join("piirit.qml").display());
    let mut steps: Steps = Vec::new();
    let steps_ptr: *mut Steps = std::ptr::addr_of_mut!(steps);

    // Each step waits on what it needs, not clock: under coverage, core answers seconds late.
    // SAFETY for every block: fire only inside `exec()`; pointees outlive it.
    single_shot(Duration::from_secs(1), move || unsafe {
        let window = QString::from(window.clone());
        assert_eq!(call(engine_ptr, "loadWindow", &[window.into()]), "ok");
        let journal = journal.clone();
        when(
            move || unsafe { call(engine_ptr, "status", &[]) == "ready" },
            PATIENCE,
            move || unsafe {
                call(engine_ptr, "refresh", &[]);
                step(engine_ptr, steps_ptr, journal, PLAN, 0);
            },
        );
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    let fresh = value("fresh");
    assert!(
        !fresh.is_empty() && fresh.iter().all(|who| who == "1"),
        "a phone that has never been asked told the core something other \
         than its own default, which goes with the profile to wherever it \
         is copied. {context}"
    );
    let silent = value("silent");
    assert!(
        silent.len() > fresh.len() && silent[fresh.len()..].iter().all(|who| who == "2"),
        "the reader switching ringing off did not tell the core. {context}"
    );
    let again = value("ringing-again");
    assert!(
        again.len() > silent.len() && again[silent.len()..].iter().all(|who| who == "1"),
        "ringing switched back on left the reader's \"nobody\" behind. {context}"
    );
}
