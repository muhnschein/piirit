//! Cover's lists, one per profile, come from a Repeater whose count runs ahead of its built rows:
//! a row is null until built (on phone at start, async). Grid rebuilt then threw on the null row
//! and stopped, so whoever came after was missing until something else rebuilt it. Here profiles
//! arrive after cover is up, so rows build one by one while it watches.

// Qt harness: see qml_cover.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::log::{install_message_handler, QMessageLogContext, QtMsgType};
use qmetaobject::*;
use std::sync::Mutex;

mod common;

/// Script errors QML only logs: one in a signal handler reaches no caller.
static SCRIPT_ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

extern "C" fn catch_script_errors(_: QtMsgType, _: &QMessageLogContext, message: &QString) {
    let text = message.to_string();
    eprintln!("{text}");
    if text.contains("Error:") {
        SCRIPT_ERRORS.lock().expect("lock").push(text);
    }
}

/// Loads cover before any profile is known; on each row built, hands cover a core event and
/// rebuilds grid at once (as either landing then would), noting whether rebuild threw and whether
/// cover rebuilt by itself.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: probe
        Loader { id: loader }
        property var repeater: null
        property int rebuilds: 0
        property int rebuildsSeen: 0
        property var notes: []
        function load(url) {
            loader.setSource(url, { width: 240, height: 360 })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            probe.rebuildsSeen = probe.rebuilds
            var kids = loader.item.data
            for (var i = 0; i < kids.length; i++) {
                if (kids[i].itemAt !== undefined && !probe.repeater) { probe.repeater = kids[i] }
            }
            return probe.repeater ? 'ok' : 'no-repeater'
        }
        Connections {
            target: loader.item
            onCellsChanged: probe.rebuilds += 1
        }
        Connections {
            target: probe.repeater
            onItemAdded: {
                // Cover's own handler ran first, connected first.
                var gathered = probe.rebuilds > probe.rebuildsSeen ? 'gathered' : 'not-gathered'
                core.core_event(1, 'ChatlistChanged', '{}')
                var note = 'ok'
                try {
                    loader.item.rebuild()
                } catch (err) {
                    note = 'threw:' + err
                }
                probe.rebuildsSeen = probe.rebuilds
                probe.notes = probe.notes.concat(
                    [index + '/' + probe.repeater.count + ':' + gathered + ':' + note])
            }
        }
        function seen() { return probe.notes.join(';') }
        function people() { return '' + loader.item.people.length }
        function refresh() { core.refresh_accounts(); return 'ok' }
    }
";

#[test]
fn the_cover_rebuilds_while_profile_lists_are_still_being_built() {
    let temp = std::env::temp_dir().join(format!("piirit-cover-late-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded; set pre-Qt and before server inherits.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    install_message_handler(Some(catch_script_errors));
    piirit_shim::register_qml_types();
    common::register_cover_enum();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let cover = format!(
        "file://{}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    );

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

    // Before core starts: no profile yet, so rows come later.
    // SAFETY: engine lives till end of test; nothing else holds it now.
    let load = unsafe { call!("load", QString::from(cover)) };

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let steps: common::Steps = common::Steps::default();
    let mut plan = common::Plan::default();
    let core_ptr = std::ptr::addr_of!(core_box);
    plan.until(1, 30, move |_| unsafe {
        if !common::ready(&*core_ptr) {
            return false;
        }
        call!("refresh");
        true
    });
    let built = steps.clone();
    // Both rows built, lists filled: four people.
    plan.until(1, 10, move |last| unsafe {
        let people = call!("people");
        if people != "4" && !last {
            return false;
        }
        common::record(&built, "people", people.into());
        common::record(&built, "seen", call!("seen").into());
        (*engine_ptr).quit();
        true
    });
    plan.run();
    engine.exec();

    let steps = steps.borrow().clone();
    assert_eq!(load, "ok", "the cover did not load");
    let errors = SCRIPT_ERRORS.lock().expect("lock").clone();
    assert!(
        errors.is_empty(),
        "the cover threw while its rows were still being built: {errors:?}"
    );
    assert_eq!(
        common::value_of(&steps, "seen"),
        "0/2:gathered:ok;1/2:gathered:ok",
        "rows were not built one by one with the cover up; or a row built late did not make the \
         cover rebuild, so a profile whose list filled first stays missing; or a rebuild with a row \
         not yet built threw and left the grid half done"
    );
    assert_eq!(
        common::value_of(&steps, "people"),
        "4",
        "the cover does not show everyone across both profiles"
    );
}
