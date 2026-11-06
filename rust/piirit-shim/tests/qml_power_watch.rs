//! Charger and battery saving mode from MCE, as auto-transcription reads them. Asked on load
//! and on MCE (re)appearance, then signals only. Loaded as shipped vs stub `Nemo.DBus`.

// Qt harness: see qml_share.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }

        function load(url) {
            loader.setSource(url, {})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function bus(name) {
            var kids = loader.item.resources
            for (var i = 0; i < kids.length; i++) {
                if (kids[i].objectName === name) { return kids[i] }
            }
            return null
        }
        function asked() {
            return bus('mceRequest').called.trim().split('\n').join(',')
        }
        function power() { return loader.item.charging + '|' + loader.item.saving }
        function charger(state) { bus('mceSignal').charger_state_ind(state); return power() }
        function saving(active) { bus('mceSignal').psm_state_ind(active); return power() }
        // MCE gone, then back on bus, answering `value` to every ask.
        function reappear(value) {
            var request = bus('mceRequest')
            request.status = 0
            request.called = ''
            request.reply = value
            request.status = 2
            return asked() + '=' + power()
        }
    }
";

#[test]
fn power_state_follows_mce_without_polling() {
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
    let watch = common::component_url("PowerWatch.qml");

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    let steps: common::Steps = common::Steps::default();
    let mut plan = common::Plan::default();

    let s = steps.clone();
    plan.after(1, move || unsafe {
        common::record(&s, "load", call!("load", QString::from(watch.clone())));
        common::record(&s, "asked", call!("asked"));
        common::record(&s, "unanswered", call!("power"));
        common::record(&s, "plugged", call!("charger", QString::from("on")));
        common::record(&s, "unplugged", call!("charger", QString::from("off")));
        common::record(&s, "unknown", call!("charger", QString::from("unknown")));
        common::record(&s, "saving-on", call!("saving", true));
        common::record(&s, "saving-off", call!("saving", false));
        common::record(&s, "back-on", call!("reappear", QString::from("on")));
        common::record(&s, "back-true", call!("reappear", true));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    let value = |label| common::value_of(&steps, label);
    assert_eq!(value("load"), "ok", "PowerWatch did not load: {steps:?}");
    assert_eq!(
        value("asked"),
        "get_charger_state [],get_psm_state []",
        "MCE was not asked for charger and saving mode on load"
    );
    assert_eq!(
        value("unanswered"),
        "false|false",
        "power state was guessed before MCE answered"
    );
    assert_eq!(value("plugged"), "true|false", "charger in not heard");
    assert_eq!(value("unplugged"), "false|false", "charger out not heard");
    assert_eq!(
        value("unknown"),
        "false|false",
        "unknown charger counted as charging"
    );
    assert_eq!(value("saving-on"), "false|true", "saving mode on not heard");
    assert_eq!(
        value("saving-off"),
        "false|false",
        "saving mode off not heard"
    );
    assert_eq!(
        value("back-on"),
        "get_charger_state [],get_psm_state []=true|false",
        "MCE coming back was not asked again, or its charger answer was misread"
    );
    assert_eq!(
        value("back-true"),
        "get_charger_state [],get_psm_state []=false|true",
        "saving mode answer misread, or a non-\"on\" charger answer counted as charging"
    );
}
