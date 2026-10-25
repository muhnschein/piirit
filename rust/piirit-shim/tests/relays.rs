//! Relays on profile page: listed, removed, added.
//!
//! Profile may use several relays; one carries its own address. That one listed first; each
//! row reports itself (core's connection words, mailbox off core's report). Row menu removes
//! after Silica countdown; removing the address holder moves address on; last
//! relay can't go; plus opens add-relay page, answer lands in list. Core refusals
//! in core's words. No row says which relay mail leaves through: core picks per connect,
//! unsaid.

// Qt harness: see qml_chat_list.rs.
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

/// Seconds before offline reading counts as stuck.
const GIVE_UP_SECS: u64 = 60;

/// Address `misname` asks core to remove; no relay has it.
const MISNAMED: &str = "nobody@nowhere.example";

/// Profile page and add-relay page over it, to see answer land.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe

        // Silica's `pageStack`, recorded not performed: which page the plus
        // opens, with which profile, and that the add-relay page goes away
        // once it has. In QML, not a QObject: a Loader-loaded page reads
        // `pageStack` off the file the Loader was declared in.
        property QtObject pageStack: QtObject {
            // `push:AddRelayPage.qml|pop|...`
            property string log: ''
            // The account the most recent push was handed.
            property int pushedAccount: -1
            function push(page, properties) {
                var name = ('' + page).split('/').pop()
                pushedAccount = properties && properties.accountId !== undefined
                                ? properties.accountId : -1
                log = log + 'push:' + name + '|'
            }
            function pop() { log = log + 'pop|' }
        }

        Loader { id: loader }
        Loader { id: sub }
        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadSub(url, accountId) {
            sub.setSource('', {})
            sub.setSource(url, { accountId: accountId })
            return sub.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // `data`, not `children`: shim objects are plain QObjects, not visual
        // children. A row's ContextMenu lives in a property: in neither list.
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function getSub(name, property) {
            var item = findIn(sub.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // Something inside a row: labels, menu items, countdown.
        function getIn(row, name, property) {
            var item = findIn(findIn(loader.item, row), name)
            if (!item) { return 'missing:' + row + '/' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function clickIn(row, name) {
            var item = findIn(findIn(loader.item, row), name)
            if (!item) { return 'missing:' + row + '/' + name }
            item.clicked()
            return 'ok'
        }
        function clickSub(name) {
            var item = findIn(sub.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        // The tap on Silica's countdown that calls it off.
        function callOff(row) {
            var remorse = findIn(findIn(loader.item, row), 'relayRemorse')
            if (!remorse) { return 'missing:' + row + '/relayRemorse' }
            remorse.tap()
            return 'ok'
        }
        // What Silica's ComboBox does on a tap of an item.
        function pickSub(name, index) {
            var item = findIn(sub.item, name)
            if (!item) { return 'missing:' + name }
            item.currentIndex = parseInt(index)
            return 'ok'
        }
        // No network long enough for the window to stop IO; relays read again once core says
        // IO stopped (asked earlier: read could beat stop).
        property bool goingOffline: false
        Connections {
            target: core
            onIo_stopped: {
                if (probe.goingOffline) {
                    probe.goingOffline = false
                    findIn(loader.item, 'transports').reload()
                }
            }
        }
        function offline() {
            probe.goingOffline = true
            core.stop_all_account_io()
            return 'ok'
        }
        function pageProperty(property) { return '' + loader.item[property] }
        function subProperty(property) { return '' + sub.item[property] }
        function navigation() { return probe.pageStack.log }
        function pushedAccount() { return '' + probe.pageStack.pushedAccount }
        // Wait before a relay goes, turned down from the four seconds a reader
        // gets.
        function hurry() { loader.item.pendingDelay = 100; return 'ok' }
        // Address with no relay, straight at the object: no row offers it; the
        // core's refusal is what the page shows.
        function misname() {
            var transports = findIn(loader.item, 'transports')
            if (!transports) { return 'missing:transports' }
            transports.remove('nobody@nowhere.example')
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_relays_are_listed_removed_and_added() {
    let temp = std::env::temp_dir().join(format!("piirit-relays-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt and server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        // Two relays: first set up, and the one holding address now.
        std::env::set_var("PIIRIT_FAKE_OLDER_RELAY", "ada@old.example.net");
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! get_in {
        ($row:expr, $name:expr, $property:expr) => {
            call!(
                "getIn",
                QString::from($row),
                QString::from($name),
                QString::from($property)
            )
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("ProfilePage.qml")),
                1
            )
        );
        record!("hurry", call!("hurry"));
    });

    // List as core has it: address holder first, own mailbox each. No row
    // claims mail leaves through it.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("count", get!("transports", "count"));
        record!("first", get!("relayRow0", "addr"));
        record!("first-detail", get!("relayDetail", "objectName"));
        record!("send-from", get!("sendFromItem", "objectName"));
        // Same search reaches row menus.
        record!("menu-found", get!("removeRelayItem", "objectName"));
        record!("second", get!("relayRow1", "addr"));
        record!("second-domain", get_in!("relayRow1", "relayDomain", "text"));
        record!(
            "first-remove",
            get_in!("relayRow0", "removeRelayItem", "enabled")
        );
        record!(
            "second-remove",
            get_in!("relayRow1", "removeRelayItem", "enabled")
        );
        record!("address", get!("profile", "address"));
        // Per relay: core's dot and words, own mailbox.
        record!(
            "first-report",
            get_in!("relayReport0", "reportDomain", "text")
        );
        record!(
            "first-status",
            get_in!("relayReport0", "reportStatus", "text")
        );
        record!("quota", get_in!("relayReport0", "reportQuota", "value"));
        record!(
            "second-report",
            get_in!("relayReport1", "reportDomain", "text")
        );
        record!(
            "second-status",
            get_in!("relayReport1", "reportStatus", "text")
        );
        record!(
            "second-quota",
            get_in!("relayReport1", "reportQuota", "value")
        );
        record!(
            "second-words",
            get_in!("relayReport1", "reportQuota", "label")
        );
        // Remove address with no relay; core refuses.
        record!("misname", call!("misname"));
    });

    // Refusal shown. Address holder asked to go; reader thinks better.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "refused",
            call!("pageProperty", QString::from("errorMessage"))
        );
        record!(
            "remove",
            call!(
                "clickIn",
                QString::from("relayRow0"),
                QString::from("removeRelayItem")
            )
        );
        record!("counting", get_in!("relayRow0", "relayRemorse", "active"));
        record!("doomed", get!("relayRow0", "doomed"));
        record!("call-off", call!("callOff", QString::from("relayRow0")));
        record!("spared", get!("relayRow0", "doomed"));
    });

    // Still two; asked again, let go.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("spared-count", get!("transports", "count"));
        record!(
            "remove-again",
            call!(
                "clickIn",
                QString::from("relayRow0"),
                QString::from("removeRelayItem")
            )
        );
    });

    // One relay, can't go. Address moved with its relay to the one left; page
    // and report follow.
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("removed-count", get!("transports", "count"));
        record!("remaining", get!("relayRow0", "addr"));
        record!("moved-address", get!("profile", "address"));
        record!(
            "moved-report",
            get_in!("relayReport0", "reportDomain", "text")
        );
        record!(
            "moved-quota",
            get_in!("relayReport0", "reportQuota", "value")
        );
        record!(
            "last-remove",
            get_in!("relayRow0", "removeRelayItem", "enabled")
        );
    });

    // Plus: nothing to add until a relay is picked, then core is asked.
    single_shot(Duration::from_secs(10), move || unsafe {
        record!("plus", call!("click", QString::from("addRelayButton")));
        record!(
            "sub-load",
            call!(
                "loadSub",
                QString::from(common::page_url("AddRelayPage.qml")),
                1
            )
        );
        record!(
            "sub-dim",
            call!(
                "getSub",
                QString::from("addButton"),
                QString::from("enabled")
            )
        );
        record!(
            "sub-pick",
            call!("pickSub", QString::from("relayCombo"), QString::from("1"))
        );
        record!(
            "sub-lit",
            call!(
                "getSub",
                QString::from("addButton"),
                QString::from("enabled")
            )
        );
        record!(
            "sub-provider",
            call!("subProperty", QString::from("providerQr"))
        );
        record!("sub-add", call!("clickSub", QString::from("addButton")));
    });

    // Relay on profile: add page gone; list has new row with new relay's mailbox.
    single_shot(Duration::from_secs(12), move || unsafe {
        record!("sub-busy", call!("subProperty", QString::from("busy")));
        record!("added-count", get!("transports", "count"));
        record!("added-first", get!("relayRow0", "addr"));
        record!("added-second", get!("relayRow1", "addr"));
        record!(
            "added-report",
            get_in!("relayReport1", "reportDomain", "text")
        );
        record!(
            "added-quota",
            get_in!("relayReport1", "reportQuota", "value")
        );
        record!("navigation", call!("navigation"));
        record!("pushed-account", call!("pushedAccount"));
        record!("offline", call!("offline"));
    });

    // Offline: core's report has no relay. Read until it says so or out of time; loaded runner
    // may take seconds.
    for second in 13..=GIVE_UP_SECS {
        single_shot(Duration::from_secs(second), move || unsafe {
            let status = get_in!("relayReport0", "reportStatus", "text");
            if second < GIVE_UP_SECS && status != "Not connected" {
                return;
            }
            record!("offline-status", status);
            record!("offline-dot", get_in!("relayReport1", "reportDot", "color"));
            (*engine_ptr).quit();
        });
    }

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let calls = common::calls(&journal);
    let navigation = value("navigation");
    let pushed_account = value("pushed-account");
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the profile page did not load. {context}"
    );
    assert_listed(&value, &context);
    // Core's words behind the transport prefix, as pages show refusals.
    assert_eq!(value("misname"), "ok", "{context}");
    assert!(
        value("refused").ends_with("Transport does not exist"),
        "the core's refusal of an address the profile has no relay for \
         did not reach the page in the core's words. {context}"
    );
    assert!(
        calls.iter().any(|(name, params)| name == "delete_transport"
            && params.pointer("/1").and_then(Value::as_str) == Some(MISNAMED)),
        "the refusal was not the core's: the removal never reached it. {calls:?}"
    );
    assert_removed(&value, &calls, &context);
    assert_added(&value, &calls, &navigation, &pushed_account, &context);
    assert_eq!(value("offline"), "ok", "{context}");
    assert_eq!(
        (
            value("offline-status").as_str(),
            value("offline-dot").as_str()
        ),
        ("Not connected", "#f33b2d"),
        "a profile with IO stopped showed its relays as still being \
         checked, for as long as the phone stays offline. {context}"
    );
}

