//! App's one `Transcriber` follows the transcription settings and MCE's power state. Loaded
//! as shipped with shim types and stub `Nemo.DBus`. Switch stays off: on would fetch model.

// Qt harness: see qml_share.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::path::PathBuf;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}

        function load(url) {{
            loader.setSource(url, {{}})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function state() {{
            var t = loader.item.transcriber
            return [t.objectName, t.enabled, t.languages, t.mode, t.older, t.charging,
                    t.power_saving].join('|')
        }}
        function set(json) {{
            var values = JSON.parse(json)
            for (var key in values) {{ Settings[key] = values[key] }}
            return state()
        }}
        function mce(signalName, value) {{
            var kids = loader.item.children
            for (var i = 0; i < kids.length; i++) {{
                if (kids[i].objectName !== 'powerWatch') {{ continue }}
                var buses = kids[i].resources
                for (var j = 0; j < buses.length; j++) {{
                    if (buses[j].objectName === 'mceSignal') {{
                        buses[j][signalName](value)
                        return state()
                    }}
                }}
            }}
            return 'no-mce'
        }}
    }}
",
        components.display()
    )
}

#[test]
fn the_transcriber_follows_settings_and_power() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    piirit_shim::register_qml_types();
    common::register_dbus_enum();
    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml()));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let url = common::component_url("Transcription.qml");

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
        common::record(&s, "load", call!("load", QString::from(url.clone())));
        common::record(&s, "fresh", call!("state"));
        common::record(
            &s,
            "settings",
            call!(
                "set",
                QString::from(
                    r#"{"transcriptionLanguages": "fi,en", "transcriptionMode": 0,
                        "transcriptionOlder": false}"#
                )
            ),
        );
        common::record(
            &s,
            "always",
            call!("set", QString::from(r#"{"transcriptionMode": 2}"#)),
        );
        common::record(
            &s,
            "plugged",
            call!(
                "mce",
                QString::from("charger_state_ind"),
                QString::from("on")
            ),
        );
        common::record(
            &s,
            "saving",
            call!("mce", QString::from("psm_state_ind"), true),
        );
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    let value = |label| common::value_of(&steps, label);
    assert_eq!(
        value("load"),
        "ok",
        "Transcription.qml did not load: {steps:?}"
    );
    assert_eq!(
        value("fresh"),
        "transcriber|false||1|true|false|false",
        "a fresh install's transcriber is not off with nothing chosen, While charging, older on"
    );
    assert_eq!(
        value("settings"),
        "transcriber|false|fi,en|0|false|false|false",
        "languages, auto-transcribe or older did not reach the transcriber"
    );
    assert_eq!(
        value("always"),
        "transcriber|false|fi,en|2|false|false|false",
        "a changed auto-transcribe choice did not reach the transcriber"
    );
    assert_eq!(
        value("plugged"),
        "transcriber|false|fi,en|2|false|true|false",
        "charger in did not reach the transcriber"
    );
    assert_eq!(
        value("saving"),
        "transcriber|false|fi,en|2|false|true|true",
        "battery saving mode did not reach the transcriber"
    );
}
