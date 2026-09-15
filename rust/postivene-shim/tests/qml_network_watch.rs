//! What the app does with connman's account of the network.
//!
//! The rule is the core's own: `maybe_network` says the network may have
//! come back, so arriving at one that works is worth passing on and losing
//! one is not -- the core has nothing to reconnect to, and an attempt that
//! cannot succeed is a wasted one. A single handover is several
//! announcements, so they are taken as the one change they are.
//!
//! The component is loaded as shipped, against the stub `Nemo.DBus`: what
//! a device's bus would deliver arrives here as the call the interface
//! makes when it hears connman.

// Qt harness: see qml_share.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property int hints: 0
        Loader { id: loader }

        function load(url) {
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.networkChanged.connect(function () { hints++ })
            return 'ok'
        }
        // What connman announced, as the interface hands it over.
        function say(value) { loader.item.heard('State', value); return 'ok' }
        // Something else about connman changed. Not the connection.
        function sayOther() { loader.item.heard('OfflineMode', false); return 'ok' }
        function count() { return '' + hints }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn only_arriving_at_a_working_network_is_passed_on_and_a_handover_counts_once() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    common::register_dbus_enum();
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let watch = common::component_url("NetworkWatch.qml");

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

    // One handover, as connman describes it: the old connection goes, the
    // new one comes up, and it reaches the internet. Three announcements,
    // one change.
    let loading = steps.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        common::record(
            &loading,
            "load",
            call!("load", QString::from(watch.clone())).into(),
        );
        call!("say", QString::from("idle"));
        call!("say", QString::from("ready"));
        call!("say", QString::from("online"));
        // Nothing yet: the component waits to see whether more is coming.
        common::record(&loading, "during", call!("count").into());
    });

    let settled = steps.clone();
    single_shot(Duration::from_secs(3), move || unsafe {
        common::record(&settled, "after-handover", call!("count").into());
        // Losing the network, and a property that is not the connection.
        call!("say", QString::from("offline"));
        call!("say", QString::from("idle"));
        call!("sayOther");
    });

    let lost = steps.clone();
    single_shot(Duration::from_secs(5), move || unsafe {
        common::record(&lost, "after-loss", call!("count").into());
        // Back, and then connman repeating itself.
        call!("say", QString::from("online"));
        call!("say", QString::from("online"));
    });

    let back = steps.clone();
    single_shot(Duration::from_secs(7), move || unsafe {
        common::record(&back, "after-return", call!("count").into());
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    assert_eq!(
        common::value_of(&steps, "load"),
        "ok",
        "the component did not load against the stub bus"
    );
    assert_eq!(
        common::value_of(&steps, "during"),
        "0",
        "the first announcement of a handover was passed on straight away, \
         so one change of network is three asks of the core"
    );
    assert_eq!(
        common::value_of(&steps, "after-handover"),
        "1",
        "a handover was not passed on as exactly one change"
    );
    assert_eq!(
        common::value_of(&steps, "after-loss"),
        "1",
        "losing the network was passed on as though it had come back: the \
         core would try to reconnect to nothing"
    );
    assert_eq!(
        common::value_of(&steps, "after-return"),
        "2",
        "coming back was not passed on, or connman repeating itself was \
         counted twice"
    );
}
