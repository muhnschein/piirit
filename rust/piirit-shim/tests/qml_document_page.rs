//! File's first page, small (`DocumentThumbnail.qml`): PDF asked of platform thumbnailer,
//! office file's stored picture drawn by app's provider, type's icon where neither.
//!
//! Stub thumbnailer draws nothing: only what it is asked is proved. Stored page: real
//! provider, real zip.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use std::path::{Path, PathBuf};

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: probe
        property var shown: []
        // All at once: one wait for every stored page.
        function show(url, files, mimes) {
            var maker = Qt.createComponent(url)
            for (var i = 0; i < files.length; i++) {
                var item = maker.createObject(probe, { width: 96, height: 96,
                                                       fileUrl: files[i], fileMime: mimes[i] })
                if (!item) { return 'load-failed: ' + maker.errorString() }
                shown.push(item)
            }
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function clipped(at) { return '' + shown[at].clip }
        // Stub thumbnailer makes nothing: made thumbnail's size set by hand.
        function shaped(at, w, h) {
            var pdf = findIn(shown[at], 'documentPdf')
            pdf.implicitWidth = w
            pdf.implicitHeight = h
            return pdf.width + 'x' + pdf.height + '@' + pdf.x + ',' + pdf.y
        }
        function of(at, name, property) {
            var item = findIn(shown[at], name)
            return item ? '' + item[property] : '-'
        }
        // No stored page still loading (Image.Loading = 2).
        function settled() {
            for (var i = 0; i < shown.length; i++) {
                if (findIn(shown[i], 'documentPage').status === 2) { return false }
            }
            return shown.length > 0
        }
        function state(at) {
            return 'kind=' + shown[at].kind + ' drawn=' + shown[at].drawn
                   + ' icon=' + of(at, 'documentIcon', 'visible')
                   + ' page=' + of(at, 'documentPage', 'visible')
                   + ' pdf=' + of(at, 'documentPdf', 'mimeType')
        }
    }
";

/// Zip of one stored entry.
fn a_zip(path: &Path, name: &str, data: &[u8]) {
    let size = u32::try_from(data.len()).expect("small").to_le_bytes();
    let name_length = u16::try_from(name.len()).expect("short").to_le_bytes();
    let mut out = b"PK\x03\x04".to_vec();
    out.extend([0; 14]);
    out.extend(size);
    out.extend(size);
    out.extend(name_length);
    out.extend([0; 2]);
    out.extend(name.as_bytes());
    out.extend(data);
    let directory_at = u32::try_from(out.len()).expect("small").to_le_bytes();
    out.extend(b"PK\x01\x02");
    out.extend([0; 16]);
    out.extend(size);
    out.extend(size);
    out.extend(name_length);
    out.extend([0; 16]);
    out.extend(name.as_bytes());
    out.extend(b"PK\x05\x06");
    out.extend([0, 0, 0, 0, 1, 0, 1, 0]);
    out.extend(u32::try_from(46 + name.len()).expect("small").to_le_bytes());
    out.extend(directory_at);
    out.extend([0; 2]);
    std::fs::write(path, out).expect("write the office file");
}

fn url_of(path: &Path) -> String {
    format!("file://{}", path.display())
}

const ODT: &str = "application/vnd.oasis.opendocument.text";
const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

