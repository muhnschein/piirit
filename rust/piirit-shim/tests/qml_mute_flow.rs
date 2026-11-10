//! Muting from every place offering it, tap to core.
//!
//! Chat list row menu's Notifications, group and contact page Notifications row: each opens the
//! notifications page, told chat and whether muted, sends nothing until a choice comes back.
//! Each entry picks another length, so none stands in for another; each reads core's answer
//! back (row's mute mark, row value with time left). Length and end kept before core mutes.
//! Page itself: `qml_chat_notifications_page.rs`.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::path::PathBuf;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::{json, Value};

mod common;

/// Steps run in order, each once its condition holds (polled; core answers async).
///
/// Page stack is a QML object (context property), not Rust: as `qml_blocking`. Fresh page per
/// push: a pick can't reach handler an earlier page connected.
#[allow(clippy::too_many_lines)]
fn probe_qml(script: &str) -> String {
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{components}'
    Item {{
        id: probe
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
        /// Chat rows' `isMuted`, tree order as `muteItem`.
        function delegates(node, found) {{
            if (!node) {{ return found }}
            if (node.isMuted !== undefined && found.indexOf(node) < 0) {{ found.push(node) }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{ delegates(kids[i], found) }}
            if (node.contentItem && node.contentItem !== node) {{
                delegates(node.contentItem, found)
            }}
            return found
        }}
        function rowMuted() {{
            var row = delegates(loader.item, [])[probe.row]
            return row ? '' + row.isMuted : 'missing'
        }}

        property var steps: []
        function record(label, value) {{ steps.push([label, '' + value]) }}
        function stepsJson() {{ return JSON.stringify(steps) }}

        /// Row menu's entry: tap, see what opened and was said, pick `seconds`.
        function muteVia(prefix, seconds) {{
            stack.pushed = ''
            stack.props = null
            record(prefix + '-mute-label', muteText())
            muteItem().clicked()
            record(prefix + '-pushed', stack.pushed)
            record(prefix + '-muted', stack.props ? stack.props.muted : 'none')
            record(prefix + '-chat', stack.props ? stack.props.chatId : 'none')
            record(prefix + '-note', stack.props ? stack.props.mentionsStillNotify : 'none')
            stack.dialog.picked(seconds)
        }}
        /// Entry tapped, page backed out of.
        function openOnly(prefix) {{
            stack.props = null
            muteItem().clicked()
            record(prefix + '-note', stack.props ? stack.props.mentionsStillNotify : 'none')
        }}

        /// Page's Notifications row: tap, see what opened and was said, pick `seconds`.
        function rowVia(prefix, seconds) {{
            stack.pushed = ''
            stack.props = null
            record(prefix + '-value', get('notifications', 'value'))
            record(prefix + '-click', click('notifications'))
            record(prefix + '-pushed', stack.pushed)
            record(prefix + '-muted', stack.props ? stack.props.muted : 'none')
            record(prefix + '-chat', stack.props ? stack.props.chatId : 'none')
            record(prefix + '-note', stack.props ? stack.props.mentionsStillNotify : 'none')
            stack.dialog.picked(seconds)
        }}
        function rowValue() {{ return get('notifications', 'value') }}

        property int step: 0
        property bool finished: false
        readonly property var script: {script}
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
    )
    .replace("{list}", &common::page_url("ChatListPage.qml"))
    .replace("{group}", &common::page_url("GroupPage.qml"))
    .replace("{contact}", &common::page_url("ContactPage.qml"))
}

/// Every entry, both mute states. Group: chat 2; contact: chat 1.
const EVERY_ENTRY: &str = r"[
            { until: function() { return core.status === 'ready' },
               run: function() {
                   // Replies still notify: note in groups only.
                   Settings.notificationsEnabled = true
                   Settings.mentionNotifications = true
                   record('list-load', load('{list}', { accountId: 1, archived: false }))
               } },
            { until: function() { layout(); return rowCount() === 2 && rowMuted() === 'false' },
               run: function() { muteVia('row0', 28800) } },
            { until: function() { return rowMuted() === 'true' },
               run: function() { muteVia('row0-muted', 0) } },
            { until: function() { return rowMuted() === 'false' },
               run: function() { row = 1; muteVia('row1', 3600) } },
            { until: function() { return rowMuted() === 'true' },
               run: function() { muteVia('row1-muted', 0) } },
            { until: function() { return rowMuted() === 'false' },
               run: function() {
                   // Mentions off: no note even in a group. Backed out: nothing sent.
                   Settings.mentionNotifications = false
                   row = 0
                   openOnly('quiet0')
                   row = 1
                   openOnly('quiet1')
                   Settings.mentionNotifications = true
                   record('group-load', load('{group}', { accountId: 1, chatId: 2, chatName: 'Group' }))
               } },
            { until: function() { return rowValue() === 'On' },
               run: function() { rowVia('group', 86400) } },
            { until: function() { return rowValue().indexOf('Muted') === 0 },
               run: function() { rowVia('group-muted', 0) } },
            { until: function() { return rowValue() === 'On' },
               run: function() {
                   // Mentions off: no note. Backed out: nothing sent.
                   Settings.mentionNotifications = false
                   stack.props = null
                   click('notifications')
                   record('group-quiet-note', stack.props ? stack.props.mentionsStillNotify : 'none')
                   Settings.mentionNotifications = true
                   record('contact-load', load('{contact}', { accountId: 1, chatId: 1, chatName: 'Contact' }))
               } },
            { until: function() { return rowValue() === 'On' },
               run: function() { rowVia('contact', 604800) } },
            { until: function() { return rowValue().indexOf('Muted') === 0 },
               run: function() { rowVia('contact-muted', 0) } },
            { until: function() { return rowValue() === 'On' },
               run: function() { finished = true } }
        ]";

