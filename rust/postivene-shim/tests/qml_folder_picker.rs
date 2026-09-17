//! Choosing a folder by walking to it.
//!
//! The dialog opens on the folder the setting names, or as near to it as
//! exists; lists what is under a folder, by name and without hidden ones;
//! goes down on a tap and up on the row above the list; offers the
//! platform's folders at the top and can take none of that level; and
//! hands over the folder being looked at when accepted.

// Qt harness: see qml_share.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, folder) {
            loader.setSource('', {})
            loader.setSource(url, { folder: folder, width: 540, height: 960 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function setRoots(documents, downloads) {
            StandardPaths.documents = documents
            StandardPaths.download = downloads
            return 'ok'
        }
        function current() { return '' + loader.item.current }
        function inFolder() { return '' + loader.item.inFolder }
        function canAccept() { return '' + loader.item.canAccept }
        function chosen() { return '' + loader.item.chosen }
        function enter(path) { loader.item.enter(path); return 'ok' }
        function up() { loader.item.up(); return 'ok' }
        function accept() { loader.item.accept(); return 'ok' }
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
        // Every folder row's name, in list order.
        function names() {
            var out = []
            gather(loader.item, 'folderName', out)
            return out.join('|')
        }
        function gather(node, name, out) {
            if (!node) { return }
            if (node.objectName === name) { out.push(node.text) }
            var kids = node.data !== undefined ? node.data : node.children
            // A view's contentItem is among its children already.
            for (var i = 0; kids && i < kids.length; i++) { gather(kids[i], name, out) }
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
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

#[test]
#[allow(clippy::too_many_lines)]
fn a_folder_is_chosen_by_walking_to_it_and_taken_only_on_accept() {
    let temp = std::env::temp_dir().join(format!("postivene-folder-picker-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    let documents = temp.join("Documents");
    let downloads = temp.join("Downloads");
    for folder in ["zeta", "Alpha", ".hidden"] {
        std::fs::create_dir_all(documents.join(folder)).expect("make a folder");
    }
    std::fs::create_dir_all(&downloads).expect("make Downloads");
    std::fs::write(documents.join("a file.txt"), b"x").expect("write a file");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    postivene_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let dialog = common::page_url("FolderPickerDialog.qml");
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

    let documents_str = documents.to_string_lossy().into_owned();
    let downloads_str = downloads.to_string_lossy().into_owned();
    let postivene = documents.join("Postivene");
    let postivene_str = postivene.to_string_lossy().into_owned();
    let alpha = documents.join("Alpha").to_string_lossy().into_owned();
    let alpha_for_probe = alpha.clone();
    let postivene_for_probe = postivene_str.clone();

    single_shot(Duration::from_secs(1), move || unsafe {
        call!(
            "setRoots",
            QString::from(documents_str.clone()),
            QString::from(downloads_str.clone())
        );
        // The default folder is not there yet, so the dialog opens as
        // near to it as exists: Documents.
        record!(
            "load",
            call!(
                "load",
                QString::from(dialog.clone()),
                QString::from(postivene_for_probe.clone())
            )
        );
        record!("opens-near", call!("current"));
        record!("lists", call!("names"));
        record!(
            "up-offered",
            call!("get", QString::from("upRow"), QString::from("visible"))
        );
        record!("acceptable", call!("canAccept"));

        // Down into a folder with nothing under it, and back up twice:
        // to Documents, then to the platform's folders, which cannot be
        // taken as a folder.
        call!("enter", QString::from(alpha_for_probe.clone()));
        record!("in-alpha", call!("current"));
        record!("alpha-lists", call!("names"));
        record!("alpha-acceptable", call!("canAccept"));
        call!("click", QString::from("upRow"));
        record!("up-once", call!("current"));
        call!("up");
        record!("up-twice", call!("inFolder"));
        record!("roots", call!("names"));
        record!("roots-acceptable", call!("canAccept"));
        record!(
            "roots-up-offered",
            call!("get", QString::from("upRow"), QString::from("visible"))
        );
        call!("accept");
        record!("nothing-taken", call!("chosen"));

        // Made since, the default opens as itself, and a swipe forward
        // takes it.
        std::fs::create_dir_all(&postivene).expect("make the default folder");
        record!(
            "reload",
            call!(
                "load",
                QString::from(dialog.clone()),
                QString::from(postivene_for_probe.clone())
            )
        );
        record!("opens-on", call!("current"));
        call!("accept");
        record!("taken", call!("chosen"));

        // A folder under none of the platform's opens on the platform's.
        record!(
            "load-elsewhere",
            call!(
                "load",
                QString::from(dialog.clone()),
                QString::from("/nowhere/at/all")
            )
        );
        record!("elsewhere", call!("inFolder"));

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
    let context = format!("steps: {steps:?}");
    let documents_str = documents.to_string_lossy().into_owned();

    assert_eq!(value("load"), "ok", "the dialog did not load. {context}");
    for (label, expected, complaint) in [
        (
            "opens-near",
            documents_str.as_str(),
            "a folder that is not there yet did not open on its nearest parent",
        ),
        (
            "lists",
            "Alpha|zeta",
            "the folders under Documents are not listed by name, or a hidden one or a file is",
        ),
        ("up-offered", "true", "there is no way up from a folder"),
        ("acceptable", "true", "a folder cannot be taken"),
        (
            "in-alpha",
            alpha.as_str(),
            "a tap did not go into the folder",
        ),
        ("alpha-lists", "", "an empty folder lists something"),
        (
            "alpha-acceptable",
            "true",
            "an empty folder cannot be taken, though a copy would make what it needs",
        ),
        (
            "up-once",
            documents_str.as_str(),
            "the row above the list did not go up",
        ),
        (
            "up-twice",
            "false",
            "up from a root did not reach the platform's folders",
        ),
        (
            "roots",
            "Documents|Downloads|Music|Videos|Pictures",
            "the platform's folders are not the ones the sandbox grants, in that order",
        ),
        (
            "roots-acceptable",
            "false",
            "the list of platform folders can be taken as a folder",
        ),
        ("roots-up-offered", "false", "there is an up from the top"),
        ("nothing-taken", "", "accepting at the top took something"),
        ("reload", "ok", "the dialog did not load again"),
        (
            "opens-on",
            postivene_str.as_str(),
            "a folder that is there did not open as itself",
        ),
        (
            "taken",
            postivene_str.as_str(),
            "accepting did not hand over the folder being looked at",
        ),
        (
            "load-elsewhere",
            "ok",
            "the dialog did not load on a folder outside the roots",
        ),
        (
            "elsewhere",
            "false",
            "a folder outside the platform's did not open on the platform's",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    let _ = std::fs::remove_dir_all(&temp);
}
