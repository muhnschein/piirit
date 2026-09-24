//! Coming back to the app asks the core to look at the network again.
//!
//! A connection killed by a change of network -- wi-fi to mobile data on
//! the way out of the house -- dies silently: nothing arrives on it and
//! nothing says so. The core finds out when its IDLE times out five
//! minutes later, and until then the phone is holding a socket to nowhere.
//!
//! The window asks on the way back in, which is both when the reader is
//! watching for a message and when the phone has most likely moved between
//! networks in a pocket. Nothing is asked on the way out: the app carries
//! on receiving in the background, which on this platform is the only way
//! a message arrives at all.
//!
//! The other way round matters more, and is here too: connman says the
//! network changed while the app is off screen, and the core is asked then
//! and there. That is the path a notification arrives by when nobody is
//! looking at the phone. What the component makes of connman's own words
//! is `qml_network_watch`; this is that it reaches the core.

// Qt harness: see qml_share.rs.
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

mod common;

/// Silica's `pageStack`, which the window binds a `PendingNavigation` to
/// and this test never makes it use. Present because a binding to nothing
/// is a `ReferenceError`, not because anything here is navigated.
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
    // `EnterKey` has no stub; the window reaches pages that use it.
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
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

    let mut loaded = String::new();
    let loaded_ptr: *mut String = std::ptr::addr_of_mut!(loaded);
    single_shot(Duration::from_secs(1), move || unsafe {
        *loaded_ptr = call!("loadWindow", QString::from(window.clone()));
    });

    // Switched away from, with the core up: nothing is asked here.
    let mut status = String::new();
    let status_ptr: *mut String = std::ptr::addr_of_mut!(status);
    let mut while_away = 0usize;
    let away_ptr: *mut usize = std::ptr::addr_of_mut!(while_away);
    let away_journal = journal.clone();
    let mut reached = String::new();
    let reached_ptr: *mut String = std::ptr::addr_of_mut!(reached);
    single_shot(Duration::from_secs(4), move || unsafe {
        *status_ptr = call!("coreStatus");
        call!("away");
        *away_ptr = common::methods(&away_journal)
            .into_iter()
            .filter(|method| method == "maybe_network")
            .count();
        // Off screen, and the phone changes network under the app.
        *reached_ptr = call!("connman", QString::from("online"));
    });

    // The watch waits to see whether more announcements are coming
    // (`NetworkWatch.settleMs`), so this reads after that wait.
    let mut while_off_screen = 0usize;
    let off_screen_ptr: *mut usize = std::ptr::addr_of_mut!(while_off_screen);
    let off_screen_journal = journal.clone();
    single_shot(Duration::from_secs(7), move || unsafe {
        *off_screen_ptr = common::methods(&off_screen_journal)
            .into_iter()
            .filter(|method| method == "maybe_network")
            .count();
        call!("back");
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        (*engine_ptr).quit();
    });

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
