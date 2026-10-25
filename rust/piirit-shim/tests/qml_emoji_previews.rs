//! Emoji in one-line previews (chat list, chat picker, search hit, reply bar) drawn as Twemoji,
//! grey, shaded toward preview's text colour by app's provider (`image://piirit/emoji`): full
//! colour beside secondary text pulls eye off names. Pinned: preview with emoji is escaped
//! `StyledText` with tinted pictures, without stays plain; reply bar cut, not elided, with
//! pictures (elided last line misplaces them, `qml_emoji_cut.rs`). Pixels prove the grey: row's
//! picture, drawn through `StyledText` and provider, has no colour; same picture as message
//! draws it has.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;
use std::path::PathBuf;

mod common;

/// Probe: rows in a shown black window, labels grabbed to files; reference label draws same
/// emoji as a message does.
fn probe_qml() -> String {
    let qml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    format!(
        r"
    import QtQuick 2.0
    import QtQuick.Window 2.0
    import 'file://{qml}/js/Emoji.js' as Emoji
    Item {{
        Window {{
            visible: true
            width: 540
            height: 600
            color: 'black'
            Loader {{ id: row }}
            Loader {{ id: result; y: 150 }}
            Loader {{ id: reply; y: 300 }}
            Text {{
                id: message
                objectName: 'messageLabel'
                y: 450
                color: 'black'
                textFormat: Text.StyledText
            }}
        }}
        function load(which, url, json) {{
            var loader = which === 'row' ? row : which === 'result' ? result : reply
            var props = JSON.parse(json)
            props.width = 540
            loader.setSource(url, props)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed:' + which
        }}
        function set(which, property, value) {{
            var loader = which === 'row' ? row : which === 'result' ? result : reply
            loader.item[property] = value
            return 'ok'
        }}
        function asMessage(text) {{
            message.text = Emoji.inText(text, false, 10)
            return message.text
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
        function label(which, name) {{
            var loader = which === 'row' ? row : which === 'result' ? result : reply
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            return (item.textFormat === Text.StyledText ? 'styled:' : 'plain:') + item.text
        }}
        function cut(name) {{
            var item = findIn(reply.item, name)
            return item.truncationMode + '|' + item.elide
        }}
        property string saved: ''
        function savedShots() {{ return saved }}
        function shoot(name, path) {{
            var item = name === 'messageLabel' ? message : findIn(row.item, name)
            var ok = item.grabToImage(function (shot) {{
                if (shot.saveToFile(path)) {{ saved += name + ';' }}
            }})
            return ok ? 'ok' : 'no-grab'
        }}
    }}
",
        qml = qml.display()
    )
}

/// Stub `Theme.highlightColor`, as Qt writes a colour.
const HIGHLIGHT: &str = "#80c0ff";

/// Largest colourfulness (max channel minus min) among drawn pixels, and how many pixels are
/// lit at all.
fn colour_in(file: &str) -> (i32, usize) {
    let image = QImage::load_from_file(QString::from(file));
    let size = image.size();
    let mut most = 0;
    let mut lit = 0;
    for y in 0..size.height {
        for x in 0..size.width {
            let (r, g, b, _) = image.get_pixel_color(x, y).get_rgba();
            most = most.max(r.max(g).max(b) - r.min(g).min(b));
            if r.max(g).max(b) > 80 {
                lit += 1;
            }
        }
    }
    (most, lit)
}

#[test]
#[allow(clippy::too_many_lines)]
fn previews_draw_their_emoji_as_grey_twemoji() {
    let temp = std::env::temp_dir().join(format!("piirit-emoji-previews-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("a temporary directory");
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
    // As app: grey pictures come from here.
    piirit_shim::install_pictures(&engine);
    engine.load_data(QByteArray::from(probe_qml().as_str()));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(String, String)> = Vec::new();
    let steps_ptr: *mut Vec<(String, String)> = std::ptr::addr_of_mut!(steps);
    let shot = |name: &str| temp.join(format!("{name}.png")).display().to_string();
    let (row_shot, message_shot) = (shot("previewLabel"), shot("messageLabel"));
    let (row_path, message_path) = (row_shot.clone(), message_shot.clone());

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
    macro_rules! props {
        ($($key:expr => $value:expr),*) => {
            QString::from(serde_json::json!({ $($key: $value),* }).to_string().as_str())
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        let row_url = QString::from(common::component_url("ChatListDelegate.qml"));
        // Sender's markup stays text; sender name escaped too.
        record!(
            "row-load",
            call!(
                "load",
                QString::from("row"),
                row_url,
                props!("preview" => "<b>hi</b> 👍", "previewSender" => "A&B")
            )
        );
        record!(
            "row-emoji",
            call!("label", QString::from("row"), QString::from("previewLabel"))
        );
        call!(
            "set",
            QString::from("row"),
            QString::from("preview"),
            QString::from("see you there")
        );
        record!(
            "row-plain",
            call!("label", QString::from("row"), QString::from("previewLabel"))
        );

        record!(
            "result-load",
            call!(
                "load",
                QString::from("result"),
                QString::from(common::component_url("SearchResultRow.qml")),
                props!("title" => "Ada", "subtitle" => "party 🎉 tonight")
            )
        );
        record!(
            "result-emoji",
            call!(
                "label",
                QString::from("result"),
                QString::from("resultSubtitle")
            )
        );
        call!(
            "set",
            QString::from("result"),
            QString::from("subtitle"),
            QString::from("a <b> tonight")
        );
        record!(
            "result-plain",
            call!(
                "label",
                QString::from("result"),
                QString::from("resultSubtitle")
            )
        );
        // Searched: marks (Marks.js) and pictures both, sender's markup still text.
        call!(
            "set",
            QString::from("result"),
            QString::from("subtitleMatch"),
            QString::from("party")
        );
        call!(
            "set",
            QString::from("result"),
            QString::from("subtitle"),
            QString::from("Party 🎉 <party>")
        );
        record!(
            "result-marked-emoji",
            call!(
                "label",
                QString::from("result"),
                QString::from("resultSubtitle")
            )
        );
        call!(
            "set",
            QString::from("result"),
            QString::from("subtitle"),
            QString::from("a party")
        );
        record!(
            "result-marked",
            call!(
                "label",
                QString::from("result"),
                QString::from("resultSubtitle")
            )
        );

        record!(
            "reply-load",
            call!(
                "load",
                QString::from("reply"),
                QString::from(common::component_url("ReplyBar.qml")),
                props!("author" => "Ada", "body" => "on my way 🚲")
            )
        );
        record!(
            "reply-emoji",
            call!("label", QString::from("reply"), QString::from("replyLabel"))
        );
        record!("reply-emoji-cut", call!("cut", QString::from("replyLabel")));
        call!(
            "set",
            QString::from("reply"),
            QString::from("body"),
            QString::from("on my way")
        );
        record!(
            "reply-plain",
            call!("label", QString::from("reply"), QString::from("replyLabel"))
        );
        record!("reply-plain-cut", call!("cut", QString::from("replyLabel")));

        // Same picture twice: row's, as message draws it.
        call!(
            "set",
            QString::from("row"),
            QString::from("previewSender"),
            QString::from("")
        );
        call!(
            "set",
            QString::from("row"),
            QString::from("preview"),
            QString::from("🔥🔥🔥")
        );
        record!("message", call!("asMessage", QString::from("🔥🔥🔥")));
    });
    plan.after(1, move || unsafe {
        for (name, path) in [("previewLabel", &row_path), ("messageLabel", &message_path)] {
            record!(
                format!("{name}-shot"),
                call!("shoot", QString::from(name), QString::from(path.as_str()))
            );
        }
    });
    // Each second: both labels grabbed, or patience spent? Then quit.
    plan.until(1, 6, move |last| unsafe {
        let saved = QString::from_qvariant((*engine_ptr).invoke_method("savedShots".into(), &[]))
            .map(|value| value.to_string())
            .unwrap_or_default();
        if saved.split(';').filter(|name| !name.is_empty()).count() < 2 && !last {
            return false;
        }
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");
    for which in ["row", "result", "reply"] {
        assert_eq!(
            value(&format!("{which}-load")),
            "ok",
            "the {which} did not load. {context}"
        );
    }

    let grey = |picture: &str| {
        let file = common::art_dir()
            .join(format!("emoji/{picture}.svg"))
            .canonicalize()
            .unwrap_or_else(|err| panic!("{picture}.svg is not shipped ({err})"));
        format!(
            "<img src=\"image://piirit/emoji?file={}&tint=%23a0a0a0&strength=0.5\" \
             width=\"12\" height=\"12\" align=\"middle\">",
            encode(&format!("file://{}", file.display()))
        )
    };
    assert_eq!(
        value("row-emoji"),
        format!("styled:A&amp;B: &lt;b&gt;hi&lt;/b&gt; {}", grey("1f44d")),
        "a chat-list preview's emoji is not a grey picture, or its text became markup. \
         {context}"
    );
    assert_eq!(
        value("row-plain"),
        "plain:A&B: see you there",
        "a preview without emoji is no longer plain text. {context}"
    );
    assert_eq!(
        value("result-emoji"),
        format!("styled:party {} tonight", grey("1f389")),
        "a search hit's emoji is not a grey picture. {context}"
    );
    assert_eq!(
        value("result-plain"),
        "plain:a <b> tonight",
        "a search hit without emoji is no longer plain text. {context}"
    );
    let mark = |text: &str| format!("<b><font color=\"{HIGHLIGHT}\">{text}</font></b>");
    assert_eq!(
        value("result-marked-emoji"),
        format!(
            "styled:{} {} &lt;{}&gt;",
            mark("Party"),
            grey("1f389"),
            mark("party")
        ),
        "a searched hit with emoji loses its marks or its pictures. {context}"
    );
    assert_eq!(
        value("result-marked"),
        format!("styled:a {}", mark("party")),
        "a searched hit without emoji loses its marks. {context}"
    );
    assert_eq!(
        value("reply-emoji"),
        format!("styled:Replying to Ada: on my way {}", grey("1f6b2")),
        "the reply bar's emoji is not a grey picture. {context}"
    );
    assert_eq!(
        value("reply-plain"),
        "plain:Replying to Ada: on my way",
        "the reply bar without emoji is no longer plain text. {context}"
    );
    // Stub: TruncationMode.None = 0, Elide = 2; Text.ElideNone = 3, ElideRight = 1.
    assert_eq!(
        value("reply-emoji-cut"),
        "0|3",
        "the reply bar elides with pictures in it: they land on the wrong line. {context}"
    );
    assert_eq!(
        value("reply-plain-cut"),
        "2|1",
        "the reply bar without pictures lost its ellipsis. {context}"
    );

    assert_eq!(
        value("previewLabel-shot"),
        "ok",
        "the row's preview could not be grabbed. {context}"
    );
    let (row_colour, row_lit) = colour_in(&row_shot);
    let (message_colour, message_lit) = colour_in(&message_shot);
    let measured = format!(
        "row colour {row_colour} over {row_lit} lit px, message colour {message_colour} over \
         {message_lit} lit px. {context}"
    );
    assert!(
        message_colour > 120 && message_lit > 20,
        "the message's fire is not drawn in full colour, so this proves nothing: {measured}"
    );
    assert!(
        row_lit * 2 > message_lit,
        "the row's emoji are not drawn: provider not reached from StyledText. {measured}"
    );
    // Grey: channels within antialiasing of each other.
    assert!(
        row_colour <= 8,
        "the row's emoji keep their colour, not grey as their text. {measured}"
    );
    let _ = std::fs::remove_dir_all(&temp);
}

/// `encodeURIComponent`, for paths: ASCII letters, digits, `-_.!~*'()` kept.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
