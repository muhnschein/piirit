//! Writing a profile out to a backup file, from the page that offers it.
//!
//! The core's export is per profile -- it takes an account id -- so the
//! page is reached from the profile's own page and names the account it
//! was given. What is pinned here is the whole of what the page does
//! around that call:
//!
//! - the export goes to `export_backup` with this profile's account and
//!   the folder the platform calls Documents, and the file that appears
//!   there is the one the page names;
//! - the write reports itself while it runs, and the page cannot be left
//!   mid-write -- leaving would drop the page listening for the answer
//!   while the core carried on;
//! - a folder the core refuses says so in the core's own words, with the
//!   button still there to try again;
//! - a write the reader stops reports nothing: the core refuses the
//!   export it was told to stop, and that refusal is the reader's own
//!   doing rather than news;
//! - a backup that was written ends the page: the button goes, and the
//!   chats are attached to the right so the way on is a swipe rather
//!   than an offer to write the same profile out again.
//!
//! The three writes run in that order -- refused, stopped, written --
//! because the last one is the end of the page: after it there is no
//! button left to start another with, which is the point.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    // `pushAttached` is Silica's own name; the probe has to answer to it.
    non_snake_case,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see piiri-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded rather than performed: Cancel pops the
/// page, and the page has to still be there for the test to read what it
/// did afterwards.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `pop:|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    /// Silica's own name for putting a page to the right of this one.
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
    let temp = std::env::temp_dir().join(format!("piiri-backup-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // The folder the page is pointed at for each of the three writes.
    // The fake core is keyed on what it is handed, as it is for an
    // import: `fail` refuses, `slow` waits long enough to be stopped.
    let good = temp.join("Documents");
    let refused = temp.join("fail-Documents");
    let slow = temp.join("slow-Documents");

    // SAFETY: single-threaded test binary, and all of these have to be
    // set before Qt initialises and before the shim spawns the server
    // that inherits them.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
        // The slow write answers after the reader has given up on it.
        std::env::set_var("PIIRI_FAKE_SLOW_MS", "2500");
    }

    piiri_shim::register_qml_types();

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

    // Refused first, then stopped, then written: the written one is the
    // end of the page, so anything asked of a button has to be asked
    // before it.
    let refused_path = refused.to_string_lossy().into_owned();
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::page_url("BackupPage.qml")),
                1,
                // The chats are on another profile: backing this one up
                // is not switching to it.
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
    single_shot(Duration::from_secs(3), move || unsafe {
        // Whose backup: the profile itself, drawn the way the invite
        // code's page draws it, in the colour the core gives it. The
        // address is not asked for -- an account the fake core has not
        // configured has none, the same as on the profile page.
        record!("colour", call!("profileShows", QString::from("ownColor")));
        record!("failure", get!("errorLabel", "text"));
        record!(
            "nothing-written",
            call!("pageProperty", QString::from("writtenPath"))
        );
        record!("button-after-failure", get!("writeButton", "visible"));
        // A write that takes its time, and the reader giving up on it.
        record!(
            "documents-slow",
            call!("setDocuments", QString::from(slow_path.clone()))
        );
        record!("write-slow", call!("click", QString::from("writeButton")));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        // Mid-write: the bar is up, the button is gone, and there is no
        // way off the page. Asked of the slow write because it is the
        // only one with a middle -- the core reports an export as it
        // goes, and a write that is over before the first event has been
        // polled for says nothing.
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

    // Past the slow write's own answer, which arrives after the cancel.
    let good_path = good.to_string_lossy().into_owned();
    single_shot(Duration::from_secs(8), move || unsafe {
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

    single_shot(Duration::from_secs(10), move || unsafe {
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
        // Nothing left to do here: the button is gone and the way on is
        // a swipe to the chats.
        record!("button-after", get!("writeButton", "visible"));
        record!("onward", get!("onwardHint", "visible"));
        (*engine_ptr).quit();
    });

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
    // The profile itself at the top, not a line of address: drawn in the
    // colour the core gives this account, which is what says the row is
    // really bound to the profile.
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
    // The end of the page: nothing to write again with, and the chats
    // are to the right -- the ones the app is on, not the profile that
    // was just backed up.
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

/// The file the page names is a `.tar` in the folder it was pointed at,
/// and it is really there -- the core names a backup itself, so the page
/// finds it by what appeared in the folder.
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

/// Every write went to `export_backup` on this profile's account, with
/// the folder the page was pointed at, and the one the reader gave up on
/// was stopped in the core rather than left running.
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
