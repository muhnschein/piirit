//! Profile page reads profile from core and writes it back.
//!
//! Name, signature, picture are core config keys. Picture is odd: `selfavatar` takes a
//! *path the core copies*, refuses empty string, clears with null. Picker gives `file://`
//! URL: unwrap before core.
//!
//! Rest is parla's profile dialog: relays and address (`relays.rs`; own-address relay
//! first), relay and phone status: space on device, per-relay connection, mailbox quota.
//! Name is a field under picture, nothing under it.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! params match dispatcher; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica `pageStack`, recorded; only which page invite opens is asked.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:QrPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
}

impl PageStackProbe {
    fn push(&mut self, page: QString, _properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page);
        let current = self.log.to_string();
        self.log = format!("{current}push:{name}|").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, accountId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // `data` rather than `children`: the model is a plain QObject, so
        // it is not among an Item's visual children at all.
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function pageProperty(property) { return '' + loader.item[property] }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        // A tap on a MouseArea: its clicked() carries the event, and
        // QML refuses to emit it without one.
        // Put the cursor in a name field, as a tap on it does.
        function edit(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.edit()
            return '' + item.editing
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked(null)
            return 'ok'
        }
        function toggleReceipts() {
            var sw = findIn(loader.item, 'readReceiptsSwitch')
            if (!sw) { return 'missing:readReceiptsSwitch' }
            // A tap and nothing else. With automaticCheck off, Silica
            // leaves `checked` to its binding, so the page has to read
            // the state it shows and ask the core for the other one. A
            // test that flipped `checked` first was standing in for the
            // behaviour the page now switches off.
            sw.clicked()
            return 'ok'
        }
        /// Leaving the page is what applies what was typed.
        function leave() {
            loader.item.status = PageStatus.Deactivating
            return 'ok'
        }
        function pick(path) {
            var profile = findIn(loader.item, 'profile')
            if (!profile) { return 'missing:profile' }
            profile.set_picture(path)
            return 'ok'
        }
        function unpick() {
            var profile = findIn(loader.item, 'profile')
            if (!profile) { return 'missing:profile' }
            profile.clear_picture()
            return 'ok'
        }
        // What the profiles page under this one does when it opens, so
        // its rows are there to go stale.
        function listProfiles() { core.refresh_accounts(); return 'ok' }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_profile_page_round_trips_the_profile() {
    let temp = std::env::temp_dir().join(format!("piirit-profile-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Profile core lists, so profiles page row for it exists to watch.
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let core_ptr: *const QObjectBox<DeltaChatCore> = std::ptr::addr_of!(core_box);
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    // Row profiles page draws: name and picture as core's account list has them.
    macro_rules! listed {
        () => {
            listed_row(&(*core_ptr).pinned().borrow(), 1)
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
        record!("listed", call!("listProfiles"));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // Row before anything typed.
        record!("listed-before", listed!());
        // Relay and phone status, as parla's dialog shows. Address: first relay row's.
        record!("address", get!("relayRow0", "addr"));
        // That relay reported first: core's connection words, and mailbox.
        record!("connection", get!("reportStatus", "text"));
        record!("quota-shown", get!("reportQuota", "visible"));
        record!("quota-words", get!("reportQuota", "label"));
        record!("quota-value", get!("reportQuota", "value"));
        record!("storage", get!("storageLabel", "text"));
        // Name: badge, and field with hint once badge tapped.
        record!("hint-before", get!("nameHint", "visible"));
        record!("field-before", get!("profileNameField", "visible"));
        record!("edit", call!("edit", QString::from("profileNameControl")));
        record!("hint-editing", get!("nameHint", "visible"));
        record!("field-editing", get!("profileNameField", "visible"));
        record!(
            "typed-name",
            call!(
                "setText",
                QString::from("profileNameField"),
                QString::from("Ada Lovelace")
            )
        );
        record!(
            "typed-status",
            call!(
                "setText",
                QString::from("profileBioField"),
                QString::from("Counting on it")
            )
        );
        record!(
            "edited-after-typing",
            call!("pageProperty", QString::from("edited"))
        );
    });

    // Pause after typing is the mechanism: wait it out, nothing tapped.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("autosaved", call!("pageProperty", QString::from("edited")));
        record!("receipts-before", get!("profile", "read_receipts"));
        record!("toggled", call!("toggleReceipts"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("saved-name", get!("profile", "display_name"));
        record!("saved-bio", get!("profile", "status"));
        // Profiles page under this one: its row, not reopened.
        record!("listed-name", listed!());
        // Reopened on named profile. Filling from core must not read as typing: leaving would
        // write back; a page left before load landed would write empties.
        record!(
            "reopen",
            call!(
                "load",
                QString::from(common::page_url("ProfilePage.qml")),
                1
            )
        );
    });

    // Picker hands URL; core wants path.
    single_shot(Duration::from_secs(10), move || unsafe {
        record!("receipts-after", get!("profile", "read_receipts"));
        record!("refilled", get!("profileNameField", "text"));
        // Reopened page holds saved name.
        record!("name-reopened", get!("profileNameField", "text"));
        record!("hint-reopened", get!("nameHint", "visible"));
        record!(
            "edited-after-refill",
            call!("pageProperty", QString::from("edited"))
        );
        record!(
            "picked",
            call!("pick", QString::from("file:///tmp/piirit-fake/photo.jpg"))
        );
    });

    single_shot(Duration::from_secs(12), move || unsafe {
        record!("picture", get!("profile", "avatar_path"));
        record!("listed-picture", listed!());
        record!("unpicked", call!("unpick"));
    });

    single_shot(Duration::from_secs(14), move || unsafe {
        record!("picture-gone", get!("profile", "avatar_path"));
        record!("listed-picture-gone", listed!());
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
    let navigation = stack_box.pinned().borrow().log.to_string();
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");

    assert_eq!(
        value("load"),
        "ok",
        "the profile page did not load. {context}"
    );
    assert_page_says_what_the_relay_said(&steps, &context);
    assert_eq!(
        value("edited-after-typing"),
        "true",
        "typing did not mark the page edited, so nothing would be saved. {context}"
    );
    assert_eq!(
        value("autosaved"),
        "false",
        "typing was never written back on its own, so a settings page with \
         nothing to confirm loses what was typed. {context}"
    );
    assert_eq!(value("toggled"), "ok", "no read-receipt switch. {context}");
    assert_eq!(
        value("receipts-before"),
        "true",
        "read receipts did not load as on, which is the core's default. \
         {context}"
    );
    assert_eq!(
        value("receipts-after"),
        "false",
        "turning read receipts off did not reach the core. {context}"
    );

    assert_eq!(
        value("saved-name"),
        "Ada Lovelace",
        "the name did not reach the core. {context}"
    );
    assert_eq!(
        value("saved-bio"),
        "Counting on it",
        "the bio did not reach the core. {context}"
    );

    assert_eq!(
        value("reopen"),
        "ok",
        "the page did not load a second time. {context}"
    );
    assert_eq!(
        value("refilled"),
        "Ada Lovelace",
        "reopening the page did not show the name that was saved. {context}"
    );
    assert_eq!(
        value("edited-after-refill"),
        "false",
        "filling the fields from the core counted as the reader typing, so \
         leaving the page would write the profile straight back over itself \
         -- and a page left before the load landed would write empties. \
         {context}"
    );

    // Picture is a path; picker URL is not.
    let avatar_sets: Vec<&Value> = calls
        .iter()
        .filter(|(name, params)| {
            name == "set_config"
                && params.pointer("/1").and_then(Value::as_str) == Some("selfavatar")
        })
        .map(|(_, params)| params)
        .collect();
    assert_eq!(
        avatar_sets.len(),
        2,
        "expected the picture to be set once and cleared once: {calls:?}"
    );
    assert_eq!(
        avatar_sets[0].pointer("/2").and_then(Value::as_str),
        Some("/tmp/piirit-fake/photo.jpg"),
        "the file:// the picker hands back went to the core as it stood; the \
         core takes a path and refuses anything it cannot open"
    );
    assert_eq!(
        avatar_sets[1].pointer("/2"),
        Some(&Value::Null),
        "clearing the picture sent something other than null. The core \
         refuses an empty string outright -- \"Copying new blobfile failed\""
    );

    assert_eq!(
        value("picture"),
        "/tmp/piirit-fake/photo.jpg",
        "the picture was set but never read back. {context}"
    );
    assert_eq!(
        value("picture-gone"),
        "",
        "the picture was cleared but the page still shows one. {context}"
    );
    assert_profiles_row_follows(&steps, &context);
}

/// Profiles page row for profile: `name|picture`, or note that list has no such row.
fn listed_row(core: &DeltaChatCore, account_id: u32) -> String {
    core.account_list
        .borrow()
        .iter()
        .find(|row| row.account_id == account_id)
        .map_or_else(
            || "<not listed>".to_string(),
            |row| format!("{}|{}", row.display_name, row.avatar_path),
        )
}

/// Profiles page draws rows off core's account list; saved name or picture must reach
/// that row before swipe back.
fn assert_profiles_row_follows(steps: &[(&str, String)], context: &str) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.as_str())
            .unwrap_or_default()
    };
    assert_eq!(
        value("listed-before"),
        "|",
        "the profiles page's row was not there before the edit, so the rest \
         of this proves nothing. {context}"
    );
    assert_eq!(
        value("listed-name"),
        "Ada Lovelace|",
        "the name was saved but the profiles page's row still shows the old \
         one: it changes only when that page is opened again. {context}"
    );
    assert_eq!(
        value("listed-picture"),
        "Ada Lovelace|/tmp/piirit-fake/photo.jpg",
        "the picture was set but the profiles page's row does not show it \
         yet. {context}"
    );
    assert_eq!(
        value("listed-picture-gone"),
        "Ada Lovelace|",
        "the picture was cleared but the profiles page's row still has it. \
         {context}"
    );
}

/// Address, connection, mailbox as fake relay reports. Invite is on profile's row, checked there.
fn assert_page_says_what_the_relay_said(steps: &[(&str, String)], context: &str) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        value("address"),
        "account1@example.org",
        "the first relay row is not the profile's own address. {context}"
    );
    assert_eq!(
        value("connection"),
        "Connected",
        "the relay's connection is not said in the core's own words. {context}"
    );
    assert_eq!(
        value("quota-shown"),
        "true",
        "the relay reported a quota and the page shows none. {context}"
    );
    assert_eq!(
        value("quota-words"),
        "1.4 GB of 2.1 GB used",
        "the mailbox is not said as used of the whole. {context}"
    );
    for (label, expected) in [
        ("hint-before", "false"),
        // No hint under name: help to one reader, subtitle to another.
        ("field-before", "true"),
        ("edit", "true"),
        ("hint-editing", "false"),
        ("field-editing", "true"),
        ("hint-reopened", "false"),
    ] {
        assert_eq!(
            value(label),
            expected,
            "the name is not a field that says what it is for while it is \
             being typed in: {label}. {context}"
        );
    }
    assert_eq!(
        value("name-reopened"),
        "Ada Lovelace",
        "the name reopened as something other than the saved one. {context}"
    );
    assert_eq!(
        value("quota-value"),
        "67",
        "the bar does not show the percentage the core wrote. {context}"
    );
    assert_eq!(
        value("storage"),
        "Piirit uses 123.5 kB of storage on this phone.",
        "the space the profile takes on the device is not said. {context}"
    );
}
