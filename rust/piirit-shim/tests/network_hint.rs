//! Coming back to app asks core to re-check network.
//!
//! Connection killed by network change dies silently; core learns only when IDLE times out, five
//! minutes on.
//!
//! Window asks on the way back in: reader watching, phone likely moved networks. Nothing asked on
//! way out: app keeps receiving in background, the only way messages arrive.
//!
//! More important: connman says network changed while app off screen; core asked then. That's how a
//! notification arrives. Component's reading of connman: `qml_network_watch`; here: it reaches
//! core.

// Qt harness: see qml_share.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, bound by window to `PendingNavigation`; never navigated. Needed:
/// binding to nothing is `ReferenceError`.
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
        // Switching away from the app, and coming back to it. The real
        // thing is `Qt.application.state`, which a headless test cannot
        // move; the property the window watches is the same one either
        // way round.
        function away() { window.item.appActive = false; return 'ok' }
        function back() { window.item.appActive = true; return 'ok' }
        function coreStatus() { return '' + core.status }
        // connman, as the window's own watch hears it. Found the way
        // anything in the window is found, by the name it carries.
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

#[test]
#[allow(clippy::too_many_lines)]
fn the_core_is_asked_to_reconnect_when_the_app_comes_back_to_the_front() {
    let temp = std::env::temp_dir().join(format!("piirit-network-hint-{}", std::process::id()));
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

    let mut plan = common::Plan::default();

    let mut loaded = String::new();
    let loaded_ptr: *mut String = std::ptr::addr_of_mut!(loaded);
    plan.after(1, move || unsafe {
        *loaded_ptr = call!("loadWindow", QString::from(window.clone()));
    });

    // Switched away, core up: nothing asked.
    let mut status = String::new();
    let status_ptr: *mut String = std::ptr::addr_of_mut!(status);
    let mut while_away = 0usize;
    let away_ptr: *mut usize = std::ptr::addr_of_mut!(while_away);
    let away_journal = journal.clone();
    let mut reached = String::new();
    let reached_ptr: *mut String = std::ptr::addr_of_mut!(reached);
    plan.after(3, move || unsafe {
        *status_ptr = call!("coreStatus");
        call!("away");
        *away_ptr = common::methods(&away_journal)
            .into_iter()
            .filter(|method| method == "maybe_network")
            .count();
        // Off screen; phone changes network under app.
        *reached_ptr = call!("connman", QString::from("online"));
    });

    // Watch waits for more announcements (`NetworkWatch.settleMs`); read after that wait.
    let mut while_off_screen = 0usize;
    let off_screen_ptr: *mut usize = std::ptr::addr_of_mut!(while_off_screen);
    let off_screen_journal = journal.clone();
    plan.after(3, move || unsafe {
        *off_screen_ptr = common::methods(&off_screen_journal)
            .into_iter()
            .filter(|method| method == "maybe_network")
            .count();
        call!("back");
    });

    plan.after(2, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    assert_eq!(loaded, "ok", "the window did not load");
    assert_eq!(
        status, "ready",
        "the core was not up yet, so the window had nothing to ask and \
         this test proves nothing"
    );
    assert_eq!(
        while_away, 0,
        "the app asked the core to reconsider the network while it was \
         being switched away from, which is not when a reader is waiting"
    );
    assert_eq!(
        reached, "ok",
        "the window holds no network watch, so nothing in it is listening \
         to the phone's own account of the network"
    );
    assert_eq!(
        while_off_screen, 1,
        "connman said the network had changed while the app was off \
         screen and the core was never told, so a message sent to a phone \
         in a pocket waits for the core's own five-minute IDLE timeout"
    );

    let hints = common::methods(&journal)
        .into_iter()
        .filter(|method| method == "maybe_network")
        .count();
    assert!(
        hints > while_off_screen,
        "coming back to the app never asked the core to look at the \
         network again, so a connection killed in a pocket stays dead \
         until the core's own five-minute IDLE times out"
    );
}
