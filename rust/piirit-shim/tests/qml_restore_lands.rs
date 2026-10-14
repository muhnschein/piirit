//! Profile taken over from backup file lands, whenever core finishes reading it.
//!
//! Silica's page stack runs one transition at a time and refuses a move asked during
//! another (stderr only). File browser still animating away during import; small
//! backup read in before it finished. So hand-over to new profile's chats was dropped
//! mid-transition: reader saw progress bar, then "Restore from a backup" again, with
//! profile imported behind it (found by restarting app).
//!
//! So move is held until stack can make it (components/`PendingNavigation.qml`).
//! Pinned: stack busy whole time core works; profile still handed over after.
//!
//! Browser here for a second reason: reports choice by property change, possibly more
//! than once; each import is its own account. Two imports: duplicate profile
//! (`restore_duplicate.rs`), without reader asking twice.

// Qt harness: see qml_restore.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: probe

        // The page stack, written here rather than as a QObject on the
        // Rust side: a page loaded by a Loader reads `pageStack` off the
        // file the Loader was declared in, and a stack that behaves like
        // Silica's is three lines of QML. It behaves like Silica's in the
        // one way this test is about -- a move asked for while a
        // transition is running is not made, and is not reported either.
        property QtObject pageStack: QtObject {
            property bool busy: false
            property string log: ''
            function name(page) { return ('' + page).split('/').pop() }
            function push(page, properties) {
                if (busy) { return }
                log += 'push:' + name(page) + '|'
            }
            function replaceAbove(target, page, properties) {
                if (busy) { return }
                log += 'replaceAbove:' + name(page) + '|'
            }
            function pop() {
                if (busy) { return }
                log += 'pop|'
            }
        }

        Loader { id: loader }

        function loadWith(url, json) {
            loader.setSource('', {})
            loader.setSource(url, JSON.parse(json))
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function pageProperty(property) {
            return loader.item ? '' + loader.item[property] : 'no-page'
        }
        // What the file browser hands the page when a file is chosen.
        function begin(text) {
            if (!loader.item) { return 'no-page' }
            loader.item.begin(text)
            return 'ok'
        }
        // A page transition starting and ending.
        function transition(running) {
            probe.pageStack.busy = (running === 'true')
            return 'ok'
        }
        function stackLog() { return '' + probe.pageStack.log }
    }
";

// Engine, QObject boxes, steps: one scope, must outlive `exec()`.
#[test]
#[allow(clippy::too_many_lines)]
fn the_profile_that_arrived_is_handed_over_once_the_stack_is_free() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-lands-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // SAFETY: single-threaded; set before Qt and server start.
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
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    // SAFETY: callbacks fire only while `exec()` runs; `engine` outlives it.
    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = unsafe {
                (*engine_ptr).invoke_method(
                    $name.into(),
                    &[$(QVariant::from(QString::from($arg))),*],
                )
            };
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    let steps: common::Steps = Rc::new(RefCell::new(Vec::new()));

    // 1s: page, and file chosen on it, browser still animating away (stack busy). Chosen
    // twice, as a picker reporting one choice twice would.
    let s = steps.clone();
    single_shot(Duration::from_secs(1), move || {
        common::record(
            &s,
            "load",
            call!(
                "loadWith",
                common::page_url("RestoreProfilePage.qml"),
                r#"{"from":"file","status":2}"#
            ),
        );
        common::record(&s, "transition", call!("transition", "true"));
        common::record(&s, "first", call!("begin", "/tmp/holiday-backup.tar"));
        common::record(&s, "second", call!("begin", "/tmp/holiday-backup.tar"));
        common::record(&s, "busy", call!("pageProperty", "busy"));
    });

    // 3s: core answered. Transfer over; move not made yet; page holds back gesture over it.
    let s = steps.clone();
    single_shot(Duration::from_secs(3), move || {
        common::record(&s, "transferring", call!("pageProperty", "busy"));
        common::record(&s, "said", call!("pageProperty", "errorMessage"));
        common::record(&s, "held", call!("stackLog"));
        common::record(
            &s,
            "backNavigation",
            call!("pageProperty", "backNavigation"),
        );
        common::record(&s, "settled", call!("transition", "false"));
    });

    // 4s: stack free: held move made.
    let s = steps.clone();
    single_shot(Duration::from_secs(4), move || {
        common::record(&s, "landed", call!("stackLog"));
        common::record(&s, "backAgain", call!("pageProperty", "backNavigation"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow();
    let imports = common::calls(&journal)
        .into_iter()
        .filter(|(method, _)| method == "import_backup")
        .count();
    let context = format!("steps: {steps:?}\nimports: {imports}");

    assert_eq!(
        common::value_of(&steps, "load"),
        "ok",
        "the take-over page did not load. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "busy"),
        "true",
        "the import did not start on the file that was chosen. {context}"
    );
    assert_eq!(
        imports, 1,
        "the file browser reporting one choice twice started two imports, \
         which is two accounts holding two copies of one profile. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "transferring"),
        "false",
        "the page is still transferring after the core has answered. \
         {context}"
    );
    assert_eq!(
        common::value_of(&steps, "said"),
        "",
        "the import was reported as a failure. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "held"),
        "",
        "the hand-over was asked for while the stack was still \
         transitioning, which is where Silica drops it. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "backNavigation"),
        "false",
        "the reader can swipe away from a page still holding the move to \
         the profile it brought over, which would take both. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "landed"),
        "replaceAbove:ChatListPage.qml|",
        "the profile that arrived is not what the app landed on once the \
         stack was free. {context}"
    );
    assert_eq!(
        common::value_of(&steps, "backAgain"),
        "true",
        "the page never gave the back gesture up again. {context}"
    );
}
