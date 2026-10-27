//! Emoji picture in a cut message drawn where it is in the text. Qt lays an elided last line out
//! apart from the rest and puts on it every picture whose place in the whole text falls inside
//! that line's length: a body or quote starting with an emoji drew it over its last shown line,
//! a gap left where it belonged. Pinned off the drawn pixels: the picture's colour sits on the
//! first line only, across body and quote both cut.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;
use std::path::PathBuf;

mod common;

/// Probe: delegate in a shown window, labels grabbed to files. Black ground: text white or grey,
/// so any coloured pixel is the picture's.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import QtQuick.Window 2.0
    import 'file://{components}'
    Item {{
        Window {{
            visible: true
            width: 540
            height: 900
            color: 'black'
            Loader {{ id: delegate }}
        }}
        function load(url, body, quote) {{
            delegate.setSource(url, {{ width: 540, messageText: body, quoteText: quote }})
            return delegate.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function findIn(node, name) {{
            if (!node) {{ return null }}
            if (node.objectName === name) {{ return node }}
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findIn(kids[i], name)
                if (hit) {{ return hit }}
            }}
            return null
        }}
        // Lines shown, whether cut, line height: where the first line ends.
        function shape(name) {{
            var label = findIn(delegate.item, name)
            return label.lineCount + '|' + label.truncated + '|'
                + Math.round(label.height / label.lineCount)
        }}
        property string saved: ''
        function savedShots() {{ return saved }}
        function shoot(name, path) {{
            var ok = findIn(delegate.item, name).grabToImage(function (result) {{
                if (result.saveToFile(path)) {{ saved += name + ';' }}
            }})
            return ok ? 'ok' : 'no-grab'
        }}
    }}
",
        components = components.display()
    )
}

/// Rows holding a clearly coloured pixel (the picture's; text is white or grey), and left-most
/// such x.
fn coloured_rows(file: &str) -> (Vec<u32>, u32) {
    let image = QImage::load_from_file(QString::from(file));
    let size = image.size();
    let mut rows = Vec::new();
    let mut left = u32::MAX;
    for y in 0..size.height {
        for x in 0..size.width {
            let (r, g, b, _) = image.get_pixel_color(x, y).get_rgba();
            if r.max(g).max(b) - r.min(g).min(b) > 60 {
                if rows.last() != Some(&y) {
                    rows.push(y);
                }
                left = left.min(x);
            }
        }
    }
    (rows, left)
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_cut_message_draws_its_first_emoji_on_its_first_line() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-emoji-cut-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        // Window drawn to be grabbed; software scene graph needs no GL.
        std::env::set_var("QT_QUICK_BACKEND", "software");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(probe_qml().as_str()));

    // As in the report: heading with the emoji, more lines than a bubble shows.
    let body: String = std::iter::once("🧠 Context Window\n".to_string())
        .chain((1..=20).map(|line| format!("\nLine {line} of the report")))
        .collect();
    let quote = "🔥 quoted, long enough to wrap past the two lines a quote shows in its \
                 bubble, and then some more words so it surely does";

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(String, String)> = Vec::new();
    let steps_ptr: *mut Vec<(String, String)> = std::ptr::addr_of_mut!(steps);
    let shot = |name: &str| temp.join(format!("{name}.png")).display().to_string();
    let (body_shot, quote_shot) = (shot("messageLabel"), shot("quoteLabel"));
    let (body_path, quote_path) = (body_shot.clone(), quote_shot.clone());

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
            (*steps_ptr).push(($label.to_string(), $value))
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("MessageDelegate.qml")),
                QString::from(body.as_str()),
                QString::from(quote)
            )
        );
    });
    plan.after(1, move || unsafe {
        for (name, path) in [("messageLabel", &body_path), ("quoteLabel", &quote_path)] {
            record!(format!("{name}-shape"), call!("shape", QString::from(name)));
            record!(
                format!("{name}-shot"),
                call!("shoot", QString::from(name), QString::from(path.as_str()))
            );
        }
    });
    plan.after(1, move || unsafe {
        record!("saved", call!("savedShots"));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");
    assert_eq!(value("load"), "ok", "the message did not load. {context}");
    assert_eq!(
        value("saved"),
        "messageLabel;quoteLabel;",
        "the labels were not drawn to files. {context}"
    );

    for (name, path, lines) in [
        ("messageLabel", &body_shot, 12),
        ("quoteLabel", &quote_shot, 2),
    ] {
        let shape = value(&format!("{name}-shape"));
        let mut parts = shape.split('|');
        let shown: u32 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        let cut = parts.next() == Some("true");
        let line: u32 = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        assert!(
            shown == lines && cut && line > 0,
            "{name} is not cut to {lines} lines ({shape}), so this proves nothing. {context}"
        );
        let (rows, left) = coloured_rows(path);
        assert!(
            !rows.is_empty(),
            "{name}'s emoji is not drawn as its picture at all. {context}"
        );
        let last = rows.last().copied().unwrap_or(0);
        assert!(
            last < line && left < line,
            "{name}'s picture is drawn at rows {}..={last} from x {left}, not at the start \
             of its first line (rows 0..{line}): a cut message puts it on its last line. \
             {context}",
            rows[0]
        );
    }
    let _ = std::fs::remove_dir_all(&temp);
}