/// Chat 2 a channel: nobody replies to reader there, so no note.
const CHANNEL: &str = r"[
            { until: function() { return core.status === 'ready' },
               run: function() {
                   Settings.notificationsEnabled = true
                   Settings.mentionNotifications = true
                   record('list-load', load('{list}', { accountId: 1, archived: false }))
               } },
            { until: function() { layout(); return rowCount() === 2 && rowMuted() === 'false' },
               run: function() { row = 1; openOnly('channel-row') } },
            { until: function() { return true },
               run: function() {
                   record('channel-load', load('{group}', { accountId: 1, chatId: 2, chatName: 'Channel' }))
               } },
            // 'On' also before core answers; is_channel only after.
            { until: function() { return get('chat', 'loaded') === 'true' && rowValue() === 'On' },
               run: function() {
                   stack.props = null
                   click('notifications')
                   record('channel-page-note', stack.props ? stack.props.mentionsStillNotify : 'none')
                   finished = true
               } }
        ]";

/// Seconds for steps; only a broken run spends them.
const PATIENCE_SECONDS: u32 = 40;

/// Steps, journal's calls, and context for assert messages.
struct Run {
    steps: Vec<(String, String)>,
    calls: Vec<(String, Value)>,
    context: String,
}

impl Run {
    fn value(&self, label: &str) -> String {
        common::value_of(&self.steps, label).to_string()
    }
}

