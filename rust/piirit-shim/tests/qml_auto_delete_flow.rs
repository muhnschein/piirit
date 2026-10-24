//! Picking a deletion period, from tap to core.
//!
//! Settings page writes nothing on tap: asks core how many messages are older, shows that count on
//! its own page, writes setting only on accept. Core object then writes it to every account. Test
//! accepts the page's dialog.
//!
//! Six periods: Silica lists them on a page of their own, popping as the count lands. Stack refuses
//! a push mid-transition, so the dialog waits for the page back on screen, stack idle.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Imports app components by absolute URL, for `Settings` singleton page writes. Binds core's
/// period to it, as app root does, so a written setting reaches core.
///
/// Page stack is a QML object passed as context property, not a Rust one: QML deletes what a
/// method hands it, and one also owned from Rust was deleted twice.
#[allow(clippy::too_many_lines)]
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
        // Each step waits on what it needs: the account list read, then
        // each count. Count step a turn later, after the page's handler.
        property int round: 0
        property bool finished: false
        Connections {{
            target: core
            onAccount_added: core.refresh_accounts()
            onAccounts_refreshed: {{
                if (round === 0) {{
                    round = 1
                    pickFirst()
                }}
            }}
            onAuto_deletion_estimated: counted.restart()
        }}
        Timer {{
            id: counted
            interval: 0
            onTriggered: round === 1 ? firstCounted() : secondCounted()
        }}
        // Silica's page stack, as far as the page uses it: push records
        // what was pushed and hands back a dialog for the page to
        // connect to, the way Silica hands back the page it made. Busy
        // while Silica's page of choices pops; Silica drops a push then.
        QtObject {{
            id: stack
            property string pushed: ''
            property bool busy: false
            property QtObject dialog: QtObject {{
                signal accepted()
                signal rejected()
                function accept() {{ accepted() }}
            }}
            function push(url, props) {{
                if (busy) {{ return null }}
                var name = ('' + url).split('/').pop()
                pushed = name + ':count=' + props.count + ',seconds=' + props.seconds
                         + ',periodLabel=' + props.periodLabel
                return dialog
            }}
        }}
        function stackObject() {{ return stack }}
        property var steps: []
        function record(label, value) {{ steps.push([label, '' + value]) }}
        function stepsJson() {{ return JSON.stringify(steps) }}
        // The page under Silica's page of choices, or back on screen.
        function setActive(active) {{
            loader.item.status = active ? PageStatus.Active : PageStatus.Inactive
        }}
        // Picked on Silica's page of choices, which pops at once.
        function pick(seconds) {{
            stack.pushed = ''
            setActive(false)
            stack.busy = true
            return click('deletionOption' + seconds)
        }}
        function pickFirst() {{
            loader.setSource('{}', {{}})
            record('load', loader.status === Loader.Ready ? 'ok' : 'load-failed')
            record('pick', pick(3600))
            // Straight after tap: nothing written or pushed.
            record('period-on-tap', period())
            record('pushed-on-tap', stack.pushed)
        }}
        // Count in; the pop still going, then the page not yet on screen.
        function firstCounted() {{
            record('pushed-mid-transition', stack.pushed)
            stack.busy = false
            record('pushed-under-choices', stack.pushed)
            setActive(true)
            record('pushed', stack.pushed)
            record('period-before-accept', period())
            record('shown-before-accept', get('deletionCombo', 'currentIndex'))
            stack.dialog.accept()
            record('period-on-accept', period())
            record('shown-on-accept', get('deletionCombo', 'currentIndex'))
            round = 2
            record('pick-again', pick(86400))
        }}
        // Again, the page back on screen before the stack goes idle:
        // Silica's order is either.
        function secondCounted() {{
            setActive(true)
            record('pushed-again-mid-transition', stack.pushed)
            stack.busy = false
            record('pushed-again', stack.pushed)
            stack.dialog.accept()
            record('period-on-accept-again', period())
            // Answered: the stack's next move asks nothing.
            stack.pushed = ''
            stack.busy = true
            stack.busy = false
            record('pushed-once-answered', stack.pushed)
            finished = true
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
        function isFinished() {{ return finished ? 'true' : 'false' }}
    }}
