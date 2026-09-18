//! A loss of network that the core was not there to hear about still
//! stops IO once it is.
//!
//! `io_pause.rs` is the ordinary case: the core is up, the network goes,
//! IO stops. This is the other order. The core dies and is being restarted
//! when the network goes; the loss is announced to a window whose core is
//! away, and the shim resumes whatever IO was asked for under every core it
//! restarts. Left alone, that core comes back with IO running against a
//! network that is still not there and stays that way until connman next
//! changes its mind -- the outage the stop exists to prevent, in full.
//!
//! What has to happen instead: the window asks for the stop anyway, the
//! shim forgets the IO it was asked for, and the restarted core starts
//! none.

// Qt harness: see qml_share.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`; see `network_hint.rs`. Nothing here navigates.
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
        // See io_pause.rs: the real wait is half a minute.
        function hurry() {
            var watch = findIn(window.item, 'networkWatch')
            if (!watch) { return 'missing:networkWatch' }
            watch.lostMs = 400
            return 'ok'
        }
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

/// How many times the core has been asked `name` so far.
fn count(journal: &std::path::Path, name: &str) -> String {
    common::methods(journal)
        .into_iter()
        .filter(|method| method == name)
        .count()
        .to_string()
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_loss_the_core_was_away_for_still_stops_io_when_it_is_back() {
    let temp = std::env::temp_dir().join(format!("piiri-io-pause-away-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; the window reaches pages that use it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        // Every server, including each replacement, dies a moment after it
        // starts. The window's restarts then come one, two and four
        // seconds apart, and the loss below is announced inside the
        // four-second gap: after the third replacement has died, before
        // the fourth is spawned. See `qml_core_gone.rs` for the same clock.
        std::env::set_var("PIIRI_FAKE_EXIT_AFTER_MS", "300");
    }

    piiri_shim::register_qml_types();

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
    let window = format!("file://{}", tree.join("piiri.qml").display());

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

    // t=1: the first server is spawned and IO is asked for. Each server
    // dies a moment after it is up; the replacements come up at about
    // 2.8, 4.8 and 9 seconds, so from about 5.1 to 9 the core is away.
    let loading = steps.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        common::record(
            &loading,
            "load",
            call!("loadWindow", QString::from(window.clone())).into(),
        );
    });

    // t=6.5: the core is away. The network goes, and stays gone.
    let away = steps.clone();
    let away_journal = journal.clone();
    single_shot(Duration::from_millis(6500), move || unsafe {
        common::record(&away, "status-at-loss", call!("coreStatus").into());
        common::record(&away, "hurry", call!("hurry").into());
        common::record(
            &away,
            "starts-before-loss",
            count(&away_journal, "start_io_for_all_accounts").into(),
        );
        call!("connman", QString::from("offline"));
    });

    // t=7.8: the loss has been announced (at about 6.9) to a window
    // whose core is still away.
    let heard = steps.clone();
    let heard_journal = journal.clone();
    single_shot(Duration::from_millis(7800), move || unsafe {
        common::record(&heard, "status-after-loss", call!("coreStatus").into());
        common::record(&heard, "paused-after-loss", call!("paused").into());
        common::record(
            &heard,
            "starts-after-loss",
            count(&heard_journal, "start_io_for_all_accounts").into(),
        );
    });

    // t=10.5: the fourth server came up at about 9 -- and must not have
    // resumed IO.
    let back = steps.clone();
    let back_journal = journal.clone();
    single_shot(Duration::from_millis(10_500), move || unsafe {
        common::record(&back, "paused-after-restart", call!("paused").into());
        common::record(
            &back,
            "starts-after-restart",
            count(&back_journal, "start_io_for_all_accounts").into(),
        );
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    assert_eq!(
        common::value_of(&steps, "load"),
        "ok",
        "the window did not load"
    );
    assert_eq!(
        common::value_of(&steps, "hurry"),
        "ok",
        "the window holds no network watch"
    );
    assert_eq!(
        common::value_of(&steps, "status-at-loss"),
        "reconnecting",
        "the core was not away when the network went, so this is the case \
         io_pause.rs already covers and proves nothing about a core that \
         missed the loss"
    );
    assert_eq!(
        common::value_of(&steps, "status-after-loss"),
        "reconnecting",
        "the core came back before the loss was announced, so the loss was \
         not one it missed"
    );
    let before: usize = common::value_of(&steps, "starts-before-loss")
        .parse()
        .expect("a count");
    assert!(
        before >= 2,
        "IO was not resumed under a restarted core before the outage, so \
         nothing here shows that a restart resumes it: {before} start(s)"
    );
    assert_eq!(
        common::value_of(&steps, "starts-after-loss"),
        before.to_string(),
        "IO was started between the loss and the restart, by nothing that \
         should have been running"
    );
    assert_eq!(
        common::value_of(&steps, "paused-after-loss"),
        "yes",
        "the loss was announced while the core was away and the window \
         did nothing with it, so the core that comes back will resume IO \
         against a network that is not there"
    );
    assert_eq!(
        common::value_of(&steps, "paused-after-restart"),
        "yes",
        "the restart undid the pause"
    );
    assert_eq!(
        common::value_of(&steps, "starts-after-restart"),
        before.to_string(),
        "the core came back mid-outage and resumed IO against a network \
         that is not there; the stop the window asked for while it was \
         away did not make the shim forget the IO it had been asked for"
    );
}
