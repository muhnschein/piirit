//! Channel making: New group page in channel mode. Name and picture only, no member list (core
//! takes none: subscribers join by invite), one `create_broadcast`, picture on it, channel opened
//! in page's place.
//!
//! Steps run in probe off contacts loading and page's own navigation, not clock.

// Qt harness: see qml_new_group.rs.
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
use serde_json::Value;

mod common;

/// Seconds before probe counts as stuck.
const GIVE_UP_SECS: u64 = 60;

/// Readings probe takes, by label.
const READINGS: [&str; 9] = [
    "title",
    "members-header",
    "self-row",
    "add-members",
    "placeholder",
    "unnamed",
    "named",
    "navigation",
    "opened",
];

// `pageStack` recorded, not performed; see qml_new_group.rs.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: probe
        property var readings: ({})
        // 0 contacts loading, 1 created and waiting for chat, 2 done.
        property int step: 0
        property QtObject pageStack: QtObject {
            property string log: ''
            property int chatId: 0
            function note(verb, page, properties) {
                var name = ('' + page).split('/').pop()
                log = log + verb + ':' + name + '|'
                if (properties && properties.chatId !== undefined) {
                    chatId = properties.chatId
                }
            }
            function push(page, properties) { note('push', page, properties) }
            function replace(page, properties) { note('replace', page, properties) }
        }
        Loader { id: loader }
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
        function read(name, property) {
            var item = findIn(loader.item, name)
            return item ? '' + item[property] : 'missing:' + name
        }
        function start(url) {
            loader.setSource(url, { accountId: 1, channel: true })
            poll.running = loader.status === Loader.Ready
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        Timer {
            id: poll
            interval: 50
            repeat: true
            onTriggered: {
                if (probe.step === 0) {
                    // Own row arrives with contacts: group page would draw it now.
                    var contacts = findIn(loader.item, 'contacts')
                    if (!contacts || contacts.count === 0) { return }
                    probe.readings['title'] = read('pageHeader', 'title')
                    probe.readings['members-header'] = read('membersHeader', 'visible')
                    var own = findIn(loader.item, 'memberRow1')
                    probe.readings['self-row'] = own ? '' + own.visible : 'false'
                    probe.readings['add-members'] = read('addMembersButton', 'visible')
                    probe.readings['placeholder'] = read('nameField', 'placeholderText')
                    probe.readings['unnamed'] = read('createButton', 'enabled')
                    findIn(loader.item, 'nameField').text = 'News'
                    loader.item.picturePath = '/tmp/news.png'
                    probe.readings['named'] = read('createButton', 'enabled')
                    findIn(loader.item, 'createButton').clicked()
                    probe.step = 1
                } else if (probe.step === 1) {
                    if (pageStack.log.indexOf('replace:') < 0) { return }
                    probe.readings['navigation'] = pageStack.log
                    probe.readings['opened'] = '' + pageStack.chatId
                    probe.step = 2
                    poll.running = false
                }
            }
        }
        function finished() { return probe.step === 2 ? 'yes' : 'no' }
        function reading(label) {
            var value = probe.readings[label]
            return value === undefined ? 'unread' : value
        }
    }
";

#[test]
fn the_new_channel_page_makes_a_channel_without_members() {
    let temp = std::env::temp_dir().join(format!("piirit-new-channel-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test; set before Qt and server start.
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
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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
    let loading = steps.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        common::record(
            &loading,
            "load",
            call!("start", QString::from(common::page_url("NewGroupPage.qml"))).into(),
        );
    });

    // Once a second: done, or out of time.
    for second in 2..=GIVE_UP_SECS {
        let done = steps.clone();
        single_shot(Duration::from_secs(second), move || unsafe {
            if second < GIVE_UP_SECS && call!("finished") != "yes" {
                return;
            }
            for label in READINGS {
                common::record(&done, label, call!("reading", QString::from(label)).into());
            }
            (*engine_ptr).quit();
        });
    }

    engine.exec();

    let steps = steps.borrow().clone();
    assert_outcome(&steps, &common::calls(&journal));
}

/// Channel page's own words, no members, one channel made and opened.
fn assert_outcome(steps: &[(String, String)], calls: &[(String, Value)]) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}\ncalls: {names:?}");
    let value = |label: &str| common::value_of(steps, label).to_string();

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("title"),
        "New channel",
        "the page does not say it makes a channel. {context}"
    );
    assert_eq!(
        value("placeholder"),
        "Channel name",
        "the name field asks for a group's name. {context}"
    );
    for label in ["members-header", "self-row", "add-members"] {
        assert_eq!(
            value(label),
            "false",
            "a channel being made offers members ({label}), which the core refuses. {context}"
        );
    }
    assert_eq!(
        value("unnamed"),
        "false",
        "creating was offered with nothing typed. {context}"
    );
    assert_eq!(
        value("named"),
        "true",
        "typing a name did not enable creating the channel. {context}"
    );

    let made: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "create_broadcast")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        made,
        vec![&serde_json::json!([1, "News"])],
        "the channel was not made once with the typed name. {context}"
    );
    assert!(
        !names.contains(&"create_group_chat"),
        "a group was made instead of, or beside, the channel. {context}"
    );
    assert!(
        !names.contains(&"add_contact_to_chat"),
        "someone was added to the channel, which the core refuses. {context}"
    );
    let chat_id: u32 = value("opened").parse().unwrap_or(0);
    assert_ne!(chat_id, 0, "the channel was not opened. {context}");
    let pictured: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "set_chat_profile_image")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        pictured,
        vec![&serde_json::json!([1, chat_id, "/tmp/news.png"])],
        "the chosen picture was not put on the channel once it existed. {context}"
    );
    assert!(
        value("navigation").ends_with("replace:ConversationPage.qml|"),
        "making the channel did not open it in place of this page. {context}"
    );
}
