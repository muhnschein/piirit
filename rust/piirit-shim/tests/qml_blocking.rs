//! The way into the block list, and the two pages behind it.
//!
//! The settings page carries the only row on it that opens a page rather
//! than setting something, and hands it the profile: the core keeps a
//! block list per account. Behind it are the blocked contacts, the
//! picker its plus opens, and the question each of them asks before
//! anything is sent. This drives the whole of that against the fake core,
//! with a page stack that hands each page a dialog to connect to and lets
//! the test accept it -- the arrangement `qml_auto_delete_flow` uses.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// The probe imports the app's components by absolute URL: the settings
/// page reads the `Settings` singleton, and it is loaded from data, which
/// has no directory of its own to resolve a relative import against.
///
/// The page stack is a QML object handed to the page as a context
/// property rather than a Rust one, for the reason
/// `qml_auto_delete_flow` gives: an object a method hands to QML is
/// QML's to delete. A fresh dialog per push, so accepting one cannot
/// fire a handler another page connected earlier.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}

        Component {{
            id: dialogs
            QtObject {{
                signal accepted()
                signal rejected()
                function accept() {{ accepted() }}
            }}
        }}

        QtObject {{
            id: stack
            /// The last push, as `Page.qml:key=value,...` with the keys
            /// in name order.
            property string pushed: ''
            property QtObject dialog: null
            function push(url, props) {{
                var name = ('' + url).split('/').pop()
                var parts = []
                for (var key in props) {{ parts.push(key + '=' + props[key]) }}
                parts.sort()
                stack.pushed = name + ':' + parts.join(',')
                stack.dialog = dialogs.createObject(stack)
                return stack.dialog
            }}
        }}

        function stackObject() {{ return stack }}
        function pushed() {{ return stack.pushed }}
        function acceptDialog() {{
            if (!stack.dialog) {{ return 'no-dialog' }}
            stack.dialog.accept()
            return 'ok'
        }}
        function load(url, accountId) {{
            stack.pushed = ''
            loader.setSource(url, {{ accountId: accountId }})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
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
        /// A row held on to, to tell later whether it is still the same
        /// row or one built again in its place.
        property var kept: null
        function keep(name) {{
            kept = findIn(loader.item, name)
            return kept ? 'ok' : 'missing:' + name
        }}
        function same(name) {{
            return '' + (kept !== null && findIn(loader.item, name) === kept)
        }}
        function reload(name) {{
            var list = findIn(loader.item, name)
            if (!list) {{ return 'missing:' + name }}
            list.reload()
            return 'ok'
        }}
    }}
