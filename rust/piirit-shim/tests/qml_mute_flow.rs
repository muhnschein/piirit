//! Muting from every place offering it, tap to core.
//!
//! Chat list row menu: Mute opens the length dialog, sends nothing until a length comes back;
//! Unmute sends at once, no dialog. Group and contact page Notifications row: opens the dialog
//! either way, told whether muted (Unmute offered), sends the pick. Each entry picks another
//! length, so none stands in for another; each reads core's answer back (label or value flips).
//! Dialog itself: `qml_mute_dialog.rs`.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::path::PathBuf;

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
    import 'file://{components}'
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
            property var props: null
            function push(url, props) {{
                stack.pushed = ('' + url).split('/').pop()
                stack.props = props === undefined ? null : props
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
        /// Every item so named, in tree order: one mute entry per chat row.
        function findAll(node, name, found) {{
            if (!node) {{ return found }}
            // Row menu is both a child and `menu`: once each.
            if (node.objectName === name && found.indexOf(node) < 0) {{ found.push(node) }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{ findAll(kids[i], name, found) }}
            if (node.contentItem && node.contentItem !== node) {{
                findAll(node.contentItem, name, found)
            }}
            if (node.menu) {{ findAll(node.menu, name, found) }}
            return found
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
        /// Mute entry: page's only one, or a chat row's (`row`).
        property int row: 0
        function muteItem() {{ return findAll(loader.item, 'muteItem', [])[row] }}
        function muteText() {{ var item = muteItem(); return item ? item.text : 'missing' }}
        function rowCount() {{ return findAll(loader.item, 'muteItem', []).length }}

        property var steps: []
        function record(label, value) {{ steps.push([label, '' + value]) }}
        function stepsJson() {{ return JSON.stringify(steps) }}

        /// Mute via dialog: tap, see what opened, pick `seconds`.
        function muteVia(prefix, seconds) {{
            stack.pushed = ''
            record(prefix + '-mute-label', muteText())
            muteItem().clicked()
            record(prefix + '-pushed', stack.pushed)
            record(prefix + '-note', stack.props ? stack.props.mentionsStillNotify : 'none')
            stack.dialog.picked(seconds)
        }}
        /// Mute tapped, dialog backed out of.
        function openOnly(prefix) {{
            stack.props = null
            muteItem().clicked()
            record(prefix + '-note', stack.props ? stack.props.mentionsStillNotify : 'none')
        }}
        /// Unmute: tap; nothing should open.
        function unmuteVia(prefix) {{
            stack.pushed = ''
            stack.dialog = null
            record(prefix + '-unmute-label', muteText())
            muteItem().clicked()
            record(prefix + '-unmute-pushed', stack.pushed)
        }}

        /// Page's Notifications row: tap, see what opened and was said, pick `seconds`.
        function rowVia(prefix, seconds) {{
            stack.pushed = ''
            stack.props = null
            record(prefix + '-value', get('notifications', 'value'))
            record(prefix + '-click', click('notifications'))
            record(prefix + '-pushed', stack.pushed)
            record(prefix + '-muted', stack.props ? stack.props.muted : 'none')
            record(prefix + '-note', stack.props ? stack.props.mentionsStillNotify : 'none')
            stack.dialog.picked(seconds)
        }}
        function rowValue() {{ return get('notifications', 'value') }}

        property int step: 0
        property bool finished: false
        readonly property var script: [
            {{ until: function() {{ return core.status === 'ready' }},
               run: function() {{
                   // Replies still notify: note in groups only.
                   Settings.notificationsEnabled = true
                   Settings.mentionNotifications = true
                   record('list-load', load('{list}', {{ accountId: 1, archived: false }}))
               }} }},
            {{ until: function() {{ layout(); return rowCount() === 2 && muteText() === 'Mute' }},
               run: function() {{ muteVia('row0', 28800) }} }},
            {{ until: function() {{ return muteText() === 'Unmute' }},
               run: function() {{ unmuteVia('row0') }} }},
            {{ until: function() {{ return muteText() === 'Mute' }},
               run: function() {{ row = 1; muteVia('row1', 3600) }} }},
            {{ until: function() {{ return muteText() === 'Unmute' }},
               run: function() {{ unmuteVia('row1') }} }},
            {{ until: function() {{ return muteText() === 'Mute' }},
               run: function() {{
                   // Mentions off: no note even in a group. Backed out: nothing sent.
                   Settings.mentionNotifications = false
                   row = 0
                   openOnly('quiet0')
                   row = 1
                   openOnly('quiet1')
                   Settings.mentionNotifications = true
                   record('group-load', load('{group}', {{ accountId: 1, chatId: 2, chatName: 'Group' }}))
               }} }},
            {{ until: function() {{ return rowValue() === 'On' }},
               run: function() {{ rowVia('group', 86400) }} }},
            {{ until: function() {{ return rowValue() === 'Muted' }},
               run: function() {{ rowVia('group-muted', 0) }} }},
            {{ until: function() {{ return rowValue() === 'On' }},
               run: function() {{
                   // Mentions off: no note. Backed out: nothing sent.
                   Settings.mentionNotifications = false
                   stack.props = null
                   click('notifications')
                   record('group-quiet-note', stack.props ? stack.props.mentionsStillNotify : 'none')
                   Settings.mentionNotifications = true
                   record('contact-load', load('{contact}', {{ accountId: 1, chatId: 1, chatName: 'Contact' }}))
               }} }},
            {{ until: function() {{ return rowValue() === 'On' }},
               run: function() {{ rowVia('contact', 604800) }} }},
            {{ until: function() {{ return rowValue() === 'Muted' }},
               run: function() {{ rowVia('contact-muted', 0) }} }},
            {{ until: function() {{ return rowValue() === 'On' }},
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
        components = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/components")
            .display(),
        list = common::page_url("ChatListPage.qml"),
        group = common::page_url("GroupPage.qml"),
        contact = common::page_url("ContactPage.qml"),
    )
}

/// Seconds for steps; only a broken run spends them.
const PATIENCE_SECONDS: u32 = 40;

#[test]
#[allow(clippy::too_many_lines)]
fn every_mute_entry_asks_how_long_and_only_the_list_unmutes_without_asking() {
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

    assert_eq!(
        value("list-load"),
        "ok",
        "the chat list did not load. {context}"
    );
    for entry in ["row0", "row1"] {
        assert_eq!(
            (
                value(&format!("{entry}-mute-label")),
                value(&format!("{entry}-pushed"))
            ),
            ("Mute".to_string(), "MuteDialog.qml".to_string()),
            "Mute in the {entry} menu did not open the length dialog. {context}"
        );
        // Label flipped: core's answer read back. Unmute opens nothing.
        assert_eq!(
            (
                value(&format!("{entry}-unmute-label")),
                value(&format!("{entry}-unmute-pushed")),
            ),
            ("Unmute".to_string(), String::new()),
            "once muted, the {entry} menu did not offer Unmute, or it opened a \
             page. {context}"
        );
    }

    // Only the picked lengths, in order; nothing sent on the tap that opened the dialog, nor
    // for a dialog backed out of.
    let mutes: Vec<Value> = calls
        .iter()
        .filter(|(method, _)| method == "set_chat_mute_duration")
        .map(|(_, params)| params.clone())
        .collect();
    let chat_of = |index: usize| mutes.get(index).and_then(|params| params.get(1)).cloned();
    let (first, second) = (chat_of(0), chat_of(2));
    assert_eq!(
        mutes,
        vec![
            json!([1, first, {"kind": "Until", "duration": 28800}]),
            json!([1, first, {"kind": "NotMuted"}]),
            json!([1, second, {"kind": "Until", "duration": 3600}]),
            json!([1, second, {"kind": "NotMuted"}]),
            json!([1, 2, {"kind": "Until", "duration": 86400}]),
            json!([1, 2, {"kind": "NotMuted"}]),
            json!([1, 1, {"kind": "Until", "duration": 604_800}]),
            json!([1, 1, {"kind": "NotMuted"}]),
        ],
        "the menus and rows did not send exactly the reader's picks to the core. {context}"
    );

    // Page rows: value says core's state, dialog told whether muted. Group with mentions on gets
    // the note; contact never.
    assert_eq!(
        value("group-quiet-note"),
        "false",
        "the group page's note shows though Mentions is off. {context}"
    );
    for (page, note) in [("group", "true"), ("contact", "false")] {
        assert_eq!(
            value(&format!("{page}-load")),
            "ok",
            "the {page} page did not load. {context}"
        );
        for (prefix, shown, muted) in [
            (page.to_string(), "On", "false"),
            (format!("{page}-muted"), "Muted", "true"),
        ] {
            assert_eq!(
                (
                    value(&format!("{prefix}-value")),
                    value(&format!("{prefix}-click")),
                    value(&format!("{prefix}-pushed")),
                    value(&format!("{prefix}-muted")),
                    value(&format!("{prefix}-note")),
                ),
                (
                    shown.to_string(),
                    "ok".to_string(),
                    "MuteDialog.qml".to_string(),
                    muted.to_string(),
                    note.to_string()
                ),
                "the {page} page's Notifications row ({prefix}) did not show core's state \
                 and open the dialog told so. {context}"
            );
        }
    }
    assert_ne!(first, second, "both picks went to one chat. {context}");

    // Chat 2 is the group: only its dialog says replies still notify, and only while the
    // setting is on.
    let note = |prefix: &str, row: Option<Value>| {
        (
            value(&format!("{prefix}-note")),
            (row == Some(json!(2)) && prefix.starts_with("row")).to_string(),
        )
    };
    for (prefix, row) in [
        ("row0", first.clone()),
        ("row1", second.clone()),
        ("quiet0", first),
        ("quiet1", second),
    ] {
        let (shown, wanted) = note(prefix, row);
        assert_eq!(
            shown, wanted,
            "the mentions note on {prefix} is wrong: only a group, with mentions on, \
             gets it. {context}"
        );
    }
}

/// Steps done, or patience spent: take steps, quit.
fn wait_for_steps(engine: *mut QmlEngine, outcome: *mut (String, String), patience: u32) {
    let mut plan = common::Plan::default();
    plan.until(1, patience, move |last| unsafe {
        let read = |method: &str| {
            QString::from_qvariant((*engine).invoke_method(method.into(), &[]))
                .map(|value| value.to_string())
                .unwrap_or_default()
        };
        let done = read("isFinished") == "true";
        if done || last {
            *outcome = (read("stepsJson"), read("stepReached"));
            (*engine).quit();
        }
        done
    });
    plan.run();
}
