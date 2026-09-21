//! The profile page reads the profile out of the core and writes it back.
//!
//! Name, signature and picture are core config keys rather than a record
//! of their own, and the picture is the odd one: `selfavatar` takes a
//! *path to a file the core copies*, refuses an empty string outright, and
//! is cleared with null. A picker hands back a `file://` URL, so the path
//! has to be unwrapped before it goes anywhere near the core.
//!
//! The rest of the page is what parla's profile dialog shows: the relays
//! and the address on each (their own test is `relays.rs`; here only that
//! the one sent from is listed first), and what the relays and the phone
//! say about the profile -- the space taken on the device, and per relay
//! the connection in the core's own words and the mailbox quota read off
//! its report, said as used of the whole. The name is a field under the
//! picture, with nothing under it.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded rather than performed: only which page
/// the invite button opens is asked about.
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

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // A profile the core lists, so the profiles page's row for it
        // is there to be watched.
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
    // The row the profiles page draws for this profile: name and
    // picture as the core's account list has them right now.
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
        // The row as it stands before anything is typed.
        record!("listed-before", listed!());
        // What the relay and the phone say, as parla's dialog shows it.
        // The address is the first relay row's: the profile page lists
        // the relays now, the one sent from first (tests/relays.rs).
        record!("address", get!("relayRow0", "addr"));
        record!("sends-from", get!("relayRow0", "sendsFrom"));
        // The relay the profile sends from is reported on first: the
        // core's own words about its connection, and its mailbox.
        record!("connection", get!("reportStatus", "text"));
        record!("quota-shown", get!("reportQuota", "visible"));
        record!("quota-words", get!("reportQuota", "label"));
        record!("quota-value", get!("reportQuota", "value"));
        record!("storage", get!("storageLabel", "text"));
        // The name: a name with a badge, and a field with a hint once
        // the badge is tapped.
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

    // Nothing is confirmed and nothing is tapped: the pause after typing
    // is the whole mechanism, so this waits it out rather than saving.
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("autosaved", call!("pageProperty", QString::from("edited")));
        record!("receipts-before", get!("profile", "read_receipts"));
        record!("toggled", call!("toggleReceipts"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("saved-name", get!("profile", "display_name"));
        record!("saved-bio", get!("profile", "status"));
        // The profiles page under this one: its row, without it being
        // opened again.
        record!("listed-name", listed!());
        // Opened again, on a profile that now has a name in it. Filling
        // the fields from the core must not read as the reader typing:
        // leaving the page would then write them straight back, and a
        // page left before the load landed would write empties.
        record!(
            "reopen",
            call!(
                "load",
                QString::from(common::page_url("ProfilePage.qml")),
                1
            )
        );
    });

    // A picker hands back a URL; the core wants a path.
    single_shot(Duration::from_secs(10), move || unsafe {
        record!("receipts-after", get!("profile", "read_receipts"));
        record!("refilled", get!("profileNameField", "text"));
        // A page opened anew holds the saved name.
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

    // The picture is a path, and the URL a picker hands over is not one.
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

/// The row the profiles page draws for the profile: `name|picture`, or
/// a note that the list has no such row.
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

/// The profiles page under this one draws its rows off the core's
/// account list, and a name or picture saved here has to reach that row
/// before the swipe back, not on the next visit.
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

/// The address, and the connection and the mailbox as the fake relay
/// reports them.
///
/// The invite is not here any more: it is on the profile's row on the
/// profiles page, with the backup, and is checked there.
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
        "the first relay row is not the address the profile sends from. {context}"
    );
    assert_eq!(
        value("sends-from"),
        "true",
        "the first relay row is not marked as the one sent from. {context}"
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
        // No hint under the name any more: what read as help to one
        // reader read as a subtitle to another.
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
