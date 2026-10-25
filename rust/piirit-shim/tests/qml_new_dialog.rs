//! "New" dialog: chat, group, channel, each with its line; one switch at a time, nothing preset,
//! forward swipe only once picked. Picked page pushed on top (back returns here), handed chat list
//! to open chat over. Channel is group page in channel mode.

// Qt harness: see qml_new_group.rs.
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

// `pageStack` recorded, not performed; see qml_new_group.rs. No core: dialog asks it nothing.
// Name stands in for chat list page.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property QtObject pageStack: QtObject {
            /// `push:Foo.qml{...}|...`
            property string log: ''
            function note(verb, page, properties) {
                var name = ('' + page).split('/').pop()
                log = log + verb + ':' + name + JSON.stringify(properties) + '|'
            }
            function push(page, properties) { note('push', page, properties) }
            function replace(page, properties) { note('replace', page, properties) }
        }
        Loader { id: loader }
        // What Silica built as destination: once per URL, so group page built for group is
        // the one channel goes to.
        QtObject { id: builtChat }
        QtObject { id: builtGroup; property bool channel: false }
        function load(url) {
            loader.setSource(url, { accountId: 3, landOn: 'chat-list' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        /// Switch's words and line, `|`-joined.
        function words(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return item.text + '|' + item.description
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// All three switches, as `chat/group/channel`.
        function switches() {
            return ['chatSwitch', 'groupSwitch', 'channelSwitch'].map(function(name) {
                var item = findIn(loader.item, name)
                return item ? '' + item.checked : 'missing'
            }).join('/')
        }
        function canAccept() { return '' + loader.item.canAccept }
        /// Forward swipe (Silica allows it only while `canAccept`); what it pushed, and for
        /// group page what the built page ends up with.
        function swipeForward() {
            var chat = loader.item.choice === 'chat'
            loader.item.acceptDestinationInstance = chat ? builtChat : builtGroup
            var before = pageStack.log.length
            loader.item.accept()
            var pushed = pageStack.log.substring(before)
            return chat || pushed === '' ? pushed : pushed + 'built-channel:' + builtGroup.channel
        }
        function header(property) { return '' + findIn(loader.item, 'header')[property] }
        function navigation() { return pageStack.log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_new_dialog_offers_chat_group_and_channel() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    piirit_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
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
    let taken = steps.clone();
    // Loads, builds switches, taps, accepts: all synchronous once loop runs.
    single_shot(Duration::from_secs(1), move || unsafe {
        let record = |label: &str, value: String| common::record(&taken, label, value.into());
        record(
            "load",
            call!("load", QString::from(common::page_url("NewDialog.qml"))),
        );
        record("title", call!("header", QString::from("title")));
        record("accept-text", call!("header", QString::from("acceptText")));
        for name in ["chatSwitch", "groupSwitch", "channelSwitch"] {
            record(name, call!("words", QString::from(name)));
        }
        record("fresh", call!("switches"));
        record("fresh-accept", call!("canAccept"));
        record("fresh-swipe", call!("swipeForward"));

        call!("tap", QString::from("chatSwitch"));
        record("chat", call!("switches"));
        record("chat-accept", call!("canAccept"));
        record("chat-swipe", call!("swipeForward"));
        // Tap again: nothing picked.
        call!("tap", QString::from("chatSwitch"));
        record("chat-off", call!("switches"));
        record("chat-off-accept", call!("canAccept"));

        call!("tap", QString::from("groupSwitch"));
        record("group", call!("switches"));
        record("group-swipe", call!("swipeForward"));
        // Straight from group to channel: group goes off.
        call!("tap", QString::from("channelSwitch"));
        record("channel", call!("switches"));
        record("channel-swipe", call!("swipeForward"));
        record("navigation", call!("navigation"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    let context = format!("steps: {steps:?}");
    let value = |label: &str| common::value_of(&steps, label).to_string();
    assert_eq!(value("load"), "ok", "the dialog did not load. {context}");
    assert_eq!(
        (value("title").as_str(), value("accept-text").as_str()),
        ("", "Next"),
        "the dialog repeats the pull-down's New as a heading, or lacks Next to go on. \
         {context}"
    );
    assert_eq!(
        value("chatSwitch"),
        "New chat|You write to one person.",
        "the chat choice lacks its name or its line. {context}"
    );
    assert_eq!(
        value("groupSwitch"),
        "New group|Several people write to each other.",
        "the group choice lacks its name or its line. {context}"
    );
    assert_eq!(
        value("channelSwitch"),
        "New channel|Channels are a one-to-many tool for broadcasting your messages.",
        "the channel choice lacks its name or Delta Chat's line for it. {context}"
    );
    assert_eq!(
        (
            value("fresh").as_str(),
            value("fresh-accept").as_str(),
            value("fresh-swipe").as_str()
        ),
        ("false/false/false", "false", ""),
        "something is picked before the reader picked, or the dialog goes on with nothing \
         picked. {context}"
    );
    assert_eq!(
        (value("chat").as_str(), value("chat-accept").as_str()),
        ("true/false/false", "true"),
        "tapping chat did not pick chat alone, ready to go on. {context}"
    );
    assert_eq!(
        value("chat-swipe"),
        r#"push:NewChatPage.qml{"accountId":3,"landOn":"chat-list"}|"#,
        "chat does not push the new chat page, handed the chat list to open the chat over. \
         {context}"
    );
    assert_eq!(
        (
            value("chat-off").as_str(),
            value("chat-off-accept").as_str()
        ),
        ("false/false/false", "false"),
        "a second tap does not take the pick back. {context}"
    );
    assert_eq!(
        value("group"),
        "false/true/false",
        "tapping group did not pick group alone. {context}"
    );
    assert_eq!(
        value("group-swipe"),
        r#"push:NewGroupPage.qml{"accountId":3,"landOn":"chat-list","channel":false}|built-channel:false"#,
        "group does not push the new group page in group mode. {context}"
    );
    assert_eq!(
        value("channel"),
        "false/false/true",
        "tapping channel did not turn group off. {context}"
    );
    assert_eq!(
        value("channel-swipe"),
        r#"push:NewGroupPage.qml{"accountId":3,"landOn":"chat-list","channel":true}|built-channel:true"#,
        "channel does not push the group page in channel mode, or the group page Silica \
         built for group stays in group mode. {context}"
    );
    // Pushed, not replaced: back from picked page comes to this dialog.
    assert!(
        !value("navigation").contains("replace:"),
        "a choice took this dialog's place, so back would skip it. {context}"
    );
}
