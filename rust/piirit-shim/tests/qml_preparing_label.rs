//! The line that says a video is being made smaller fits a phone in every
//! language, with its number in sight.
//!
//! The number at its end is the only part that changes, so a line that
//! fades out before it says nothing. While the video is made smaller the
//! bar wraps its line instead of cutting it, and every catalog's wording
//! has to fit in two lines of the width the bar leaves it.
//!
//! The width is measured at the proportions of a Jolla phone at a pixel
//! ratio of one -- 540 wide, the label at `Theme.fontSizeExtraSmall` (24),
//! with the page margins, the stop button and the padding between taken
//! off, as `AttachmentBar.qml` does. Every phone since scales all of those
//! together. The text is set in `DejaVu Sans`, which is wider than Sailfish's
//! own face, so what fits here fits there.

// Qt harness: see qml_send_file.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use qmetaobject::*;

mod common;

/// What `AttachmentBar.qml` asks for, as lupdate filed it.
const SOURCE: &str = "Making video smaller for sending: %1%";

/// 540, less the margin on each side (24), the stop button (64) and the
/// padding before it (12).
const LABEL_WIDTH: u32 = 540 - 24 - 64 - 24 - 12;
const FONT_PIXELS: u32 = 24;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 400
        Text {
            id: measure
            width: __WIDTH__
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font.family: 'DejaVu Sans'
            font.pixelSize: __FONT__
        }
        function lines(text) {
            measure.text = text
            return String(measure.lineCount)
        }

        Loader { id: loader }
        function bar(url, preparing) {
            loader.setSource(url, {
                width: 160,
                filePath: '/home/user/Videos/a rather long name for a video.mp4',
                preparing: preparing,
                progress: 0.42
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            var label = find(loader.item, 'pendingAttachmentLabel')
            return label.lineCount + ' ' + label.truncated + ' '
                + (loader.item.height >= label.height) + ' ' + label.text
        }
        function find(node, name) {
            if (node.objectName === name) { return node }
            for (var i = 0; i < node.children.length; i++) {
                var hit = find(node.children[i], name)
                if (hit) { return hit }
            }
            return null
        }
    }
";

/// Each catalog's translation of `SOURCE`, by language.
fn translations() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../translations");
    let mut found = Vec::new();
    for entry in fs::read_dir(&dir).expect("read translations/").flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(language) = name
            .strip_prefix("piirit-")
            .and_then(|rest| rest.strip_suffix(".ts"))
        else {
            continue;
        };
        let text = fs::read_to_string(entry.path()).expect("read a catalog");
        let source = format!("<source>{SOURCE}</source>");
        let Some(at) = text.find(&source) else {
            continue;
        };
        let rest = &text[at..];
        let translated = rest
            .find("<translation>")
            .filter(|start| *start < rest.find("</message>").unwrap_or(0))
            .and_then(|start| {
                let body = &rest[start + "<translation>".len()..];
                body.find("</translation>")
                    .map(|end| body[..end].to_string())
            })
            // English has no translation of its own: the source is the
            // text.
            .unwrap_or_else(|| SOURCE.to_string());
        found.push((language.to_string(), translated.replace("&amp;", "&")));
    }
    found.sort();
    found
}

#[test]
fn the_preparing_line_fits_two_lines_in_every_language() {
    let wanted = translations();
    assert!(
        wanted.len() >= 40,
        "only {} catalogs carry the line; did lupdate stop running?",
        wanted.len()
    );

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(
        PROBE_QML
            .replace("__WIDTH__", &LABEL_WIDTH.to_string())
            .replace("__FONT__", &FONT_PIXELS.to_string())
            .as_str(),
    ));
    let bar_url = format!(
        "file://{}",
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/components/AttachmentBar.qml")
            .display()
    );

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut measured: Vec<(String, String)> = Vec::new();
    let measured_ptr: *mut Vec<(String, String)> = std::ptr::addr_of_mut!(measured);
    let mut bars: Vec<String> = Vec::new();
    let bars_ptr: *mut Vec<String> = std::ptr::addr_of_mut!(bars);
    let texts = wanted.clone();

    single_shot(Duration::from_millis(200), move || unsafe {
        for (language, text) in &texts {
            // The widest the number gets.
            let shown = text.replace("%1", "100");
            let lines = (*engine_ptr).invoke_method(
                "lines".into(),
                &[QVariant::from(QString::from(shown.as_str()))],
            );
            let lines = QString::from_qvariant(lines)
                .map(|value| value.to_string())
                .unwrap_or_default();
            (*measured_ptr).push((language.clone(), lines));
        }
        for preparing in [true, false] {
            let seen = (*engine_ptr).invoke_method(
                "bar".into(),
                &[
                    QVariant::from(QString::from(bar_url.as_str())),
                    QVariant::from(preparing),
                ],
            );
            (*bars_ptr).push(
                QString::from_qvariant(seen)
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            );
        }
        (*engine_ptr).quit();
    });
    engine.exec();

    let too_long: Vec<&(String, String)> = measured
        .iter()
        .filter(|(_, lines)| lines.parse::<u32>().map_or(true, |lines| lines > 2))
        .collect();
    assert!(
        too_long.is_empty(),
        "these languages take more than two lines of a phone's width, as \
         (language, lines): {too_long:?}. All: {measured:?}"
    );

    // The bar itself: wrapped and whole while the video is made smaller,
    // and as tall as what it shows ...
    let preparing = bars.first().cloned().unwrap_or_default();
    let mut fields = preparing.splitn(4, ' ');
    let lines: u32 = fields.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    assert!(
        lines >= 2 && fields.next() == Some("false") && fields.next() == Some("true"),
        "the bar cuts the line or does not wrap it, so the number is lost: {preparing}"
    );
    assert!(
        preparing.ends_with("42%"),
        "the bar does not end its line with the number: {preparing}"
    );
    // ... and one faded line for a picked file, whose name may be long.
    let picked = bars.get(1).cloned().unwrap_or_default();
    assert!(
        picked.starts_with("1 true true "),
        "a long file name is no longer kept to one line: {picked}"
    );
}