/// Address holder first, other own mailbox; either may go while two. No row says
/// mail leaves through it, nor offers to make it.
fn assert_listed(value: &dyn Fn(&str) -> String, context: &str) {
    assert_eq!(value("count"), "2", "{context}");
    assert_eq!(
        value("first"),
        "account1@example.org",
        "the relay with the profile's own address is not listed first. {context}"
    );
    assert_eq!(value("menu-found"), "removeRelayItem", "{context}");
    assert_eq!(
        (value("first-detail").as_str(), value("send-from").as_str()),
        ("missing:relayDetail", "missing:sendFromItem"),
        "a row still claims to be the relay mail leaves through, or offers \
         to make it so, which the core no longer lets anyone choose. {context}"
    );
    assert_eq!(value("second"), "ada@old.example.net", "{context}");
    assert_eq!(value("second-domain"), "old.example.net", "{context}");
    assert_eq!(value("first-report"), "example.org", "{context}");
    assert_eq!(
        value("first-status"),
        "Connected",
        "the relay's connection is not said in the core's own words. {context}"
    );
    assert_eq!(value("second-report"), "old.example.net", "{context}");
    assert_eq!(
        value("second-status"),
        "Connecting…",
        "the second relay's connection is not its own. {context}"
    );
    assert_eq!(
        value("second-quota"),
        "95",
        "the second relay's mailbox is not its own. {context}"
    );
    assert_eq!(
        value("second-words"),
        "2.0 GB of 2.1 GB used",
        "the second relay's mailbox is not said as used of the whole. {context}"
    );
    assert_eq!(value("first-remove"), "true", "{context}");
    assert_eq!(value("second-remove"), "true", "{context}");
    assert_eq!(value("address"), "account1@example.org", "{context}");
    assert_eq!(
        value("quota"),
        "67",
        "the first bar is not the mailbox of the relay with the profile's \
         own address. {context}"
    );
}

