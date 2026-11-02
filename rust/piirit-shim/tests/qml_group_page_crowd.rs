//! Group of hundreds: page builds member rows near screen only, rest as scrolled to. Built all
//! at page-in (751 members: page slid in and out at 35 fps on phone).

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const CROWD: u32 = 700;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader; width: 540; height: 960 }
        function load(url) {
            loader.setSource(url, { accountId: 1, chatId: 2, chatName: 'crowd' })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // Every item once: list views reach rows through `contentItem` too.
        function walk(test) {
            var seen = [], found = []
            function visit(node) {
                if (!node || seen.indexOf(node) >= 0) { return }
                seen.push(node)
                if (test(node)) { found.push(node) }
                var kids = node.children
                for (var i = 0; kids && i < kids.length; i++) { visit(kids[i]) }
                if (node.contentItem) { visit(node.contentItem) }
            }
            visit(loader.item)
            return found
        }
        function rowsBuilt() {
            return '' + walk(function(node) {
                return ('' + node.objectName).indexOf('memberRow') === 0
            }).length
        }
        function heading() {
            var found = walk(function(node) { return node.objectName === 'membersHeader' })
            return found.length > 0 ? '' + found[0].text : 'missing'
        }
        // Reader flicks to the foot of the list.
        function toEnd() {
            var lists = walk(function(node) { return node.objectName === 'memberList' })
            if (lists.length === 0) { return 'missing' }
            lists[0].positionViewAtEnd()
            return 'ok'
        }
        function has(name) {
            return walk(function(node) { return node.objectName === name }).length > 0
                ? 'yes' : 'no'
        }
    }
";

#[test]
fn a_group_of_hundreds_builds_only_the_member_rows_on_screen() {
    let temp = std::env::temp_dir().join(format!("piirit-group-crowd-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_CROWD", CROWD.to_string());
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

    let core_ptr = std::ptr::addr_of!(core_box);
    let mut plan = common::Plan::default();
    // SAFETY for every block: fire only inside `exec()`; `engine`, `core_box`, `steps` outlive it.
    plan.until(1, 30, move |_| unsafe { common::ready(&*core_ptr) });
    plan.after(0, move || unsafe {
        let loaded = call!("load", QString::from(common::page_url("GroupPage.qml")));
        (*steps_ptr).push(("load", loaded));
    });
    // Members in: heading counts all.
    let expected = format!("{}", CROWD + 2);
    plan.until(1, 30, move |last| unsafe {
        let heading = call!("heading");
        if !heading.starts_with(&expected) && !last {
            return false;
        }
        (*steps_ptr).push(("heading", heading));
        (*steps_ptr).push(("built", call!("rowsBuilt")));
        (*steps_ptr).push(("to-end", call!("toEnd")));
        true
    });
    let last_row = format!("memberRow{}", 1000 + CROWD - 1);
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("last-row", call!("has", QString::from(last_row.as_str()))));
        (*steps_ptr).push(("built-at-end", call!("rowsBuilt")));
        (*engine_ptr).quit();
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
    assert_eq!(value("load"), "ok", "{context}");
    assert!(
        value("heading").starts_with(&(CROWD + 2).to_string()),
        "the heading does not count every member. {context}"
    );
    let built: usize = value("built").parse().unwrap_or(usize::MAX);
    assert!(
        (1..60).contains(&built),
        "{built} member rows built for a page that shows a screenful. {context}"
    );
    assert_eq!(
        value("last-row"),
        "yes",
        "scrolling to the end did not show the last member. {context}"
    );
    let at_end: usize = value("built-at-end").parse().unwrap_or(usize::MAX);
    assert!(
        at_end < 60,
        "{at_end} rows held after scrolling to the end. {context}"
    );
}
