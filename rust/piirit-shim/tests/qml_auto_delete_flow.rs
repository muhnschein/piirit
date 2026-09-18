//! Picking a deletion period, from the tap to the core.
//!
//! The settings page does not write the period on the tap. It asks the
//! core how many messages are already older than it, puts that number
//! to the reader on a page of its own, and writes the setting only when
//! that page is accepted -- after which the core object writes it to
//! every account, as the app's root binds it to. This drives the whole
//! of that against the fake core, with a page stack that hands the page
//! a dialog to connect to and lets the test accept it.

// Qt harness: see qml_pages.rs.
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
use serde_json::Value;

mod common;

/// The probe imports the app's components by absolute URL, for the
/// `Settings` singleton the page writes -- and binds the core's period
/// to it, as the app's root does, so a setting written reaches the core.
///
/// The page stack is a QML object handed to the page as a context
/// property, rather than a Rust one: an object a method hands to QML is
/// QML's to delete, and one owned from Rust as well was deleted twice.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}
        Binding {{
            target: core
            property: 'delete_device_after'
            value: Settings.deleteDeviceAfter
        }}
        // An account to count in and to write to: the fake core starts
        // with none, and the list is read once one exists, the way the
        // welcome page reads it.
        Timer {{
            id: poll
            interval: 200; running: true; repeat: true
            onTriggered: {{
                if (core.status === 'ready') {{
                    poll.running = false
                    core.add_account()
                }}
            }}
        }}
        Connections {{
            target: core
            onAccount_added: core.refresh_accounts()
        }}
        // Silica's page stack, as far as the page uses it: push records
        // what was pushed and hands back a dialog for the page to
        // connect to, the way Silica hands back the page it made.
        QtObject {{
            id: stack
            property string pushed: ''
            property QtObject dialog: QtObject {{
                signal accepted()
                signal rejected()
                function accept() {{ accepted() }}
            }}
            function push(url, props) {{
                var name = ('' + url).split('/').pop()
                pushed = name + ':count=' + props.count + ',seconds=' + props.seconds
                         + ',periodLabel=' + props.periodLabel
                return dialog
            }}
        }}
        function stackObject() {{ return stack }}
        function acceptDialog() {{ stack.dialog.accept(); return 'ok' }}
        function load(url) {{
            loader.setSource(url, {{}})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function findIn(node, name) {{
            if (!node) {{ return null }}
            if (node.objectName === name) {{ return node }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findIn(kids[i], name)
                if (hit) {{ return hit }}
            }}
            if (node.contentItem && node.contentItem !== node) {{
                return findIn(node.contentItem, name)
            }}
            if (node.menu) {{
                var inMenu = findIn(node.menu, name)
                if (inMenu) {{ return inMenu }}
            }}
            return null
        }}
        function get(name, property) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            return '' + item[property]
        }}
        function click(name) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            item.clicked()
            return 'ok'
        }}
        function period() {{ return '' + Settings.deleteDeviceAfter }}
        function pushed() {{ return stack.pushed }}
    }}
",
        components.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_period_is_counted_confirmed_and_then_written_to_every_account() {
    let temp = std::env::temp_dir().join(format!("piirit-auto-delete-flow-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml()));
    // Named for the page before it is loaded, as Silica names its own.
    let stack = engine.invoke_method("stackObject".into(), &[]);
    engine.set_property("pageStack".into(), stack);

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // By now the core is up and the account read, as the app has done
    // on its way in before the page can be opened.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::page_url("SettingsPage.qml")))
        );
        record!("pick", call!("click", QString::from("deletionOption3600")));
        // Straight after the tap: nothing written, nothing pushed yet.
        record!("period-on-tap", call!("period"));
        record!("pushed-on-tap", call!("pushed"));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("pushed", call!("pushed"));
        record!("period-before-accept", call!("period"));
        record!(
            "shown-before-accept",
            call!(
                "get",
                QString::from("deletionCombo"),
                QString::from("currentIndex")
            )
        );
        record!("accept", call!("acceptDialog"));
        record!("period-on-accept", call!("period"));
        record!(
            "shown-on-accept",
            call!(
                "get",
                QString::from("deletionCombo"),
                QString::from("currentIndex")
            )
        );
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, calls: {names:?}");

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(value("pick"), "ok", "the period was not picked. {context}");
    assert_eq!(
        (value("period-on-tap"), value("pushed-on-tap")),
        ("0".to_string(), String::new()),
        "the tap wrote the setting or opened the dialog before the core had \
         been asked. {context}"
    );
    assert_eq!(
        value("pushed"),
        "AutoDeleteDialog.qml:count=4,seconds=3600,periodLabel=After 1 hour",
        "the dialog was not opened with the core's count and the period. \
         {context}"
    );
    assert_eq!(
        (value("period-before-accept"), value("shown-before-accept")),
        ("0".to_string(), "0".to_string()),
        "the setting was written, or the choice moved, before the reader \
         had agreed. {context}"
    );
    assert_eq!(
        (value("period-on-accept"), value("shown-on-accept")),
        ("3600".to_string(), "1".to_string()),
        "accepting the dialog did not write the setting and move the \
         choice to it. {context}"
    );
    let written: Vec<Value> = calls
        .iter()
        .filter(|(method, params)| {
            method == "set_config"
                && params.pointer("/1").and_then(Value::as_str) == Some("delete_device_after")
        })
        .map(|(_, params)| params.clone())
        .collect();
    // The default, when the account list was first read, and the
    // reader's choice once they had agreed -- and nothing in between.
    assert_eq!(
        written,
        vec![
            serde_json::json!([1, "delete_device_after", "0"]),
            serde_json::json!([1, "delete_device_after", "3600"])
        ],
        "the period did not reach the core's account exactly when the \
         reader had agreed: {calls:?}"
    );
}
