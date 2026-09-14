//! A profile the reader asked to delete goes, even if they left before
//! the page was told they were leaving.
//!
//! The wait before a profile goes lives beside the list (`PendingRemoval`),
//! and the profiles page empties it on the way out -- "leaving is exactly
//! when a timer has not fired yet". But `Deactivating` and "gone" are two
//! moments, and a back gesture that is already under way when the menu
//! item is tapped puts the tap in between them: the wait is armed on a
//! page that has had its warning already, and then the page is destroyed
//! with the timer still on it. Nothing was deleted, nothing said so, and
//! the profile was still there when the reader looked again.
//!
//! So the page empties it as it is destroyed as well. What that costs is
//! one flush over an empty set, which is nothing; what it buys is that no
//! deletion depends on a page status arriving first.

// Qt harness: see qml_profile_rows.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    non_snake_case,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // A qt_method is dispatched through the object, so a stack method
    // that records nothing still takes `&mut self`.
    clippy::unused_self,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see postivene-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Takes the page's navigation and does nothing with it: what is under
/// test here is what reaches the core.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    busy: qt_property!(bool; NOTIFY busy_changed),
    busy_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    replaceAbove:
        qt_method!(fn(&mut self, target: QVariant, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn push(&mut self, _page: QString, _properties: QVariantMap) {}
    fn replaceAbove(&mut self, _target: QVariant, _page: QString, _properties: QVariantMap) {}
    fn pop(&mut self) {}
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }

        function load(url) {
            loader.setSource('', {})
            // The wait is turned right up: what is under test is a page
            // that goes away before it is over, so it must not run out
            // on its own while the test is still setting up.
            loader.setSource(url, { currentAccountId: 1, pendingDelay: 60000 })
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
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
            }
            return null
        }
        function askDelete(accountId) {
            var row = findIn(loader.item, 'profileRow' + accountId)
            if (!row) { return 'missing:profileRow' + accountId }
            var item = findIn(row, 'deleteProfileItem')
            if (!item) { return 'missing:deleteProfileItem' }
            item.clicked()
            return 'ok'
        }
        // What Silica does to a page it has finished popping, without
        // the status change that comes first: the swipe was already
        // under way when the menu item was tapped, so this page has had
        // its Deactivating already.
        function destroyPage() {
            loader.setSource('', {})
            return 'ok'
        }
        function refresh() { core.refresh_accounts(); return 'ok' }
    }
";

#[test]
fn a_deletion_asked_for_on_the_way_out_still_goes() {
    let temp =
        std::env::temp_dir().join(format!("postivene-profile-delete-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and
    // before the server inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
        // Two profiles, so the one deleted is not also the last one.
        std::env::set_var("POSTIVENE_FAKE_ACCOUNTS", "1,2");
    }

    postivene_shim::register_qml_types();

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
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from(QString::from($arg))),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push(("refresh", call!("refresh")));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("load", call!("load", common::page_url("ProfilesPage.qml"))));
        (*steps_ptr).push(("ask", call!("askDelete", "2")));
        // No PageStatus.Deactivating first: the page is simply gone.
        (*steps_ptr).push(("gone", call!("destroyPage")));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
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
    let context = format!("steps: {steps:?}\ncalls: {calls:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the profiles page did not load. {context}"
    );
    assert_eq!(
        value("ask"),
        "ok",
        "nothing on the row asks for the profile to be deleted. {context}"
    );

    let removed: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "remove_account")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        removed,
        vec![2],
        "the profile the reader asked to delete was never deleted: the \
         page went before the wait was up, taking the timer with it. \
         {context}"
    );
}
