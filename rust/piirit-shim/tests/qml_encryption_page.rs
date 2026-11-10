//! Encryption info page: what core says about a contact's or group's keys, laid out to compare.
//! Contact: status, own key and theirs, relays as domains. Group: each member's key, "Key missing"
//! where core says so. Copy takes names and fingerprints, never an address.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader; width: 540; height: 960 }
        function load(url, contactId, chatId) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: 1, contactId: contactId, chatId: chatId,
                                    name: 'from the page' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function clip() { return Clipboard.text }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                var inside = findIn(node.contentItem, name)
                if (inside) { return inside }
            }
            if (node.headerItem) {
                var inHeader = findIn(node.headerItem, name)
                if (inHeader) { return inHeader }
            }
            if (node.footerItem) {
                return findIn(node.footerItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // A part of one key's entry: every entry has one.
        function getIn(entry, name, property) {
            var item = findIn(findIn(loader.item, entry), name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

/// Fake core's fingerprint for a contact: same sum as `fake_fingerprint` there.
fn fingerprint(contact: u32) -> String {
    let hex = format!(
        "{:040X}",
        u128::from(contact).wrapping_mul(0x9E37_79B9_7F4A_7C15)
    );
    let groups: Vec<&str> = (0..10)
        .map(|group| &hex[group * 4..group * 4 + 4])
        .collect();
    format!("{}\n{}", groups[..5].join(" "), groups[5..].join(" "))
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_encryption_page_lays_keys_out_to_compare() {
    let temp = std::env::temp_dir().join(format!("piirit-encryption-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Group: Ada and member0; core has no key for the last.
        std::env::set_var("PIIRIT_FAKE_CROWD", "1");
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
            let result = (*engine_ptr).invoke_method($name.into(), &[$(QVariant::from($arg)),*]);
            QString::from_qvariant(result).map(|value| value.to_string()).unwrap_or_default()
        }};
    }
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! get_in {
        ($entry:expr, $name:expr, $property:expr) => {
            call!(
                "getIn",
                QString::from($entry),
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

    let page = common::page_url("EncryptionPage.qml");
    let core_ptr = std::ptr::addr_of!(core_box);
    let mut plan = common::Plan::default();
    // SAFETY for every block: fire only inside `exec()`; `engine`, `core_box`, `steps` outlive it.
    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    let url = page.clone();
    plan.after(0, move || unsafe {
        record!(
            "load-contact",
            call!("load", QString::from(url.clone()), 10, 0)
        );
    });
    let url = page;
    plan.until(1, 20, move |last| unsafe {
        if get!("info", "loaded") != "true" && !last {
            return false;
        }
        record!("contact-parsed", get!("info", "parsed"));
        record!("contact-status", get!("statusLabel", "text"));
        record!("contact-raw", get!("rawText", "visible"));
        record!("contact-keys", get!("keyList", "count"));
        record!("own-fingerprint", get_in!("key0", "fingerprint", "text"));
        record!("their-fingerprint", get_in!("key1", "fingerprint", "text"));
        record!("contact-relays", get!("relays", "text"));
        record!("contact-menu", get!("encryptionMenu", "visible"));
        record!("copy", call!("click", QString::from("copyFingerprints")));
        record!("clipboard", call!("clip"));
        record!("copied", get!("noticeLabel", "text"));
        record!(
            "load-group",
            call!("load", QString::from(url.clone()), 0, 2)
        );
        true
    });
    plan.until(1, 20, move |last| unsafe {
        if get!("info", "loaded") != "true" && !last {
            return false;
        }
        record!("group-keys", get!("keyList", "count"));
        record!("first-name", get_in!("key0", "memberName", "name"));
        record!("first-relays", get_in!("key0", "memberRelays", "text"));
        record!("first-missing", get_in!("key0", "keyMissing", "visible"));
        record!("first-fingerprint", get_in!("key0", "fingerprint", "text"));
        record!("last-name", get_in!("key1", "memberName", "name"));
        record!("last-missing", get_in!("key1", "keyMissing", "visible"));
        (*engine_ptr).quit();
        true
    });
    plan.run();
    engine.exec();

    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    for label in ["load-contact", "copy", "load-group"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    let ada = fingerprint(10);
    for (label, expected, complaint) in [
        (
            "contact-parsed",
            "true",
            "the page did not read core's text",
        ),
        (
            "contact-status",
            "Messages are end-to-end encrypted.",
            "core's status line is not shown",
        ),
        (
            "contact-raw",
            "false",
            "core's raw text shows over the laid-out keys",
        ),
        ("contact-keys", "2", "the page does not show both keys"),
        (
            "own-fingerprint",
            fingerprint(1).as_str(),
            "core's key order is not kept",
        ),
        (
            "their-fingerprint",
            ada.as_str(),
            "the contact's key is not shown",
        ),
        (
            "contact-relays",
            "example.org",
            "relays are not shown as domains",
        ),
        ("contact-menu", "true", "the page offers no copy"),
        ("copied", "Copied to clipboard", "copy did not say so"),
        (
            "group-keys",
            "2",
            "the group page does not show every member but the reader",
        ),
        ("first-name", "ada", "the first member is not named"),
        (
            "first-relays",
            "example.org",
            "a member's relays are not shown as domains",
        ),
        (
            "first-missing",
            "false",
            "a member with a key is marked missing",
        ),
        (
            "first-fingerprint",
            ada.as_str(),
            "a member's key is not shown",
        ),
        ("last-name", "member0", "the last member is not named"),
        (
            "last-missing",
            "true",
            "a member whose key core lacks is not marked",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    let clipboard = value("clipboard");
    assert!(
        clipboard.contains(&ada) && clipboard.contains("ada") && !clipboard.contains('@'),
        "the copy is not names and fingerprints, or carries an address. {context}"
    );
    let _ = std::fs::remove_dir_all(&temp);
}
