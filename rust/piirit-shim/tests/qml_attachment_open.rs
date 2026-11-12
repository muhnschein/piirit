//! Open in another app (`AttachmentSaver.open`): other app gets copy in Piirit's Downloads
//! folder under sender's name, not hash-named file in chat's own folder its sandbox can't read.
//!
//! Real `FileSaver`, real files. Not proved here: that other app opens it (device).

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe
        property var saver: null
        property var handed: []
        property var failures: []
        function load(url, downloads) {
            StandardPaths.download = downloads
            var maker = Qt.createComponent(url)
            saver = maker.createObject(probe)
            if (!saver) { return 'load-failed: ' + maker.errorString() }
            // After component's own handler: reads what it handed on.
            saver.opened.connect(function (path) { probe.handed.push(saver.handedOn) })
            saver.error.connect(function (message) { probe.failures.push(message) })
            return 'ok'
        }
        function open(url, name) { saver.open(url, name); return 'ok' }
        function keep(url, name) { saver.keep(url, name); return 'ok' }
        function idle() { return !saver.busy }
        function handedOn() { return handed.join('|') }
        function failed() { return failures.join('|') }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_file_opened_in_another_app_goes_there_as_a_named_copy_made_once() {
    let temp = std::env::temp_dir().join(format!("piirit-attachment-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let blob = temp.join("5d41402abc4b2a76b9719d911017c592.pdf");
    std::fs::write(&blob, b"%PDF one").expect("write blob");
    let other = temp.join("0cc175b9c0f1b6a831c399e269772661.pdf");
    std::fs::write(&other, b"%PDF two").expect("write other blob");
    let url_of = |path: &std::path::Path| format!("file://{}", path.display());
    let downloads = temp.join("Downloads");

    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    piirit_shim::register_qml_types();
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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

    let component = common::component_url("AttachmentSaver.qml");
    let folder = downloads.to_string_lossy().into_owned();
    let (first, second) = (url_of(&blob), url_of(&other));
    let missing = url_of(&temp.join("missing.pdf"));
    let mut plan = common::Plan::default();
    plan.after(1, move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(component.as_str()),
                QString::from(folder.as_str())
            ),
        ));
        // Name with a space and `#`: URL handed on is encoded.
        let name = QString::from("minutes #1.pdf");
        // Queued; answered in order.
        call!("open", QString::from(first.as_str()), name.clone());
        call!("open", QString::from(first.as_str()), name.clone());
        call!("open", QString::from(second.as_str()), name.clone());
        call!("open", QString::from(missing.as_str()), name.clone());
        call!("keep", QString::from(first.as_str()), name);
    });
    plan.until(1, 30, move |_| unsafe {
        QVariant::to_bool(&(*engine_ptr).invoke_method("idle".into(), &[]))
    });
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("handed", call!("handedOn")));
        (*steps_ptr).push(("failed", call!("failed")));
        (*engine_ptr).quit();
    });
    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");
    let kept = downloads.join("Piirit");
    let mut names: Vec<String> = std::fs::read_dir(&kept)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let read = |name: &str| std::fs::read(kept.join(name)).unwrap_or_default();
    let (one, two) = (read("minutes #1.pdf"), read("minutes #1 (2).pdf"));
    let _ = std::fs::remove_dir_all(&temp);

    assert_eq!(value("load"), "ok", "the saver did not load. {context}");
    let base = format!("file://{}/", kept.display());
    assert_eq!(
        value("handed"),
        format!("{base}minutes%20%231.pdf|{base}minutes%20%231.pdf|{base}minutes%20%231%20(2).pdf"),
        "another app is not handed the named copy, the same one for the same file and \
         another for another file of that name. {context}"
    );
    assert_eq!(
        names,
        ["minutes #1 (2).pdf", "minutes #1 (3).pdf", "minutes #1.pdf"],
        "opening twice copied twice, or Save to device no longer makes a copy of its own. \
         {context}"
    );
    assert_eq!(one, b"%PDF one", "the copy is not the file. {context}");
    assert_eq!(
        two, b"%PDF two",
        "the second file's copy is not it. {context}"
    );
    assert!(
        value("failed").contains("cannot copy") && !value("failed").contains('|'),
        "a file that is gone is handed on, or says nothing. {context}"
    );
}