/// `script` against the fake core; `channels` names chats core calls channels.
fn run(name: &str, script: &str, channels: &str) -> Run {
    let temp = std::env::temp_dir().join(format!("piirit-{name}-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_CHANNELS", channels);
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml(script)));
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
    let calls = common::calls(&journal);
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, reached step {reached}, calls: {names:?}");
    Run {
        steps,
        calls,
        context,
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_mute_entry_opens_the_notifications_page_and_sends_the_choice() {
    let run = run("mute-flow", EVERY_ENTRY, "");
    let value = |label: &str| run.value(label);
    let (calls, context) = (&run.calls, &run.context);

    assert_eq!(
        value("list-load"),
        "ok",
        "the chat list did not load. {context}"
    );
    // Same entry muted or not: page told which; core's answer read back (mute mark flips).
    for (entry, muted) in [
        ("row0", "false"),
        ("row0-muted", "true"),
        ("row1", "false"),
        ("row1-muted", "true"),
    ] {
        assert_eq!(
            (
                value(&format!("{entry}-mute-label")),
                value(&format!("{entry}-pushed")),
                value(&format!("{entry}-muted")),
            ),
            (
                "Notifications".to_string(),
                "ChatNotificationsPage.qml".to_string(),
                muted.to_string()
            ),
            "Notifications in the {entry} menu did not open the notifications page told \
             whether muted. {context}"
        );
    }

    // Only the picked lengths, in order; nothing sent on the tap that opened the page, nor
    // for a page backed out of.
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
    for (entry, chat) in [("row0", &first), ("row1", &second)] {
        assert_eq!(
            Some(json!(value(&format!("{entry}-chat"))
                .parse::<u32>()
                .unwrap_or_default())),
            *chat,
            "the {entry} menu's page was told another chat. {context}"
        );
    }

    // Each mute's length and end kept just before core mutes, so its ChatModified finds it;
    // unmute clears it. Other chats' calls may come between.
    let mut pending: Vec<(Value, Value)> = Vec::new();
    let mut kept = Vec::new();
    for (method, params) in calls {
        let key = params[1].as_str().unwrap_or_default();
        if method == "set_config" && key.starts_with("ui.piirit.mute.") {
            pending.push((json!(key), params[2].clone()));
        } else if method == "set_chat_mute_duration" {
            let key = json!(format!("ui.piirit.mute.{}", params[1]));
            let found = pending.iter().position(|(pending, _)| *pending == key);
            let value = found.map(|at| pending.remove(at).1);
            let length = params[2].get("duration").map(ToString::to_string);
            // "LENGTH:END": end now plus length, so well past 2023.
            let shown = value.map(|value| {
                value.as_str().map(|kept| {
                    let (length, end) = kept.split_once(':').unwrap_or_default();
                    (
                        length.to_string(),
                        end.parse::<u64>().unwrap_or_default() > 1_700_000_000,
                    )
                })
            });
            kept.push((length, shown));
        }
    }
    assert_eq!(
        kept,
        [28800, 3600, 86400, 604_800]
            .into_iter()
            .flat_map(|seconds| [
                (
                    Some(seconds.to_string()),
                    Some(Some((seconds.to_string(), true)))
                ),
                (None, Some(None))
            ])
            .collect::<Vec<_>>(),
        "a mute reached core without its length and end kept first, or an unmute without \
         clearing them. {context}"
    );

    // Page rows: value says core's state and time left, page told chat and whether muted.
    // Group with mentions on gets the note; contact never.
    assert_eq!(
        value("group-quiet-note"),
        "false",
        "the group page's note shows though Mentions is off. {context}"
    );
    for (page, chat, note, left) in [
        ("group", "2", "true", "Muted for 1 day(s) (1 d remaining)"),
        (
            "contact",
            "1",
            "false",
            "Muted for 7 day(s) (7 d remaining)",
        ),
    ] {
        assert_eq!(
            value(&format!("{page}-load")),
            "ok",
            "the {page} page did not load. {context}"
        );
        for (prefix, shown, muted) in [
            (page.to_string(), "On", "false"),
            (format!("{page}-muted"), left, "true"),
        ] {
            assert_eq!(
                (
                    value(&format!("{prefix}-value")),
                    value(&format!("{prefix}-click")),
                    value(&format!("{prefix}-pushed")),
                    value(&format!("{prefix}-chat")),
                    value(&format!("{prefix}-muted")),
                    value(&format!("{prefix}-note")),
                ),
                (
                    shown.to_string(),
                    "ok".to_string(),
                    "ChatNotificationsPage.qml".to_string(),
                    chat.to_string(),
                    muted.to_string(),
                    note.to_string()
                ),
                "the {page} page's Notifications row ({prefix}) did not show core's state \
                 and time left, and open the page told so. {context}"
            );
        }
    }
    assert_ne!(first, second, "both picks went to one chat. {context}");

    // Chat 2 is the group: only its page says replies still notify, and only while the
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

#[test]
fn a_channel_gets_no_note_on_replies() {
    let run = run("mute-channel", CHANNEL, "2");
    let context = &run.context;
    assert_eq!(
        (
            run.value("list-load"),
            run.value("channel-row-note"),
            run.value("channel-load"),
            run.value("channel-page-note"),
        ),
        (
            "ok".to_string(),
            "false".to_string(),
            "ok".to_string(),
            "false".to_string()
        ),
        "a channel's notifications page says replies still notify; nobody replies there. \
         {context}"
    );
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
