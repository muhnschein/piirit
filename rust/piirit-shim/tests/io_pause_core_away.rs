//! Network loss core missed still stops IO once core is back.
//!
//! `io_pause.rs` is the ordinary order. Here core dies and restarts while network goes. Restarted
//! core would run IO on dead network: the outage the stop prevents.
//!
//! Required: window stop still asked; shim forgets IO; restarted core starts none.
//!
//! Steps hang on core's own restarts, not the clock. Fixed reads raced spawn time: each restart
//! lands a little later under load (coverage), and `single_shot` drops sub-seconds, so a read meant
//! for 6.5 s ran at 6 s, beside the edge of the away window, and saw `ready`.

// Qt harness: see qml_share.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use std::path::PathBuf;

mod common;

/// Silica's `pageStack`; see `network_hint.rs`. Never navigated.
#[derive(QObject, Default)]
struct StackProbe {
    base: qt_base_class!(trait QObject),
}

/// Fake core's journal, read from QML at the moment a step happens.
#[derive(QObject, Default)]
struct JournalProbe {
    base: qt_base_class!(trait QObject),
    path: PathBuf,
    /// How many times core was asked to start IO for all accounts.
    starts: qt_method!(fn(&self) -> QString),
}

impl JournalProbe {
    fn starts(&self) -> QString {
        common::methods(&self.path)
            .into_iter()
            .filter(|method| method == "start_io_for_all_accounts")
            .count()
            .to_string()
            .into()
    }
}

/// Every server dies 300 ms after start; shim waits 1, 2, 4, 8 s before each replacement. Network
/// goes as core goes away the third time: 4 s away, ten times the 400 ms loss wait, so loss lands
/// while core is away. Fourth away: the server that came back mid-outage has lived and died, so
/// any IO it was asked for is in the journal.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: probe
        Loader { id: window }

        /// Times core went away.
        property int aways: 0
        /// Network gone; window not yet seen stopping IO.
        property bool waitingForPause: false
        /// Labelled readings, each taken when what it waits for happens.
        property var readings: ({})

        function note(label, value) { readings[label] = '' + value }
        function reading(label) {
            return readings[label] !== undefined ? readings[label] : '<step did not run>'
        }
        function finished() { return readings['starts-after-restart'] !== undefined ? 'yes' : 'no' }

        function loadWindow(url) {
            window.setSource(url, {})
            return window.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function paused() { return window.item.ioPaused ? 'yes' : 'no' }

        Connections {
            target: core
            // Qt 5.6 handler syntax; see WelcomePage.qml.
            onStatus_changed: {
                if (core.status !== 'reconnecting') { return }
                probe.aways += 1
                if (probe.aways === 3) {
                    probe.loseNetwork()
                } else if (probe.aways === 4) {
                    probe.afterRestart()
                }
            }
        }

        function loseNetwork() {
            var watch = findIn(window.item, 'networkWatch')
            if (!watch) { note('hurry', 'missing:networkWatch'); return }
            // See io_pause.rs: the real wait is half a minute.
            watch.lostMs = 400
            note('hurry', 'ok')
            note('starts-before-loss', journal.starts())
            watch.heard('State', 'offline')
            waitingForPause = true
        }

        Timer {
            interval: 50; repeat: true; running: probe.waitingForPause
            onTriggered: {
                if (!window.item.ioPaused) { return }
                probe.waitingForPause = false
                note('status-after-loss', core.status)
                note('paused-after-loss', 'yes')
                note('starts-after-loss', journal.starts())
            }
        }

        function afterRestart() {
            if (waitingForPause) {
                waitingForPause = false
                note('paused-after-loss', 'no')
            }
            note('paused-after-restart', paused())
            note('starts-after-restart', journal.starts())
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

/// Readings the probe takes, copied out before quitting.
const READINGS: [&str; 7] = [
    "hurry",
    "starts-before-loss",
    "status-after-loss",
    "paused-after-loss",
    "starts-after-loss",
    "paused-after-restart",
    "starts-after-restart",
];

/// Checks, a second apart, before readings taken anyway. Fourth away due ~9 s; slack for a slow
/// runner, under nextest's 30 s warning.
const PATIENCE: u32 = 24;

#[test]
#[allow(clippy::too_many_lines)]
fn a_loss_the_core_was_away_for_still_stops_io_when_it_is_back() {
    let temp = std::env::temp_dir().join(format!("piirit-io-pause-away-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // `EnterKey` has no stub; window reaches pages using it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded; set before Qt starts and before server inherits env.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Every server dies shortly after start. Same clock as `qml_core_gone.rs`.
        std::env::set_var("PIIRIT_FAKE_EXIT_AFTER_MS", "300");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(StackProbe::default());
    let journal_box = QObjectBox::new(JournalProbe {
        path: journal,
        ..JournalProbe::default()
    });
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    common::register_dbus_enum();
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.set_object_property("journal".into(), journal_box.pinned());
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

    // Window starts core; the rest runs off core's restarts, in the probe.
    let loading = steps.clone();
    plan.after(1, move || unsafe {
        common::record(
            &loading,
            "load",
            call!("loadWindow", QString::from(window.clone())).into(),
        );
    });

    // Once a second: done, or out of time.
    let done = steps.clone();
    plan.until(1, PATIENCE, move |last| unsafe {
        if !last && call!("finished") != "yes" {
            return false;
        }
        if !done.borrow().iter().any(|(label, _)| label == "hurry") {
            for label in READINGS {
                common::record(&done, label, call!("reading", QString::from(label)).into());
            }
        }
        (*engine_ptr).quit();
        true
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
        common::value_of(&steps, "hurry"),
        "ok",
        "the window holds no network watch, or the core never went away a \
         third time"
    );
    assert_eq!(
        common::value_of(&steps, "paused-after-loss"),
        "yes",
        "the loss was announced while the core was away and the window \
         did nothing with it, so the core that comes back will resume IO \
         against a network that is not there"
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
