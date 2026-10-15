//! App's reaction to connman. Arrival at working network: `maybe_network` (dead socket
//! may need replacing), after short wait to merge handover announcements into one change. Loss
//! stops IO, costly if wrong: waits much longer, so no handover looks like an outage. Brief loss
//! never announced. Loaded as shipped
//! vs stub `Nemo.DBus`.

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
        property int losses: 0
        Loader { id: loader }

        function load(url) {
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.networkChanged.connect(function () { hints++ })
            loader.item.networkLost.connect(function () { losses++ })
            // Half a minute is the real wait and the point of it; a test
            // that sat through it would be a test nobody runs.
            loader.item.lostMs = 400
            return 'ok'
        }
        // What connman announced, as the interface hands it over.
        function say(value) { loader.item.heard('State', value); return 'ok' }
        // Something else about connman changed. Not the connection.
        function sayOther() { loader.item.heard('OfflineMode', false); return 'ok' }
        function count() { return hints + '|' + losses }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_handover_counts_once_and_only_an_outage_that_lasts_is_called_one() {
    // SAFETY: single thread; pre-Qt.
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

    // One handover: old connection goes, new up, reaches internet. Three announcements,
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
        // Nothing yet: waiting to see if more comes.
        common::record(&loading, "during", call!("count").into());
    });

    let settled = steps.clone();
    single_shot(Duration::from_secs(3), move || unsafe {
        common::record(&settled, "after-handover", call!("count").into());
        // Network goes; connman changes kind of nothing: must not delay announcement.
        call!("say", QString::from("offline"));
        call!("say", QString::from("idle"));
        // Non-connection connman news.
        call!("sayOther");
    });

    let lost = steps.clone();
    single_shot(Duration::from_secs(5), move || unsafe {
        common::record(&lost, "after-loss", call!("count").into());
        // Back, then connman repeats itself.
        call!("say", QString::from("online"));
        call!("say", QString::from("online"));
    });

    let back = steps.clone();
    single_shot(Duration::from_millis(7000), move || unsafe {
        common::record(&back, "after-return", call!("count").into());
        // Blink: gone and back inside wait. Handover looks like this; nothing should follow.
        call!("say", QString::from("offline"));
    });

    single_shot(Duration::from_millis(7150), move || unsafe {
        call!("say", QString::from("online"));
    });

    let blinked = steps.clone();
    single_shot(Duration::from_millis(9500), move || unsafe {
        common::record(&blinked, "after-blink", call!("count").into());
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
        "0|0",
        "the first announcement of a handover was passed on straight away, \
         so one change of network is three asks of the core"
    );
    assert_eq!(
        common::value_of(&steps, "after-handover"),
        "1|0",
        "a handover was not passed on as exactly one change, or the `idle` \
         it passes through was called an outage"
    );
    assert_eq!(
        common::value_of(&steps, "after-loss"),
        "1|1",
        "losing the network was passed on as though it had come back -- the \
         core would try to reconnect to nothing -- or connman changing its \
         mind between `offline` and `idle` put the announcement off again"
    );
    assert_eq!(
        common::value_of(&steps, "after-return"),
        "2|1",
        "coming back was not passed on, or connman repeating itself was \
         counted twice"
    );
    assert_eq!(
        common::value_of(&steps, "after-blink"),
        "3|1",
        "a network gone and back inside the wait was called an outage, so \
         an ordinary handover would stop the app receiving"
    );
}
