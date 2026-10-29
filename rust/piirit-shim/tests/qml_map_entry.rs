//! Chat's location rows open chat's map: one-off point on itself, live notice on its sender.
//!
//! Page loaded from QML tree copy with `EnterKey` taken out; see
//! `common::qml_tree_without_enter_key`. Steps wait on what they need, not on time.

// Qt harness: see qml_chat_info_attached.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    import 'file://__COMPONENTS__'
    Item {
        // Silica's `pageStack`, recorded not performed.
        property QtObject pageStack: QtObject {
            property string log: ''
            function push(page, properties) {
                log += 'push:' + ('' + page).split('/').pop()
                       + '(account=' + properties.accountId + ',query=' + properties.query
                       + ',heading=' + properties.heading
                       + ',subheading=' + properties.subheading + ')|'
            }
            function pushAttached(page, properties) { return null }
            function nextPage(page) { return null }
        }
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, {
                accountId: 1,
                chatId: 1,
                chatName: 'Ada',
                status: PageStatus.Activating
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.status = PageStatus.Active
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        // Row tapped, as list reports it: message, sender.
        function tap(messageId, senderId) {
            var list = findIn(loader.item, 'messageList')
            if (!list) { return 'missing:messageList' }
            list.locationRequested(messageId, senderId)
            return 'ok'
        }
        function setLocations(on) { Settings.locationStreamingEnabled = on; return 'ok' }
        // What page hands on: list's map taps, tray's location entry.
        function gates() {
            var list = findIn(loader.item, 'messageList')
            var attach = findIn(loader.item, 'attachButton')
            if (!list || !attach) { return 'missing' }
            return list.locationsEnabled + ',' + attach.locationAvailable
        }
        function ready() { return '' + (core.status === 'ready') }
        function pushes() { return pageStack.log }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_location_row_opens_the_chats_map_where_it_points() {
    let temp = common::temp_tree("map-entry");
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_LOCATIONS", "1");
        std::env::set_var("XDG_CACHE_HOME", temp.join("cache"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(
        PROBE_QML
            .replace(
                "__COMPONENTS__",
                &tree.join("components").display().to_string(),
            )
            .as_str(),
    ));
    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(String, String)> = Vec::new();
    let steps_ptr: *mut Vec<(String, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            QString::from_qvariant(
                (*engine_ptr).invoke_method($name.into(), &[$(QVariant::from($arg)),*]),
            )
            .map(|value| value.to_string())
            .unwrap_or_default()
        }};
    }

    let mut plan = common::Plan::default();
    let url = common::page_url_in(&tree, "ConversationPage.qml");
    plan.until(1, 10, move |last| unsafe {
        let done = call!("ready") == "true";
        if done || last {
            (*steps_ptr).push((
                "load".to_string(),
                call!("load", QString::from(url.clone())),
            ));
            // Location streaming off (experimental, as in Delta Chat), then on.
            (*steps_ptr).push(("gates-off".to_string(), call!("gates")));
            call!("setLocations", true);
            (*steps_ptr).push(("gates-on".to_string(), call!("gates")));
            // Ada's one-off point.
            (*steps_ptr).push(("tap-point".to_string(), call!("tap", 30, 10)));
        }
        done
    });
    plan.until(1, 10, move |last| unsafe {
        let done = call!("pushes").contains("WebxdcPage");
        if done || last {
            (*steps_ptr).push(("point".to_string(), call!("pushes")));
            // Her live sharing's start notice.
            (*steps_ptr).push(("tap-notice".to_string(), call!("tap", 31, 10)));
        }
        done
    });
    plan.until(1, 10, move |last| unsafe {
        let done = call!("pushes").matches("WebxdcPage").count() == 2;
        if done || last {
            (*steps_ptr).push(("notice".to_string(), call!("pushes")));
            (*engine_ptr).quit();
        }
        done
    });

    plan.run();
    engine.exec();

    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");
    assert_eq!(
        value("load"),
        "ok",
        "the conversation page did not load. {context}"
    );
    assert_eq!(
        value("gates-off"),
        "false,false",
        "the chat offers locations while location streaming is off. {context}"
    );
    assert_eq!(
        value("gates-on"),
        "true,true",
        "the chat hides locations while location streaming is on. {context}"
    );
    assert_eq!(value("tap-point"), "ok", "{context}");
    assert_eq!(
        value("point"),
        "push:WebxdcPage.qml(account=1,query=?poi=60.1,24.9,heading=chat 1,subheading=Map)|",
        "a one-off point did not open the chat's map on itself. {context}"
    );
    assert!(
        value("notice").ends_with(
            "|push:WebxdcPage.qml(account=1,query=?contactId=10,heading=chat 1,subheading=Map)|"
        ),
        "a live notice did not open the chat's map on its sender. {context}"
    );
    // Chat's map, not every chat's.
    let pointed: Vec<serde_json::Value> = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "init_webxdc_integration")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        pointed,
        [serde_json::json!([1, 1]), serde_json::json!([1, 1])],
        "{context}"
    );
}