/// Countdown called off once, let run once; then one relay, no longer removable,
/// address moved to it.
fn assert_removed(value: &dyn Fn(&str) -> String, calls: &[(String, Value)], context: &str) {
    assert_eq!(value("remove"), "ok", "{context}");
    assert_eq!(
        value("counting"),
        "true",
        "asking to remove a relay did not raise Silica's countdown on it. {context}"
    );
    assert_eq!(value("doomed"), "true", "{context}");
    assert_eq!(value("call-off"), "ok", "{context}");
    assert_eq!(
        value("spared"),
        "false",
        "tapping the countdown did not call the removal off. {context}"
    );
    assert_eq!(
        value("spared-count"),
        "2",
        "a removal called off still removed the relay. {context}"
    );
    assert_eq!(value("remove-again"), "ok", "{context}");
    assert_eq!(
        value("removed-count"),
        "1",
        "the relay was not removed once the countdown ran out. {context}"
    );
    assert_eq!(value("remaining"), "ada@old.example.net", "{context}");
    assert_eq!(
        value("moved-address"),
        "ada@old.example.net",
        "the profile's own address did not follow the core to the relay \
         left. {context}"
    );
    assert_eq!(value("moved-report"), "old.example.net", "{context}");
    assert_eq!(
        value("moved-quota"),
        "95",
        "the reports did not follow the order of the rows. {context}"
    );
    assert_eq!(
        value("last-remove"),
        "false",
        "the last relay is offered for removal, which the core refuses. {context}"
    );
    // Address no relay has goes to core too, refused; rest are menu asks.
    let deletions: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "delete_transport")
        .map(|(_, params)| params)
        .filter(|params| params.pointer("/1").and_then(Value::as_str) != Some(MISNAMED))
        .collect();
    assert_eq!(
        deletions.len(),
        1,
        "expected exactly one removal to reach the core -- the one not \
         called off: {calls:?}"
    );
    assert_eq!(
        deletions[0].pointer("/1").and_then(Value::as_str),
        Some("account1@example.org"),
        "the wrong relay was removed"
    );
}

