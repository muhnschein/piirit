//! The two picker pages, loaded on their own.
//!
//! They are pages rather than Components inside `ConversationPage`, precisely
//! so that a `Sailfish.Pickers` type that is not there costs one button
//! instead of the whole conversation -- which also means nothing else loads
//! them, and without this nothing would notice them breaking.
//!
//! What can be checked here is our half: that each page reports the chosen
//! path on `picked`, and reports nothing when the picker was left without a
//! choice. That the Silica types themselves exist is a device question, and
//! `tests/silica-stubs` cannot answer it.

// Qt harness: see qml_chat_list.rs.
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

/// Each picker page, and the file a test hands it. The photo picker is
/// the profile and group pages' (a picture, and nothing else); the
/// library picker is the attach tray's paper clip, and takes a document
/// or a video as readily as a picture; the app picker is the tray's
/// webxdc entry, which browses for a .xdc rather than listing what the
/// media index knows.
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
    // No core here: these pages talk to nothing but their caller, so there
    // is no server to spawn and no accounts directory to point it at.
    // SAFETY: single-threaded test binary; set before Qt starts.
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

    single_shot(Duration::from_secs(1), move || unsafe {
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
