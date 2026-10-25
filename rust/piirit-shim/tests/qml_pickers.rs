//! Two picker pages, loaded alone.
//!
//! Pages, not Components in `ConversationPage`: a missing `Sailfish.Pickers` type costs
//! one button, not the conversation. Checked: each reports path on `picked`, nothing when
//! left without choice. Silica types existing is a device question.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property string reported: ''
        Loader { id: loader }
        // Load, then choose, then say what was heard -- the picker's
        // handler runs on the assignment, so this needs no waiting.
        function pick(url, path) {
            loader.setSource('', {})
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            reported = ''
            loader.item.picked.connect(function (chosen) { reported = chosen })
            // Nothing chosen yet: an empty path must not be reported as a
            // file, or cancelling the picker would arm an attachment.
            if (reported !== '') { return 'reported-before-choosing' }
            loader.item.selectedContentProperties = { filePath: path }
            return reported
        }
    }
";

/// Each picker page and its test file. Photo: profile and group pages. Library: attach
/// tray (documents, videos too). App: tray's webxdc entry (`.xdc`, not media index).
const PICKERS: [(&str, &str); 3] = [
    (
        "AttachPhotoPage.qml",
        "/home/user/Pictures/holiday photo.png",
    ),
    ("AttachLibraryPage.qml", "/home/user/Documents/report.pdf"),
    ("AttachAppPage.qml", "/home/user/Downloads/checkers.xdc"),
];

#[test]
fn each_picker_reports_the_file_that_was_chosen() {
    // No core: pages talk only to caller.
    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut heard: Vec<(&str, String)> = Vec::new();
    let heard_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(heard);

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        for (page, path) in PICKERS {
            let result = (*engine_ptr).invoke_method(
                "pick".into(),
                &[
                    QVariant::from(QString::from(common::page_url(page))),
                    QVariant::from(QString::from(path)),
                ],
            );
            (*heard_ptr).push((
                page,
                QString::from_qvariant(result)
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            ));
        }
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    assert_eq!(
        heard.len(),
        PICKERS.len(),
        "not every picker was driven: {heard:?}"
    );
    for ((page, path), (driven, reported)) in PICKERS.iter().zip(&heard) {
        assert_eq!(
            page, driven,
            "the pickers were driven out of order: {heard:?}"
        );
        assert_eq!(
            reported, path,
            "{page} did not report what was chosen. It answered {reported:?}, \
             where 'load-failed' means the page does not load at all and \
             'reported-before-choosing' means opening it arms an attachment \
             on its own. All: {heard:?}"
        );
    }
}
