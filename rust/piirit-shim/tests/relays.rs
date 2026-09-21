//! The relays on the profile page, and what can be done with them.
//!
//! A profile can be reached through several relays at once, and sends
//! from one of them. The page lists them with the one sent from first,
//! and reports on each by itself -- the core's own words about its
//! connection, and its mailbox off the core's report; a row's menu sends
//! from that relay instead, or removes it after Silica's countdown; the
//! last relay cannot be removed; and the plus under the rows opens the
//! page that adds one, whose answer lands back in the list. The core's
//! refusals reach the page in the core's own words.

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

/// Two loaders: the profile page, and the page that adds a relay over
/// it, so that what the second does can be seen landing on the first.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe

        // Silica's `pageStack`, recorded rather than performed: which
        // page the plus opens and with which profile, and that the page
        // that adds a relay goes away once it has. In QML rather than as
        // a QObject on the Rust side, since a page loaded by a Loader
        // reads `pageStack` off the file the Loader was declared in.
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
        // `data` rather than `children`: the shim objects are plain
        // QObjects, so they are not among an Item's visual children.
        // A row's ContextMenu is held in a property, so it is in neither
        // list even though it is instantiated.
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
        // Something inside a row: the labels, the menu's items, the
        // countdown.
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
        // What Silica's ComboBox does on a tap of one of its items.
        function pickSub(name, index) {
            var item = findIn(sub.item, name)
            if (!item) { return 'missing:' + name }
            item.currentIndex = parseInt(index)
            return 'ok'
        }
        function pageProperty(property) { return '' + loader.item[property] }
        function subProperty(property) { return '' + sub.item[property] }
        function navigation() { return probe.pageStack.log }
        function pushedAccount() { return '' + probe.pageStack.pushedAccount }
        // The wait before a relay goes, turned down from the four
        // seconds a reader gets.
        function hurry() { loader.item.pendingDelay = 100; return 'ok' }
        // An address the profile has no relay for, straight at the
        // object: no row offers it, and the core's refusal is what the
        // page has to show.
        function misname() {
            var transports = findIn(loader.item, 'transports')
            if (!transports) { return 'missing:transports' }
            transports.set_primary('nobody@nowhere.example')
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_relays_are_listed_switched_removed_and_added() {
    let temp = std::env::temp_dir().join(format!("piirit-relays-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
        // A profile on two relays: the one it was set up on first, and
        // the one it sends from now.
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

    // The list as the core has it: the relay sent from first, whatever
    // the core's order, each with its own mailbox.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("count", get!("transports", "count"));
        record!("first", get!("relayRow0", "addr"));
        record!("first-sends", get!("relayRow0", "sendsFrom"));
        record!("first-detail", get_in!("relayRow0", "relayDetail", "text"));
        record!(
            "first-switch",
            get_in!("relayRow0", "sendFromItem", "visible")
        );
        record!("second", get!("relayRow1", "addr"));
        record!("second-domain", get_in!("relayRow1", "relayDomain", "text"));
        record!(
            "second-detail",
            get_in!("relayRow1", "relayDetail", "visible")
        );
        record!(
            "second-switch",
            get_in!("relayRow1", "sendFromItem", "visible")
        );
        record!(
            "second-remove",
            get_in!("relayRow1", "removeRelayItem", "enabled")
        );
        record!("address", get!("profile", "address"));
        // Reported on in the same order, each relay by itself: the
        // core's dot and words for its connection, and its own mailbox.
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
        record!(
            "switch",
            call!(
                "clickIn",
                QString::from("relayRow1"),
                QString::from("sendFromItem")
            )
        );
    });

    // Sent from the older relay now: it is first, the profile's address
    // is on it, and the bar is its mailbox. Then the other one is asked
    // to go, and the reader thinks better of it.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("switched-first", get!("relayRow0", "addr"));
        record!("switched-first-sends", get!("relayRow0", "sendsFrom"));
        record!("switched-second", get!("relayRow1", "addr"));
        record!("switched-address", get!("profile", "address"));
        record!(
            "switched-report",
            get_in!("relayReport0", "reportDomain", "text")
        );
        record!(
            "switched-quota",
            get_in!("relayReport0", "reportQuota", "value")
        );
        record!(
            "remove",
            call!(
                "clickIn",
                QString::from("relayRow1"),
                QString::from("removeRelayItem")
            )
        );
        record!("counting", get_in!("relayRow1", "relayRemorse", "active"));
        record!("doomed", get!("relayRow1", "doomed"));
        record!("call-off", call!("callOff", QString::from("relayRow1")));
        record!("spared", get!("relayRow1", "doomed"));
    });

    // Still two; asked again, and this time let go.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("spared-count", get!("transports", "count"));
        record!(
            "remove-again",
            call!(
                "clickIn",
                QString::from("relayRow1"),
                QString::from("removeRelayItem")
            )
        );
    });

    // One relay, which cannot be removed; and a switch to an address
    // the profile has no relay for, refused by the core.
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("removed-count", get!("transports", "count"));
        record!("remaining", get!("relayRow0", "addr"));
        record!(
            "last-remove",
            get_in!("relayRow0", "removeRelayItem", "enabled")
        );
        record!("misname", call!("misname"));
    });

    // The plus, and the page it opens: nothing to add until a relay is
    // picked, and then the core is asked.
    single_shot(Duration::from_secs(10), move || unsafe {
        record!(
            "refused",
            call!("pageProperty", QString::from("errorMessage"))
        );
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

    // The relay is on the profile: the page that added it is gone, and
    // the list under it has the new row, with the mailbox the new relay
    // reports.
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
    let navigation = value("navigation");
    let pushed_account = value("pushed-account");
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the profile page did not load. {context}"
    );
    assert_listed(&value, &context);
    assert_switched(&value, &context);
    assert_removed(&value, &calls, &context);
    // The core's words, behind the transport's own prefix, as every
    // page shows a refusal.
    assert!(
        value("refused").ends_with("Address does not belong to any transport."),
        "the core's refusal of an address the profile has no relay for \
         did not reach the page in the core's words. {context}"
    );
    assert_added(&value, &calls, &navigation, &pushed_account, &context);
}

