//! Export of a profile to a backup file, from the page offering it.
//!
//! Core export takes account id: page reached from profile page. Pinned: export to `export_backup`
//! with this account and Documents; file there is the one page names. Write reports progress and
//! can't be left mid-write (leaving drops page listening for answer while core carries on).
//! Refused folder: core's words shown, button stays. Reader-stopped write reports nothing (core
//! refuses stopped export; reader's own doing, not news). Written backup ends page: button gone,
//! chats attached right, so way on is a swipe.
//!
//! Writes run refused, stopped, written: written one ends page, so button asks come first.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    // `pushAttached` is Silica's own name; probe answers to it.
    non_snake_case,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    // `qt_method!` decls must match dispatcher's by-value params; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded not performed: Cancel pops page, which must exist for test to
/// read it.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `pop:|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    /// Silica's name for a page to the right of this one.
    pushAttached: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

impl PageStackProbe {
    fn push(&mut self, page: QString, _properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        self.note(&format!("push:{name}"));
    }

    fn pushAttached(&mut self, page: QString, properties: QVariantMap) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let account = properties.value(QString::from("accountId"), QVariant::default());
        let account = i32::from_qvariant(account).unwrap_or_default();
        self.note(&format!("attach:{name}:{account}"));
    }

    fn pop(&mut self) {
        self.note("pop:");
    }

    fn note(&mut self, what: &str) {
        let current = self.log.to_string();
        self.log = format!("{current}{what}|").into();
        self.log_changed();
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // `currentAccountId` is the profile the chats are on, which is
        // not the one being backed up: the page attaches that chat list
        // to the right once it has written something.
        function load(url, accountId, currentAccountId) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: accountId,
                currentAccountId: currentAccountId
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Whose backup, off the row at the top: the profile drawn the
        // way the invite code's page draws it.
        function profileShows(property) {
            var row = findIn(loader.item, 'profileRow')
            if (!row) { return 'missing:profileRow' }
            if (property === 'ownColor') {
                var avatar = findIn(row, 'contactAvatar')
                return avatar ? '' + avatar.ownColor : 'missing:contactAvatar'
            }
            return '' + row[property]
        }
        // Where the platform puts documents, which is where the backup
        // goes. Writable on the stub, so the test can hand the page a
        // folder of its own -- and one the fake core refuses.
        function setDocuments(path) {
            StandardPaths.documents = path
            return StandardPaths.documents
        }
        // `data` rather than `children`: the Backup object is a plain
        // QObject and is not among an Item's visual children at all.
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
        function pageProperty(property) { return '' + loader.item[property] }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_backup_page_writes_one_profile_out_and_says_where() {
    let temp = std::env::temp_dir().join(format!("piirit-backup-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // Folder per write. Fake core keyed on it: `fail` refuses, `slow` waits to be stopped.
    let good = temp.join("Documents");
    let refused = temp.join("fail-Documents");
    let slow = temp.join("slow-Documents");

    // SAFETY: single-threaded; all set pre-Qt and before shim spawns server.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Slow write answers after reader gave up.
        std::env::set_var("PIIRIT_FAKE_SLOW_MS", "2500");
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

    let mut plan = common::Plan::default();

    // Refused, stopped, then written: written one ends page, so button asks come first.
    let refused_path = refused.to_string_lossy().into_owned();
    plan.after(1, move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("BackupPage.qml")),
                1,
                // Chats on another profile: backing this one up isn't switching to it.
                2
            )
        );
        record!(
            "leavable",
            call!("pageProperty", QString::from("backNavigation"))
        );
        record!(
            "documents-refused",
            call!("setDocuments", QString::from(refused_path.clone()))
        );
        record!(
            "write-refused",
            call!("click", QString::from("writeButton"))
        );
    });

    let slow_path = slow.to_string_lossy().into_owned();
    plan.after(2, move || unsafe {
        // Whose backup: profile drawn as invite page draws it. Address not asked: unconfigured
        // account has none.
        record!("colour", call!("profileShows", QString::from("ownColor")));
        record!("failure", get!("errorLabel", "text"));
        record!(
            "nothing-written",
            call!("pageProperty", QString::from("writtenPath"))
        );
        record!("button-after-failure", get!("writeButton", "visible"));
        // Write that takes its time, and reader giving up on it.
        record!(
            "documents-slow",
            call!("setDocuments", QString::from(slow_path.clone()))
        );
        record!("write-slow", call!("click", QString::from("writeButton")));
    });

    plan.after(1, move || unsafe {
        // Mid-write: bar up, button gone, no way off. Asked of slow write: only one with a
        // middle (core reports export as it goes; write done before first poll says nothing).
        record!("running", get!("backup", "running"));
        record!("bar", get!("writeProgress", "visible"));
        record!("button", get!("writeButton", "visible"));
        record!(
            "pinned",
            call!("pageProperty", QString::from("backNavigation"))
        );
        record!("reported", get!("backup", "permille"));
        record!("cancel", call!("click", QString::from("cancelButton")));
    });

    // Past slow write's answer, which arrives after cancel.
    let good_path = good.to_string_lossy().into_owned();
    plan.after(4, move || unsafe {
        record!("after-cancel", get!("errorLabel", "text"));
        record!("idle", get!("backup", "running"));
        record!(
            "still-nothing",
            call!("pageProperty", QString::from("writtenPath"))
        );
        record!(
            "documents",
            call!("setDocuments", QString::from(good_path.clone()))
        );
        record!("write", call!("click", QString::from("writeButton")));
    });

    plan.after(2, move || unsafe {
        record!(
            "written",
            call!("pageProperty", QString::from("writtenPath"))
        );
        record!("shown", get!("writtenLabel", "visible"));
        record!("said", get!("writtenLabel", "text"));
        record!(
            "leavable-after",
            call!("pageProperty", QString::from("backNavigation"))
        );
        // Nothing left: button gone, way on is a swipe to chats.
        record!("button-after", get!("writeButton", "visible"));
        record!("onward", get!("onwardHint", "visible"));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let popped = stack_box.pinned().borrow().log.to_string();
    let calls = common::calls(&journal);
    let context = format!("steps: {steps:?}\nstack: {popped}\ncalls: {calls:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    // Profile at top, drawn in account's colour: row bound to profile.
    assert_eq!(
        value("colour"),
        "#00875a",
        "the page does not say whose backup it is. {context}"
    );
    assert_eq!(
        value("leavable"),
        "true",
        "a page that has not started writing should be leavable. {context}"
    );
    assert_eq!(value("write"), "ok", "nothing started the write. {context}");
    assert_eq!(
        (
            value("running").as_str(),
            value("bar").as_str(),
            value("button").as_str(),
            value("pinned").as_str()
        ),
        ("true", "true", "false", "false"),
        "mid-write the page should be a bar and no way off it. {context}"
    );

    assert_written(&value("written"), &value("said"), &good, &context);
    assert_eq!(value("shown"), "true", "the path is not shown. {context}");
    assert_eq!(
        value("leavable-after"),
        "true",
        "the page is still pinned once the write is done. {context}"
    );
    // End of page: nothing to write; chats to right are current app's, not the profile just
    // backed up.
    assert_eq!(
        value("button-after"),
        "false",
        "a written backup still offers to write the same profile out \
         again. {context}"
    );
    assert_eq!(
        value("onward"),
        "true",
        "nothing says there is anywhere to swipe on to. {context}"
    );
    assert!(
        popped.contains("attach:ChatListPage.qml:2|"),
        "the chats were not attached to the right of the written \
         backup, so the swipe the page offers goes nowhere. {context}"
    );

    assert!(
        value("failure").contains("backup could not be written"),
        "a folder the core refused did not say so in the core's words. \
         {context}"
    );
    assert_eq!(
        value("nothing-written"),
        "",
        "a refused write left the earlier path on the page as if it had \
         just been written. {context}"
    );
    assert_eq!(
        value("button-after-failure"),
        "true",
        "a failure took away the button that tries again. {context}"
    );

    assert!(
        value("reported").parse::<u32>().unwrap_or(0) >= 300,
        "the write never reported itself, so the bar sat at nothing the \
         whole way. {context}"
    );
    assert_eq!(value("cancel"), "ok", "nothing cancelled. {context}");
    assert!(
        popped.contains("pop:"),
        "Cancel did not leave the page. {context}"
    );
    assert_eq!(
        value("after-cancel"),
        "",
        "the export the reader stopped was reported back to them as a \
         failure. {context}"
    );
    assert_eq!(
        value("still-nothing"),
        "",
        "a write the reader stopped left a path on the page as if it \
         had been written. {context}"
    );
    assert_eq!(
        value("idle"),
        "false",
        "the stopped write never ended. {context}"
    );

    assert_exports(&calls, &good, &refused, &slow, &context);
}

/// File page names: a `.tar` in the folder, really there. Core names backups itself; page finds
/// it by what appeared.
fn assert_written(path: &str, said: &str, folder: &std::path::Path, context: &str) {
    let folder = folder.to_string_lossy().into_owned();
    assert!(
        path.starts_with(&folder)
            && std::path::Path::new(path)
                .extension()
                .is_some_and(|kind| kind == "tar"),
        "the page did not name the backup it wrote: {path}. {context}"
    );
    assert!(
        std::path::Path::new(path).is_file(),
        "the page named {path}, which is not there. {context}"
    );
    assert!(
        said.contains(path),
        "what the page says does not carry the path: {said}. {context}"
    );
}

/// Every write went to `export_backup` on this account, in folder pointed at; the abandoned one
/// was stopped in core.
fn assert_exports(
    calls: &[(String, Value)],
    good: &std::path::Path,
    refused: &std::path::Path,
    slow: &std::path::Path,
    context: &str,
) {
    let exports: Vec<(u64, String)> = calls
        .iter()
        .filter(|(method, _)| method == "export_backup")
        .map(|(_, params)| {
            (
                params.get(0).and_then(Value::as_u64).unwrap_or(0),
                params
                    .get(1)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    assert_eq!(
        exports,
        vec![
            (1, refused.to_string_lossy().into_owned()),
            (1, slow.to_string_lossy().into_owned()),
            (1, good.to_string_lossy().into_owned()),
        ],
        "the exports, in order, each on the profile the page was given. \
         {context}"
    );
    let stopped: Vec<u64> = calls
        .iter()
        .filter(|(method, _)| method == "stop_ongoing_process")
        .filter_map(|(_, params)| params.get(0).and_then(Value::as_u64))
        .collect();
    assert_eq!(
        stopped,
        vec![1],
        "the write the reader gave up on was not stopped in the core. \
         {context}"
    );
}