/// Plus opens page for profile; page adds picked relay, then goes; list shows it.
fn assert_added(
    value: &dyn Fn(&str) -> String,
    calls: &[(String, Value)],
    navigation: &str,
    pushed_account: &str,
    context: &str,
) {
    assert_eq!(value("plus"), "ok", "{context}");
    assert!(
        navigation.contains("push:AddRelayPage.qml"),
        "the plus did not open the page that adds a relay. {context}"
    );
    assert_eq!(
        pushed_account, "1",
        "the page that adds a relay was not told which profile. {context}"
    );
    assert_eq!(value("sub-load"), "ok", "{context}");
    assert_eq!(
        value("sub-dim"),
        "false",
        "a relay can be added before one is picked. {context}"
    );
    assert_eq!(value("sub-pick"), "ok", "{context}");
    assert_eq!(value("sub-lit"), "true", "{context}");
    assert_eq!(
        value("sub-provider"),
        "dcaccount:mailchat.pl",
        "the picked relay is not what the core would be handed. {context}"
    );
    assert_eq!(value("sub-add"), "ok", "{context}");
    let added: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "add_transport_from_qr")
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        added.len(),
        1,
        "expected one transport call for the added relay: {calls:?}"
    );
    assert_eq!(
        added[0].pointer("/0").and_then(Value::as_u64),
        Some(1),
        "the relay was added to the wrong profile"
    );
    assert_eq!(
        added[0].pointer("/1").and_then(Value::as_str),
        Some("dcaccount:mailchat.pl")
    );
    assert_eq!(
        value("sub-busy"),
        "false",
        "the page that adds a relay is still waiting after the core answered. {context}"
    );
    assert!(
        navigation.ends_with("pop|"),
        "the page that adds a relay did not go once it had. {context}"
    );
    assert_eq!(
        value("added-count"),
        "2",
        "the added relay did not land in the list. {context}"
    );
    assert_eq!(value("added-first"), "ada@old.example.net", "{context}");
    assert_eq!(
        value("added-second"),
        "account1@mailchat.pl",
        "the added relay is not the one listed. {context}"
    );
    assert_eq!(
        value("added-report"),
        "mailchat.pl",
        "the added relay is not reported on. {context}"
    );
    assert_eq!(
        value("added-quota"),
        "2",
        "the added relay's report does not show its own mailbox. {context}"
    );
    assert!(
        !calls.iter().any(|(name, _)| name == "remove_account"),
        "adding a relay removed a profile: {calls:?}"
    );
}