/// The relay sent from first, marked as such and without the item that
/// would switch to it; the other with its own mailbox and both items.
fn assert_listed(value: &dyn Fn(&str) -> String, context: &str) {
    assert_eq!(value("count"), "2", "{context}");
    assert_eq!(
        value("first"),
        "account1@example.org",
        "the relay the profile sends from is not listed first. {context}"
    );
    assert_eq!(value("first-sends"), "true", "{context}");
    assert_eq!(
        value("first-detail"),
        "Sends from this relay",
        "the first row does not say the profile sends from it. {context}"
    );
    assert_eq!(
        value("first-switch"),
        "false",
        "the relay already sent from offers to be sent from. {context}"
    );
    assert_eq!(value("second"), "ada@old.example.net", "{context}");
    assert_eq!(value("second-domain"), "old.example.net", "{context}");
    assert_eq!(
        value("second-detail"),
        "false",
        "a relay not sent from carries a marker. {context}"
    );
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
    assert_eq!(value("second-switch"), "true", "{context}");
    assert_eq!(value("second-remove"), "true", "{context}");
    assert_eq!(value("address"), "account1@example.org", "{context}");
    assert_eq!(
        value("quota"),
        "67",
        "the first bar is not the mailbox of the relay sent from. {context}"
    );
}

/// After "send from this relay" on the second row: it is first, the
/// profile's address is on it, and the bar is its mailbox.
fn assert_switched(value: &dyn Fn(&str) -> String, context: &str) {
    assert_eq!(value("switch"), "ok", "{context}");
    assert_eq!(
        value("switched-first"),
        "ada@old.example.net",
        "sending from the other relay did not put it first. {context}"
    );
    assert_eq!(value("switched-first-sends"), "true", "{context}");
    assert_eq!(
        value("switched-second"),
        "account1@example.org",
        "{context}"
    );
    assert_eq!(
        value("switched-address"),
        "ada@old.example.net",
        "the profile's address did not follow the relay it sends from. {context}"
    );
    assert_eq!(value("switched-report"), "old.example.net", "{context}");
    assert_eq!(
        value("switched-quota"),
        "95",
        "the reports did not follow the order of the rows. {context}"
    );
}

/// The countdown, called off once and let run once; then one relay,
/// which the menu no longer offers to remove.
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
        value("last-remove"),
        "false",
        "the last relay is offered for removal, which the core refuses. {context}"
    );
    let deletions: Vec<&Value> = calls
        .iter()
        .filter(|(name, _)| name == "delete_transport")
        .map(|(_, params)| params)
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

/// The plus opens the page for this profile; the page adds the picked
/// relay and goes; the list under it has the new row.
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
        "dcaccount:mehl.cloud",
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
        Some("dcaccount:mehl.cloud")
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
        "account1@mehl.cloud",
        "the added relay is not the one listed. {context}"
    );
    assert_eq!(
        value("added-report"),
        "mehl.cloud",
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
