//! Own channel's page: says Channel, offers its QR invite code (only way subscribers join: core
//! takes no members added). QR page then shows that channel's code alone, no scanner. Group page
//! beside it keeps its words, offers no code.
//!
//! Steps run in probe off pages' own loads, not clock.

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
const READINGS: [&str; 12] = [
    "channel-title",
    "channel-menu",
    "channel-invite",
    "channel-leave",
    "navigation",
    "code-title",
    "code-switcher",
    "code-profile",
    "code-intro",
    "code-link",
    "group-title",
    "group-invite",
];

// `pageStack` recorded, not performed; see qml_new_group.rs.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe
        property var readings: ({})
        property string groupUrl: ''
        property string qrUrl: ''
        // 0 channel page, 1 channel's code, 2 group page, 3 done.
        property int step: 0
        property QtObject pageStack: QtObject {
            property string log: ''
            function note(verb, page, properties) {
                var name = ('' + page).split('/').pop()
                log = log + verb + ':' + name + JSON.stringify(properties) + '|'
            }
            function push(page, properties) { note('push', page, properties) }
            function replace(page, properties) { note('replace', page, properties) }
            function previousPage(page) { return null }
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
        function openGroup(chatId) {
            loader.setSource('', {})
            loader.setSource(probe.groupUrl, { accountId: 1, chatId: chatId, chatName: '' })
        }
        function start(groupUrl, qrUrl) {
            probe.groupUrl = groupUrl
            probe.qrUrl = qrUrl
            probe.openGroup(1)
            poll.running = loader.status === Loader.Ready
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        Timer {
            id: poll
            interval: 50
            repeat: true
            onTriggered: {
                if (probe.step === 0) {
                    var chat = findIn(loader.item, 'chat')
                    if (!chat || !chat.loaded) { return }
                    probe.readings['channel-title'] = read('pageHeader', 'title')
                    probe.readings['channel-menu'] = read('groupMenu', 'visible')
                    probe.readings['channel-invite'] = read('channelInvite', 'visible')
                    probe.readings['channel-leave'] = read('leaveButton', 'visible')
                    findIn(loader.item, 'channelInvite').clicked()
                    probe.readings['navigation'] = pageStack.log
                    // What the push hands QR page.
                    loader.setSource('', {})
                    loader.setSource(probe.qrUrl, {
                        accountId: 1, chatId: 1, chatName: 'News', appActive: true
                    })
                    probe.step = 1
                } else if (probe.step === 1) {
                    var link = read('myInviteLabel', 'text')
                    if (link.indexOf('https://') !== 0) { return }
                    probe.readings['code-title'] = read('pageHeader', 'title')
                    probe.readings['code-switcher'] = read('viewSwitcher', 'visible')
                    probe.readings['code-profile'] = read('profileRow', 'visible')
                    probe.readings['code-intro'] = read('intro', 'text')
                    probe.readings['code-link'] = link
                    probe.openGroup(2)
                    probe.step = 2
                } else if (probe.step === 2) {
                    var group = findIn(loader.item, 'chat')
                    if (!group || !group.loaded) { return }
                    probe.readings['group-title'] = read('pageHeader', 'title')
                    probe.readings['group-invite'] = read('channelInvite', 'visible')
                    probe.step = 3
                    poll.running = false
                }
            }
        }
        function finished() { return probe.step === 3 ? 'yes' : 'no' }
        function reading(label) {
            var value = probe.readings[label]
            return value === undefined ? 'unread' : value
        }
    }
";

#[test]
fn an_own_channel_offers_the_code_subscribers_join_by() {
    let temp = std::env::temp_dir().join(format!("piirit-channel-code-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test; set before Qt and server start.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_OWN_CHANNELS", "1");
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
        let started = call!(
            "start",
            QString::from(common::page_url("GroupPage.qml")),
            QString::from(common::page_url("QrPage.qml"))
        );
        common::record(&loading, "load", started.into());
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

/// Channel page and its code; group page unchanged.
fn assert_outcome(steps: &[(String, String)], calls: &[(String, Value)]) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}\ncalls: {names:?}");
    let value = |label: &str| common::value_of(steps, label).to_string();

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("channel-title"),
        "Channel",
        "an own channel's page is headed as a group. {context}"
    );
    for label in ["channel-menu", "channel-invite"] {
        assert_eq!(
            value(label),
            "true",
            "an own channel's page offers no QR invite code ({label}). {context}"
        );
    }
    assert_eq!(
        value("channel-leave"),
        "false",
        "an own channel's page offers to leave it as a group. {context}"
    );
    assert!(
        value("navigation")
            .ends_with(r#"push:QrPage.qml{"accountId":1,"chatId":1,"chatName":"chat 1"}|"#),
        "QR invite code does not open the QR page on this channel. {context}"
    );

    assert_eq!(
        value("code-title"),
        "QR invite code",
        "the channel's code page is not headed as an invite code. {context}"
    );
    for label in ["code-switcher", "code-profile"] {
        assert_eq!(
            value(label),
            "false",
            "the channel's code page shows the profile's side ({label}). {context}"
        );
    }
    assert_eq!(
        value("code-intro"),
        "Scan to join channel \"News\"",
        "the channel's code is not named as Delta Chat names it. {context}"
    );
    assert!(
        value("code-link").contains("&b="),
        "the code shown is not the channel's invite. {context}"
    );
    let asked: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "get_chat_securejoin_qr_code")
        .map(|(_, params)| params)
        .collect();
    assert!(
        asked.contains(&&serde_json::json!([1, 1])),
        "the channel's own invite was never asked for. {context}"
    );
    assert!(
        !asked.contains(&&serde_json::json!([1, null])),
        "the channel's code page asked for the profile's invite. {context}"
    );

    assert_eq!(
        value("group-title"),
        "Group",
        "a group's page lost its heading. {context}"
    );
    assert_eq!(
        value("group-invite"),
        "false",
        "a group's page offers a channel's invite. {context}"
    );
}