",
        components.display(),
        common::page_url("SettingsPage.qml")
    )
}

/// Ticks to wait for the steps and the last write; only a broken run spends them.
const PATIENCE_SECONDS: u32 = 30;

#[test]
#[allow(clippy::too_many_lines)]
fn a_period_is_counted_confirmed_and_then_written_to_every_account() {
    let temp = std::env::temp_dir().join(format!("piirit-auto-delete-flow-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
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
    // Named for the page before load, as Silica names its own.
    let stack = engine.invoke_method("stackObject".into(), &[]);
    engine.set_property("pageStack".into(), stack);

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps_json = String::new();
    let steps_ptr: *mut String = std::ptr::addr_of_mut!(steps_json);
    wait_for_steps(engine_ptr, steps_ptr, journal.clone(), PATIENCE_SECONDS);

    engine.exec();

    let steps: Vec<(String, String)> = serde_json::from_str(&steps_json).unwrap_or_default();
    let value = |label: &str| common::value_of(&steps, label).to_string();
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
        (
            value("pushed-mid-transition"),
            value("pushed-under-choices")
        ),
        (String::new(), String::new()),
        "the dialog was pushed while Silica's page of choices was still \
         leaving, when Silica drops it. {context}"
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
    assert_eq!(
        (value("pick-again"), value("pushed-again-mid-transition")),
        ("ok".to_string(), String::new()),
        "picked again, the dialog was pushed while the stack was still \
         moving. {context}"
    );
    assert!(
        value("pushed-again").starts_with("AutoDeleteDialog.qml:count=")
            && value("pushed-again").ends_with(",seconds=86400,periodLabel=After 1 day"),
        "picked again, the dialog did not open once the stack was idle. \
         {context}"
    );
    assert_eq!(
        value("period-on-accept-again"),
        "86400",
        "accepting the second dialog did not write the setting. {context}"
    );
    assert_eq!(
        value("pushed-once-answered"),
        "",
        "the dialog was pushed again on the stack's next move. {context}"
    );
    let written: Vec<Value> = calls
        .iter()
        .filter(|(method, params)| {
            method == "set_config"
                && params.pointer("/1").and_then(Value::as_str) == Some("delete_device_after")
        })
        .map(|(_, params)| params.clone())
        .collect();
    // The default, when the account list was first read, and each of the
    // reader's choices once they had agreed -- and nothing in between.
    assert_eq!(
        written,
        vec![
            serde_json::json!([1, "delete_device_after", "0"]),
            serde_json::json!([1, "delete_device_after", "3600"]),
            serde_json::json!([1, "delete_device_after", "86400"])
        ],
        "the period did not reach the core's account exactly when the \
         reader had agreed: {calls:?}"
    );
}

/// Whether the reader's second choice has reached the core's account.
fn second_choice_written(journal: &Path) -> bool {
    common::calls(journal).iter().any(|(method, params)| {
        method == "set_config" && *params == serde_json::json!([1, "delete_device_after", "86400"])
    })
}

/// Each second: probe's steps done and last write landed, or patience spent? Then take the
/// steps and quit. Whole seconds: vendored `single_shot` drops fraction.
fn wait_for_steps(engine: *mut QmlEngine, steps: *mut String, journal: PathBuf, patience: u32) {
    single_shot(Duration::from_secs(1), move || unsafe {
        let finished = QString::from_qvariant((*engine).invoke_method("isFinished".into(), &[]))
            .map(|value| value.to_string())
            .unwrap_or_default();
        if (finished == "true" && second_choice_written(&journal)) || patience == 0 {
            *steps = QString::from_qvariant((*engine).invoke_method("stepsJson".into(), &[]))
                .map(|value| value.to_string())
                .unwrap_or_default();
            (*engine).quit();
        } else {
            wait_for_steps(engine, steps, journal.clone(), patience - 1);
        }
    });
}
