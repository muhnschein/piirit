//! Video-shrinking line fits a phone in every language, number in sight.
//!
//! Number at end is the only changing part; a faded line says nothing. While shrinking,
//! bar wraps, not cuts. Every catalog's wording must fit two lines.
//!
//! Width measured at Jolla proportions, pixel ratio 1: 540 wide, label 24
//! (`Theme.fontSizeExtraSmall`),
//! margins, stop button, padding removed (`AttachmentBar.qml`). Other phones scale together.
//! Text in `DejaVu Sans`, wider than Sailfish face: fits here, fits there.

// Qt harness: see qml_send_file.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;
use std::fs;
use std::path::PathBuf;

mod common;

/// What `AttachmentBar.qml` asks for, as lupdate filed it.
const SOURCE: &str = "Making video smaller for sending: %1%";

/// 540 less margin each side (24), stop button (64), padding (12).
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

/// Each catalog's `SOURCE` translation.
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
            // English: no own translation; source is the text.
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

    // SAFETY: single-threaded; set before Qt starts.
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
    let texts = wanted;

    let mut plan = common::Plan::default();

    plan.after(0, move || unsafe {
        for (language, text) in &texts {
            // Widest the number gets.
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
    plan.run();
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

    // Bar: wrapped and whole while shrinking, as tall as what it shows ...
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
    // ... picked file: one faded line; name may be long.
    let picked = bars.get(1).cloned().unwrap_or_default();
    assert!(
        picked.starts_with("1 true true "),
        "a long file name is no longer kept to one line: {picked}"
    );
}
