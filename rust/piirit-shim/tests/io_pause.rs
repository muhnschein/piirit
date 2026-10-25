//! IO stops with no network, restarts when it returns.
//!
//! Core on dead network keeps retrying, waking radio, never succeeding. Stopping loses nothing: no
//! message arrives over no network.
//!
//! Never stop IO for backgrounding: background is the only way messages arrive here.

// Qt harness: see qml_share.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`; see `network_hint.rs`. Never navigated.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: window }

        function loadWindow(url) {
            window.setSource(url, {})
            return window.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function coreStatus() { return '' + core.status }
        function paused() { return window.item.ioPaused ? 'yes' : 'no' }
        // The real wait before a loss is acted on is half a minute, which
        // is the point of it; a test that sat through that would be a test
        // nobody runs. The component allows it to be shortened for exactly
        // this.
        function hurry() {
            var watch = findIn(window.item, 'networkWatch')
            if (!watch) { return 'missing:networkWatch' }
            watch.lostMs = 400
            return 'ok'
        }
        function away() { window.item.appActive = false; return 'ok' }
        function back() { window.item.appActive = true; return 'ok' }
        function connman(value) {
            var watch = findIn(window.item, 'networkWatch')
            if (!watch) { return 'missing:networkWatch' }
            watch.heard('State', value)
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
    }
";

/// First index of method in what core was asked, if any.
fn first(methods: &[String], name: &str) -> Option<usize> {
    methods.iter().position(|method| method == name)
}

/// Last index of method in what core was asked, if any.
fn last(methods: &[String], name: &str) -> Option<usize> {
    methods.iter().rposition(|method| method == name)
}

#[test]
#[allow(clippy::too_many_lines)]
fn io_stops_only_for_a_lasting_loss_of_network_and_starts_again_with_it() {
    let temp = std::env::temp_dir().join(format!("piirit-io-pause-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; window reaches pages using it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let window = format!("file://{}", tree.join("piirit.qml").display());

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    let steps: common::Steps = common::Steps::default();

    let mut plan = common::Plan::default();

    let loading = steps.clone();
    plan.after(1, move || unsafe {
        common::record(
            &loading,
            "load",
            call!("loadWindow", QString::from(window.clone())).into(),
        );
    });

    // Backgrounded: where app spends most life and does its only useful work. IO must survive.
    let backgrounded = steps.clone();
    let background_journal = journal.clone();
    plan.after(3, move || unsafe {
        common::record(&backgrounded, "status", call!("coreStatus").into());
        common::record(&backgrounded, "hurry", call!("hurry").into());
        call!("away");
        common::record(
            &backgrounded,
            "stops-while-backgrounded",
            common::methods(&background_journal)
                .into_iter()
                .filter(|method| method == "stop_io_for_all_accounts")
                .count()
                .to_string()
                .into(),
        );
        // Network goes, stays gone.
        call!("connman", QString::from("offline"));
    });

    let lost = steps.clone();
    let lost_journal = journal.clone();
    plan.after(2, move || unsafe {
        common::record(
            &lost,
            "stops-after-loss",
            common::methods(&lost_journal)
                .into_iter()
                .filter(|method| method == "stop_io_for_all_accounts")
                .count()
                .to_string()
                .into(),
        );
        common::record(&lost, "paused", call!("paused").into());
        // Comes back.
        call!("connman", QString::from("online"));
    });

    let back = steps.clone();
    plan.after(2, move || unsafe {
        common::record(&back, "paused-after-return", call!("paused").into());
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    assert_eq!(
        common::value_of(&steps, "load"),
        "ok",
        "the window did not load"
    );
    assert_eq!(
        common::value_of(&steps, "status"),
        "ready",
        "the core was not up yet, so the window had no IO to stop and this \
         test proves nothing"
    );
    assert_eq!(
        common::value_of(&steps, "hurry"),
        "ok",
        "the window holds no network watch"
    );
    assert_eq!(
        common::value_of(&steps, "stops-while-backgrounded"),
        "0",
        "IO was stopped because the app went into the background, which is \
         the one thing it must never be stopped for: the app in the \
         background is the only way a message reaches this platform"
    );
    assert_eq!(
        common::value_of(&steps, "stops-after-loss"),
        "1",
        "the network was gone for longer than a handover and IO was left \
         running, so the core spends the outage reconnecting to nothing \
         and wakes the radio for each attempt"
    );
    assert_eq!(
        common::value_of(&steps, "paused"),
        "yes",
        "IO was stopped without the window remembering it, so nothing \
         would ever start it again"
    );
    assert_eq!(
        common::value_of(&steps, "paused-after-return"),
        "no",
        "the network came back and the window still thinks IO is stopped"
    );

    let methods = common::methods(&journal);
    let stopped = last(&methods, "stop_io_for_all_accounts")
        .expect("IO was never stopped, so the loss was never acted on");
    let started = last(&methods, "start_io_for_all_accounts").expect("IO was never started at all");
    assert!(
        started > stopped,
        "IO was stopped when the network went and never started when it \
         came back, so the phone stops receiving until the app is opened: \
         {methods:?}"
    );
    let hinted =
        last(&methods, "maybe_network").expect("the core was never asked to look at the network");
    assert!(
        hinted > stopped,
        "IO was started again without telling the core to look at the \
         network it now has: {methods:?}"
    );
    assert!(
        first(&methods, "start_io_for_all_accounts").is_some_and(|at| at < stopped),
        "IO was never running before the outage, so nothing here was tested"
    );
}
