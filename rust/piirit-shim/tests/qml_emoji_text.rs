//! Emoji in a message drawn as the same pictures the reactions are.
//!
//! Sailfish draws few emoji itself, so a message body or a quote with one
//! in it becomes `StyledText` with the emoji as `<img>` of a shipped picture
//! (Emoji.js's `inText`). What this pins is what counts as an emoji --
//! not a digit, not a "©" sent as text -- and that nothing the sender
//! wrote can become markup on the way.
//!
//! And what it costs. Drawing reads tables built ahead of time from the
//! picker's list (EmojiPictures.js), not the list with every emoji's
//! search words in it, which is the picker's alone; so the tables have to
//! agree with the list for every emoji in it. And a text with no
//! character of any emoji in it is passed over at a glance rather than
//! read through.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use qmetaobject::*;

mod common;

/// The probe imports the app's script and components by absolute URL: it
/// is loaded from data, which has no directory to resolve one against.
fn probe_qml() -> String {
    let qml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    format!(
        r"
    import QtQuick 2.0
    import 'file://{qml}/components'
    import 'file://{qml}/js/Emoji.js' as Emoji
    import 'file://{qml}/js/EmojiData.js' as Data
    import 'file://{qml}/js/EmojiPictures.js' as Pictures
    Item {{
        width: 540
        height: 400

        function inText(text, styled) {{ return Emoji.inText(text, styled, 18) }}
        function artBase() {{ return '' + Emoji.artBase }}

        // Every emoji the picker offers, drawn in a message on its own and
        // between words, and looked up as a reaction: the tables drawing
        // reads are built from the picker's list ahead of time, and have
        // to say what it says. The ones that do not, and how many there
        // were.
        function everyEmoji() {{
            var wrong = []
            for (var i = 0; i < Data.list.length; i++) {{
                var emoji = Data.list[i][0]
                var picture = Data.list[i][1]
                var img = '/' + picture + '.png'
                if (Emoji.pictureFor(emoji) !== picture
                        || Emoji.inText(emoji, false, 18).indexOf(img) < 0
                        || Emoji.inText('a ' + emoji + ' b', true, 18).indexOf(img) < 0) {{
                    wrong.push(emoji)
                }}
            }}
            return Data.list.length + ':' + wrong.join(' ')
        }}
        // Whether a text gets past the quick look `inText` takes before it
        // reads a text through.
        function mayHold(text) {{ return '' + Pictures.mayHold.test(text) }}

        Loader {{ id: delegate }}
        function loadDelegate(url) {{
            delegate.setSource(url, {{
                width: 540, messageText: 'hi 👍 there', quoteText: 'was 🔥'
            }})
            return delegate.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        Loader {{ id: glyph }}
        function loadGlyph(url) {{
            glyph.setSource(url, {{ emoji: '👍', size: 40 }})
            return glyph.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function glyphText() {{
            return glyph.item.drawn ? 'picture' : 'text:' + glyph.item.emoji
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
        // A label as drawn: StyledText or not, and its text.
        function label(name) {{
            var item = findIn(delegate.item, name)
            if (!item) {{ return 'missing:' + name }}
            return (item.textFormat === Text.StyledText ? 'styled:' : 'plain:') + item.text
        }}
    }}
",
        qml = qml.display()
    )
}

/// The <img> `inText` writes for a picture under `base`, for a font of
/// 18 pixels, after checking the picture is there to be drawn.
fn img(base: &str, picture: &str) -> String {
    let file = format!("{}{picture}.png", base.trim_start_matches("file://"));
    assert!(
        PathBuf::from(&file).is_file(),
        "{file} is not shipped where the text looks for it"
    );
    format!("<img src=\"{base}{picture}.png\" width=\"21\" height=\"21\" align=\"middle\">")
}

#[test]
#[allow(clippy::too_many_lines)]
fn messages_draw_their_emoji_as_the_reactions_do() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(probe_qml().as_str()));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(String, String)> = Vec::new();
    let steps_ptr: *mut Vec<(String, String)> = std::ptr::addr_of_mut!(steps);

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

    let texts: Vec<(&'static str, &'static str, bool)> = vec![
        ("thumb", "hi 👍", false),
        ("none", "no emoji at all, 1 # * 2", false),
        ("escaped", "a <b> & 👍\n  two  spaces", false),
        ("copyright-text", "(c) © 2024", false),
        ("copyright-emoji", "©\u{fe0f}", false),
        ("heart-text", "\u{2764}", false),
        ("smiley-text", "\u{263a} hi", false),
        ("heart-emoji", "\u{2764}\u{fe0f}", false),
        // An emoji whatever follows it, as every client sends it: bare.
        ("star", "\u{2b50} \u{2705}\u{2728}", false),
        ("star-text", "\u{2b50}\u{fe0e}", false),
        ("keycap", "1\u{fe0f}\u{20e3}", false),
        ("flag", "🇫🇮!", false),
        ("family", "👨\u{200d}👩\u{200d}👧", false),
        ("toned", "👍🏽", false),
        ("styled", "<b>bold</b> 🔥<br>&lt;i&gt;", true),
        ("styled-none", "<b>bold</b>", true),
    ];
    let texts_ptr = std::ptr::addr_of!(texts);

    // Text with nothing to draw, which is most of what anyone writes: a
    // message without a character of any emoji in it is not read
    // through a character at a time. A digit, # and * among them: they
    // start the keycaps, and are in far more text than any keycap is.
    let plain: Vec<&'static str> = vec![
        "no emoji at all, 1 # * 2",
        "Täällä on lunta -- 3 cm, ja \"pakkasta\" (10°C) <b>&amp;</b>\n  OK?",
        "Встретимся в 19:30 у входа №2.",
    ];
    let plain_ptr = std::ptr::addr_of!(plain);

    single_shot(Duration::from_secs(1), move || unsafe {
        record!("every", call!("everyEmoji"));
        for text in &*plain_ptr {
            record!(
                format!("plain:{text}"),
                call!("mayHold", QString::from(*text))
            );
        }
        record!("base", call!("artBase"));
        for (label, text, styled) in &*texts_ptr {
            record!(*label, call!("inText", QString::from(*text), *styled));
        }
        record!(
            "load",
            call!(
                "loadDelegate",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
        record!(
            "glyph-load",
            call!(
                "loadGlyph",
                QString::from(common::component_url("EmojiGlyph.qml"))
            )
        );
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("body-on", call!("label", QString::from("messageLabel")));
        record!("quote-on", call!("label", QString::from("quoteLabel")));
        record!("glyph-on", call!("glyphText"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    let every = value("every");
    let (count, wrong) = every.split_once(':').unwrap_or(("0", ""));
    assert!(
        count.parse::<usize>().unwrap_or(0) > 1000 && wrong.is_empty(),
        "of the {count} emoji the picker offers, these are not drawn as \
         their pictures in a message, or not looked up as them: {wrong:?}. \
         EmojiPictures.js is built from EmojiData.js by \
         scripts/fetch-emoji.py --pictures. {context}"
    );
    for text in &plain {
        assert_eq!(
            value(&format!("plain:{text}")),
            "false",
            "{text:?} has no emoji in it, and is read through a character \
             at a time anyway. {context}"
        );
    }

    let base = value("base");
    assert!(
        base.starts_with("file://") && base.ends_with("/qml/art/emoji/"),
        "the pictures are looked for in {base:?}. {context}"
    );
    let img = |picture: &str| img(&base, picture);

    for (label, expected) in [
        ("thumb", format!("hi {}", img("1f44d"))),
        ("none", String::new()),
        // The sender's markup is text, their line break a <br> and their
        // runs of spaces kept, which StyledText would fold.
        (
            "escaped",
            format!(
                "a &lt;b&gt; &amp; {}<br>&nbsp;&nbsp;two &nbsp;spaces",
                img("1f44d")
            ),
        ),
        ("copyright-text", String::new()),
        ("copyright-emoji", img("a9")),
        // A heart or a smiley is text by Unicode's default, as the
        // copyright sign is, but whoever sends one bare means the emoji:
        // left to the font, it was Sailfish's glyph where every phone
        // draws the emoji.
        ("heart-text", img("2764")),
        ("heart-emoji", img("2764")),
        ("smiley-text", format!("{} hi", img("263a"))),
        // An emoji bare by Unicode: left to the font, a star sat between
        // pictures in Sailfish's own glyph.
        (
            "star",
            format!("{} {}{}", img("2b50"), img("2705"), img("2728")),
        ),
        // Unless the sender asked for text.
        ("star-text", String::new()),
        ("keycap", img("31-20e3")),
        ("flag", format!("{}!", img("1f1eb-1f1ee"))),
        ("family", img("1f468-200d-1f469-200d-1f467")),
        ("toned", img("1f44d")),
        // The shim's rendering is markup already: left as it is.
        (
            "styled",
            format!("<b>bold</b> {}<br>&lt;i&gt;", img("1f525")),
        ),
        ("styled-none", String::new()),
    ] {
        assert_eq!(value(label), expected, "{label} is wrong. {context}");
    }

    assert_eq!(value("load"), "ok", "the message did not load. {context}");
    assert_eq!(
        value("glyph-load"),
        "ok",
        "the glyph did not load. {context}"
    );
    assert!(
        value("body-on").starts_with("styled:hi <img ")
            && value("body-on").contains("/1f44d.png")
            && value("body-on").ends_with("> there"),
        "the body's emoji is not drawn as its picture. {context}"
    );
    assert!(
        value("quote-on").starts_with("styled:was <img ")
            && value("quote-on").contains("/1f525.png"),
        "the quote's emoji is not drawn as its picture. {context}"
    );
    assert_eq!(value("glyph-on"), "picture", "{context}");
}

/// Only the picker loads the whole list. Every chat opened draws its
/// messages through Emoji.js, and the list -- each emoji's name and search
/// words -- is nearly three times what drawing needs, and is parsed again on
/// every launch: on Qt 5.6 nothing keeps a compiled copy of a script.
#[test]
fn only_the_picker_loads_the_list_with_search_words() {
    let qml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    let mut importers = Vec::new();
    let mut stack = vec![qml];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read qml dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let code = path
                .extension()
                .is_some_and(|extension| extension == "qml" || extension == "js");
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !code {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read qml");
            for (what, imported) in [("EmojiData.js", "list"), ("EmojiPicker.js", "picker")] {
                if text.lines().any(|line| {
                    let line = line.trim();
                    (line.starts_with(".import") || line.starts_with("import"))
                        && line.contains(what)
                }) {
                    importers.push(format!("{imported}:{name}"));
                }
            }
        }
    }
    importers.sort();
    assert_eq!(
        importers,
        ["list:EmojiPicker.js", "picker:ReactionPickerPage.qml"],
        "the emoji list with its search words is loaded by more than the \
         picker: drawing an emoji needs only EmojiPictures.js"
    );
}