#[test]
#[allow(clippy::too_many_lines)]
fn a_document_shows_its_first_page_where_one_can_be_had_and_its_icon_where_not() {
    // Name with a space and `#`: URL QML hands over is encoded.
    let temp: PathBuf =
        std::env::temp_dir().join(format!("piirit-document-page-{} #1", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let with_page = temp.join("letter.odt");
    a_zip(
        &with_page,
        "Thumbnails/thumbnail.png",
        b"P6\n2 2\n255\n\x10\x20\x30\x10\x20\x30\x10\x20\x30\x10\x20\x30",
    );
    let without = temp.join("plain.docx");
    a_zip(&without, "word/document.xml", b"<w:document/>");
    let encoded = |path: &Path| url_of(path).replace(' ', "%20").replace('#', "%23");

    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    piirit_shim::install_pictures(&engine);
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
    let url = common::component_url("DocumentThumbnail.qml");
    let cases: Vec<(&'static str, String, &'static str)> = vec![
        (
            "pdf",
            "file:///tmp/piirit/a%20b.pdf".to_string(),
            "application/pdf",
        ),
        ("odt", encoded(&with_page), ODT),
        ("docx-bare", encoded(&without), DOCX),
        (
            "odt-gone",
            "file:///nonexistent/piirit/x.odt".to_string(),
            ODT,
        ),
        (
            "zip",
            "file:///tmp/piirit/a.zip".to_string(),
            "application/zip",
        ),
        ("unloaded", String::new(), "application/pdf"),
    ];

    let mut plan = common::Plan::default();
    let cases_ptr = std::ptr::addr_of!(cases);
    plan.after(1, move || unsafe {
        let mut files = QVariantList::default();
        let mut mimes = QVariantList::default();
        for (_, file, mime) in &*cases_ptr {
            files.push(QString::from(file.as_str()).into());
            mimes.push(QString::from(*mime).into());
        }
        (*steps_ptr).push((
            "load",
            call!("show", QString::from(url.as_str()), files, mimes),
        ));
    });
    // Provider answers off thread.
    plan.until(1, 30, move |_| unsafe {
        QVariant::to_bool(&(*engine_ptr).invoke_method("settled".into(), &[]))
    });
    plan.after(1, move || unsafe {
        for (at, (label, _, _)) in (0_i32..).zip(&*cases_ptr) {
            (*steps_ptr).push((label, call!("state", at)));
        }
        (*steps_ptr).push((
            "pdf-shape",
            format!(
                "{}x{} clip={} fit={} ask={} wide:{} tall:{}",
                call!(
                    "of",
                    0,
                    QString::from("documentPdf"),
                    QString::from("width")
                ),
                call!(
                    "of",
                    0,
                    QString::from("documentPdf"),
                    QString::from("height")
                ),
                call!("clipped", 0),
                call!(
                    "of",
                    0,
                    QString::from("documentPdf"),
                    QString::from("fillMode")
                ),
                call!(
                    "of",
                    0,
                    QString::from("documentPdf"),
                    QString::from("sourceSize")
                ),
                call!("shaped", 0, 160, 90),
                call!("shaped", 0, 100, 200)
            ),
        ));
        (*steps_ptr).push((
            "pdf-source",
            call!(
                "of",
                0,
                QString::from("documentPdf"),
                QString::from("source")
            ),
        ));
    });
    plan.after(1, move || unsafe { (*engine_ptr).quit() });
    plan.run();
    engine.exec();
    std::fs::remove_dir_all(&temp).expect("remove temp dir");

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");
    for (label, expected, complaint) in [
        ("load", "ok", "the thumbnails did not load"),
        (
            "pdf",
            "kind=pdf drawn=false icon=true page=false pdf=application/pdf",
            "a PDF is not asked of the thumbnailer over its icon",
        ),
        (
            "pdf-shape",
            "96x136 clip=true fit=1 ask=QSizeF(96, 192) wide:96x54@0,21 tall:96x192@0,0",
            "a PDF is not asked for whole, as wide as its tile, top shown or centred when short",
        ),
        (
            "pdf-source",
            "file:///tmp/piirit/a b.pdf",
            "the thumbnailer is not asked for the PDF's own file",
        ),
        (
            "odt",
            "kind=page drawn=true icon=false page=true pdf=-",
            "an office file's stored page is not drawn in place of its icon",
        ),
        (
            "docx-bare",
            "kind=page drawn=false icon=true page=false pdf=-",
            "an office file storing no page lost its icon",
        ),
        (
            "odt-gone",
            "kind=page drawn=false icon=true page=false pdf=-",
            "a missing file lost its icon",
        ),
        (
            "zip",
            "kind= drawn=false icon=true page=false pdf=-",
            "a file of no drawable kind was read for a page",
        ),
        (
            "unloaded",
            "kind= drawn=false icon=true page=false pdf=-",
            "a row without a file asked for a page",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
}
