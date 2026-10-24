//! Muting from every menu offering it, tap to core.
//!
//! Chat list row menu, group page pull-down, contact page pull-down: Mute opens the length dialog,
//! sends nothing until a length comes back (each entry picks another, so none stands in for
//! another); Unmute sends at once, no dialog. Each entry then reads core's answer back (label
//! flips). Dialog itself: `qml_mute_dialog.rs`.

// Qt harness: see qml_pages.rs.
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
use serde_json::{json, Value};

mod common;

/// Steps run in order, each once its condition holds (polled; core answers async).
///
/// Page stack is a QML object (context property), not Rust: as `qml_blocking`. Fresh dialog per
/// push: a pick can't reach handler an earlier page connected.
#[allow(clippy::too_many_lines)]
fn probe_qml() -> String {
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {{
        Loader {{ id: loader }}

        Component {{
            id: dialogs
            QtObject {{ signal picked(int seconds) }}
        }}

        QtObject {{
            id: stack
            property string pushed: ''
            property bool busy: false
            property QtObject dialog: null
            function push(url, props) {{
                stack.pushed = ('' + url).split('/').pop()
                stack.dialog = dialogs.createObject(stack)
                return stack.dialog
            }}
        }}
        function stackObject() {{ return stack }}

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
        function load(url, props) {{
            loader.setSource('', {{}})
            loader.setSource(url, props)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        // Windowless view builds no rows unless asked.
        function layout() {{
            var view = findIn(loader.item, 'chatList')
            if (view && view.forceLayout) {{ view.forceLayout() }}
        }}
        function muteText() {{ return get('muteItem', 'text') }}

        property var steps: []
        function record(label, value) {{ steps.push([label, '' + value]) }}
        function stepsJson() {{ return JSON.stringify(steps) }}

        /// Mute via dialog: tap, see what opened, pick `seconds`.
        function muteVia(prefix, seconds) {{
            stack.pushed = ''
            record(prefix + '-mute-label', muteText())
            record(prefix + '-tap', click('muteItem'))
            record(prefix + '-pushed', stack.pushed)
            stack.dialog.picked(seconds)
        }}
        /// Unmute: tap; nothing should open.
        function unmuteVia(prefix) {{
            stack.pushed = ''
            stack.dialog = null
            record(prefix + '-unmute-label', muteText())
            record(prefix + '-unmute-tap', click('muteItem'))
            record(prefix + '-unmute-pushed', stack.pushed)
        }}

        property int step: 0
        property bool finished: false
        readonly property var script: [
            {{ until: function() {{ return core.status === 'ready' }},
               run: function() {{
                   record('list-load', load('{list}', {{ accountId: 1, archived: false }}))
               }} }},
            {{ until: function() {{ layout(); return muteText() === 'Mute' }},
               run: function() {{ muteVia('list', 28800) }} }},
            {{ until: function() {{ return muteText() === 'Unmute' }},
               run: function() {{ unmuteVia('list') }} }},
            {{ until: function() {{ return muteText() === 'Mute' }},
               run: function() {{
                   record('group-load', load('{group}', {{ accountId: 1, chatId: 2, chatName: '' }}))
               }} }},
            {{ until: function() {{ return get('chat', 'loaded') === 'true' }},
               run: function() {{
                   record('group-menu', get('groupMenu', 'visible'))
                   muteVia('group', 86400)
               }} }},
            {{ until: function() {{ return muteText() === 'Unmute' }},
               run: function() {{ unmuteVia('group') }} }},
            {{ until: function() {{ return muteText() === 'Mute' }},
               run: function() {{
                   record('contact-load', load('{contact}', {{ accountId: 1, chatId: 1, chatName: '' }}))
               }} }},
            {{ until: function() {{ return get('chat', 'loaded') === 'true' }},
               run: function() {{
                   record('contact-menu', get('contactMenu', 'visible'))
                   muteVia('contact', 604800)
               }} }},
            {{ until: function() {{ return muteText() === 'Unmute' }},
               run: function() {{ unmuteVia('contact') }} }},
            {{ until: function() {{ return muteText() === 'Mute' }},
               run: function() {{ finished = true }} }}
        ]
        Timer {{
            interval: 50; running: !finished; repeat: true
            onTriggered: {{
                var next = script[step]
                if (next.until()) {{
                    step += 1
                    next.run()
                }}
            }}
        }}
        function isFinished() {{ return finished ? 'true' : 'false' }}
        function stepReached() {{ return '' + step }}
    }}
",
        list = common::page_url("ChatListPage.qml"),
        group = common::page_url("GroupPage.qml"),
        contact = common::page_url("ContactPage.qml"),
    )
}

/// Seconds for steps; only a broken run spends them.
const PATIENCE_SECONDS: u32 = 40;

#[test]
#[allow(clippy::too_many_lines)]
fn every_mute_entry_asks_how_long_and_every_unmute_entry_does_not() {
    let temp = std::env::temp_dir().join(format!("piirit-mute-flow-{}", std::process::id()));
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
    let mut outcome = (String::new(), String::new());
    let outcome_ptr: *mut (String, String) = std::ptr::addr_of_mut!(outcome);
    wait_for_steps(engine_ptr, outcome_ptr, PATIENCE_SECONDS);

    engine.exec();

    let (steps_json, reached) = outcome;
    let steps: Vec<(String, String)> = serde_json::from_str(&steps_json).unwrap_or_default();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, reached step {reached}, calls: {names:?}");

    for page in ["list", "group", "contact"] {
        assert_eq!(
            value(&format!("{page}-load")),
            "ok",
            "the {page} page did not load. {context}"
        );
    }
    assert_eq!(
        (value("group-menu"), value("contact-menu")),
        ("true".to_string(), "true".to_string()),
        "the group or contact page hides the pull-down that holds Mute. {context}"
    );
    for entry in ["list", "group", "contact"] {
        assert_eq!(
            (
                value(&format!("{entry}-mute-label")),
                value(&format!("{entry}-tap")),
                value(&format!("{entry}-pushed")),
            ),
            (
                "Mute".to_string(),
                "ok".to_string(),
                "MuteDialog.qml".to_string()
            ),
            "Mute in the {entry} menu did not open the length dialog. {context}"
        );
        // Label flipped: core's answer read back. Unmute opens nothing.
        assert_eq!(
            (
                value(&format!("{entry}-unmute-label")),
                value(&format!("{entry}-unmute-tap")),
                value(&format!("{entry}-unmute-pushed")),
            ),
            ("Unmute".to_string(), "ok".to_string(), String::new()),
            "once muted, the {entry} menu did not offer Unmute, or it opened a \
             page. {context}"
        );
    }

    // Only the picked lengths, in order; nothing sent on the tap that opened the dialog.
    let mutes: Vec<Value> = calls
        .iter()
        .filter(|(method, _)| method == "set_chat_mute_duration")
        .map(|(_, params)| params.clone())
        .collect();
    let row_chat = mutes.first().and_then(|params| params.get(1)).cloned();
    assert_eq!(
        mutes,
        vec![
            json!([1, row_chat, {"kind": "Until", "duration": 28800}]),
            json!([1, row_chat, {"kind": "NotMuted"}]),
            json!([1, 2, {"kind": "Until", "duration": 86400}]),
            json!([1, 2, {"kind": "NotMuted"}]),
            json!([1, 1, {"kind": "Until", "duration": 604_800}]),
            json!([1, 1, {"kind": "NotMuted"}]),
        ],
        "the menus did not send exactly the reader's picks to the core. {context}"
    );
}

/// Each second: steps done or patience spent? Then take steps and quit. Whole seconds: vendored
/// `single_shot` drops fraction.
fn wait_for_steps(engine: *mut QmlEngine, outcome: *mut (String, String), patience: u32) {
    single_shot(Duration::from_secs(1), move || unsafe {
        let read = |method: &str| {
            QString::from_qvariant((*engine).invoke_method(method.into(), &[]))
                .map(|value| value.to_string())
                .unwrap_or_default()
        };
        if read("isFinished") == "true" || patience == 0 {
            *outcome = (read("stepsJson"), read("stepReached"));
            (*engine).quit();
        } else {
            wait_for_steps(engine, outcome, patience - 1);
        }
    });
}