",
        components.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_contact_is_blocked_and_let_back_in_from_the_settings() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-blocking-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them.
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
    engine.load_data(QByteArray::from(probe_qml()));
    // Named for the page before it is loaded, as Silica names its own.
    let stack = engine.invoke_method("stackObject".into(), &[]);
    engine.set_property("pageStack".into(), stack);

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let journal_ptr: *const PathBuf = std::ptr::addr_of!(journal);
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    // The settings page: the row is there, between the notifications and
    // the links, and it opens the block list of the profile it was given.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load-settings",
            call!(
                "load",
                QString::from(common::page_url("SettingsPage.qml")),
                1
            )
        );
        record!(
            "settings-row",
            call!(
                "get",
                QString::from("blockedContactsLabel"),
                QString::from("text")
            )
        );
        record!(
            "open-blocked",
            call!("click", QString::from("blockedContactsEntry"))
        );
        record!("pushed-blocked", call!("pushed"));
    });

    // Nobody is blocked yet, and the plus is the way to change that --
    // once the core has said so, and not while the list is still empty
    // for want of an answer.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "load-blocked",
            call!(
                "load",
                QString::from(common::page_url("BlockedContactsPage.qml")),
                1
            )
        );
        record!(
            "plus-before-answer",
            call!(
                "get",
                QString::from("blockSomeone"),
                QString::from("visible")
            )
        );
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        record!(
            "blocked-empty",
            call!("get", QString::from("blocked"), QString::from("count"))
        );
        record!(
            "placeholder",
            call!(
                "get",
                QString::from("nobodyBlocked"),
                QString::from("enabled")
            )
        );
        record!(
            "plus-shown",
            call!(
                "get",
                QString::from("blockSomeone"),
                QString::from("visible")
            )
        );
        record!("plus", call!("click", QString::from("blockSomeone")));
        record!("pushed-picker", call!("pushed"));
        record!(
            "load-picker",
            call!(
                "load",
                QString::from(common::page_url("BlockContactPage.qml")),
                1
            )
        );
    });

    // A tap asks first, and nothing is sent until the question is
    // answered.
    single_shot(Duration::from_secs(8), move || unsafe {
        record!(
            "contacts-before",
            call!("get", QString::from("contacts"), QString::from("count"))
        );
        record!("tap-contact", call!("click", QString::from("contactRow10")));
        record!("pushed-question", call!("pushed"));
        record!(
            "blocked-before-answer",
            blocked_calls(&*journal_ptr).to_string()
        );
        record!("accept-block", call!("acceptDialog"));
    });
    single_shot(Duration::from_secs(10), move || unsafe {
        // The core keeps a blocked contact out of this list, so the row
        // goes by itself once the block lands.
        record!(
            "contacts-after",
            call!("get", QString::from("contacts"), QString::from("count"))
        );
        record!(
            "load-blocked-again",
            call!(
                "load",
                QString::from(common::page_url("BlockedContactsPage.qml")),
                1
            )
        );
    });
    // The plus stays under the last row once there is one, and reading
    // the list again leaves the rows where they are: rebuilt, they would
    // take the view back to its top, away from the plus.
    single_shot(Duration::from_secs(12), move || unsafe {
        record!(
            "blocked-count",
            call!("get", QString::from("blocked"), QString::from("count"))
        );
        record!(
            "plus-with-rows",
            call!(
                "get",
                QString::from("blockSomeone"),
                QString::from("visible")
            )
        );
        record!(
            "plus-y",
            call!("get", QString::from("blockSomeone"), QString::from("y"))
        );
        record!(
            "row-y",
            call!("get", QString::from("blockedRow10"), QString::from("y"))
        );
        record!("keep-row", call!("keep", QString::from("blockedRow10")));
        record!("reload-blocked", call!("reload", QString::from("blocked")));
    });
    single_shot(Duration::from_secs(14), move || unsafe {
        record!("row-kept", call!("same", QString::from("blockedRow10")));
        record!("tap-blocked", call!("click", QString::from("blockedRow10")));
        record!("pushed-unblock", call!("pushed"));
        record!("accept-unblock", call!("acceptDialog"));
    });
    single_shot(Duration::from_secs(16), move || unsafe {
        record!(
            "blocked-after",
            call!("get", QString::from("blocked"), QString::from("count"))
        );
        record!(
            "error",
            call!("get", QString::from("errorBanner"), QString::from("text"))
        );
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
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}, calls: {names:?}");

    for label in [
        "load-settings",
        "load-blocked",
        "load-picker",
        "load-blocked-again",
        "open-blocked",
        "plus",
        "tap-contact",
        "keep-row",
        "reload-blocked",
        "tap-blocked",
        "accept-block",
        "accept-unblock",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    assert_eq!(
        value("settings-row"),
        "Blocked contacts",
        "the settings page has no row leading to the block list. {context}"
    );
    assert_eq!(
        value("pushed-blocked"),
        "BlockedContactsPage.qml:accountId=1",
        "the settings row did not open the block list of the profile it was \
         given. {context}"
    );
    assert_eq!(
        (value("blocked-empty"), value("placeholder")),
        ("0".to_string(), "true".to_string()),
        "a profile that has blocked nobody did not say so. {context}"
    );
    assert_eq!(
        (value("plus-before-answer"), value("plus-shown")),
        ("false".to_string(), "true".to_string()),
        "the plus was offered before the core had said who is blocked, or not \
         once it had. {context}"
    );
    assert_eq!(
        value("pushed-picker"),
        "BlockContactPage.qml:accountId=1",
        "the plus did not open the contacts to pick from. {context}"
    );
    assert_eq!(
        value("contacts-before"),
        "2",
        "the picker did not list the account's contacts. {context}"
    );
    assert_eq!(
        value("pushed-question"),
        "BlockContactDialog.qml:blocking=true,contactName=ada",
        "tapping a contact did not ask about blocking them by name. {context}"
    );
    assert_eq!(
        value("blocked-before-answer"),
        "0",
        "the contact was blocked before the reader answered the question. \
         {context}"
    );
    assert_eq!(
        value("contacts-after"),
        "1",
        "the blocked contact stayed in the picker. {context}"
    );
    assert_eq!(
        value("blocked-count"),
        "1",
        "the blocked contact is not on the blocked list. {context}"
    );
    let y = |label: &str| value(label).parse::<f64>().unwrap_or(f64::NAN);
    assert!(
        value("plus-with-rows") == "true" && y("plus-y") > y("row-y"),
        "the plus is not under the blocked contacts once there are some. \
         {context}"
    );
    assert_eq!(
        value("row-kept"),
        "true",
        "reading the block list again built its rows anew, which takes the \
         view back to its top. {context}"
    );
    assert_eq!(
        value("pushed-unblock"),
        "BlockContactDialog.qml:blocking=false,contactName=ada",
        "tapping a blocked contact did not ask about letting them back in. \
         {context}"
    );
    assert_eq!(
        value("blocked-after"),
        "0",
        "the contact was not let back in. {context}"
    );
    assert_eq!(
        value("error"),
        "",
        "something on the way said it had failed. {context}"
    );
    assert_eq!(
        names
            .iter()
            .filter(|name| **name == "block_contact")
            .count(),
        1,
        "the contact was blocked more than once, or not at all. {context}"
    );
    assert_eq!(
        names
            .iter()
            .filter(|name| **name == "unblock_contact")
            .count(),
        1,
        "the contact was let back in more than once, or not at all. {context}"
    );
}

/// How many blocks the core has been asked for so far.
fn blocked_calls(journal: &std::path::Path) -> usize {
    common::calls(journal)
        .iter()
        .filter(|(name, _)| name == "block_contact")
        .count()
}
