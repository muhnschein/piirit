//! Every kind of attachment the core can hand back, drawn by the one
//! component that knows the difference.
//!
//! The core classifies an attachment and nothing here does, so what is
//! worth testing is that each `viewType` reaches the renderer meant for it
//! and no other -- and that the two cases the core leaves blank do not
//! collapse the row. `real_server.rs` pins that the core really does leave
//! them blank: no dimensions for a GIF, no duration for a sound file.
//!
//! A real 1x1 PNG and a real two-frame GIF are written to disk, because
//! the fallback under test is "size the picture from the loaded image", and
//! an image that does not load has no size to fall back to -- and because
//! the GIF policy counts the movie going round, which takes a movie.

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

/// A 1x1 PNG, as `real_server.rs` sends to the core, and a 2x2 GIF of two
/// frames, 40 ms each, looping for ever: one colour, then the other.
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];
const TWO_FRAME_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x02, 0x00, 0x02, 0x00, 0x91, 0x00, 0x00, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0x21, 0xff, 0x0b, 0x4e, 0x45, 0x54, 0x53,
    0x43, 0x41, 0x50, 0x45, 0x32, 0x2e, 0x30, 0x03, 0x01, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x00,
    0x04, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x02, 0x03,
    0x4c, 0x92, 0x02, 0x00, 0x21, 0xf9, 0x04, 0x00, 0x04, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00,
    0x00, 0x02, 0x00, 0x02, 0x00, 0x00, 0x02, 0x03, 0x94, 0xa4, 0x02, 0x00, 0x3b,
];

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { contentWidth: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // One kind at a time, the way a row is handed one message.
        function show(viewType, path, fileName) {
            loader.item.viewType = viewType
            loader.item.filePath = path
            loader.item.fileName = fileName
            return 'ok'
        }
        function set(property, value) { loader.item[property] = value; return 'ok' }
        function get(property) { return '' + loader.item[property] }
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
        // Which renderers are showing, so a case can assert on the whole
        // set rather than on one member of it: two at once is the bug.
        function showing() {
            var names = ['attachmentImage', 'attachmentAnimation', 'attachmentVideo',
                         'attachmentAudio', 'attachmentVcard', 'attachmentApp',
                         'attachmentLabel']
            var out = []
            for (var i = 0; i < names.length; i++) {
                var item = findIn(loader.item, names[i])
                if (item && item.visible) { out.push(names[i]) }
            }
            return out.join(',')
        }
        function textOf(name, property) {
            var item = findIn(loader.item, name)
            return item ? '' + item[property] : 'missing:' + name
        }
        function sizeOf(bytes) { return loader.item.readableSize(bytes) }
        function replay() { loader.item.replay(); return 'ok' }
        // Whether the mark that plays a stopped GIF is up.
        function marked() {
            var mark = findIn(loader.item, 'gifMark')
            return mark ? '' + mark.visible : 'missing:gifMark'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn each_view_type_reaches_the_renderer_meant_for_it() {
    let temp = std::env::temp_dir().join(format!("piiri-attachments-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let png = temp.join("dot.png");
    let gif = temp.join("dot.gif");
    std::fs::write(&png, ONE_PIXEL_PNG).expect("write png");
    std::fs::write(&gif, TWO_FRAME_GIF).expect("write gif");
    let png = png.to_string_lossy().into_owned();
    let gif = gif.to_string_lossy().into_owned();

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
    macro_rules! show {
        ($kind:expr, $path:expr, $name:expr) => {
            call!(
                "show",
                QString::from($kind),
                QString::from($path),
                QString::from($name)
            )
        };
    }

    let png_for_load = png.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::component_url("AttachmentPreview.qml"))
            ),
        ));
        // Nothing attached: the preview must take no room at all, or every
        // text message gains a gap.
        (*steps_ptr).push(("empty-showing", call!("showing")));
        (*steps_ptr).push(("empty-height", call!("get", QString::from("height"))));
        (*steps_ptr).push(("png", show!("Image", png_for_load.as_str(), "dot.png")));
    });

    let gif_for_show = gif.clone();
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("image-showing", call!("showing")));
        (*steps_ptr).push(("image-height", call!("get", QString::from("height"))));
        (*steps_ptr).push(("image-wide", call!("get", QString::from("wantsFullWidth"))));
        // A GIF, for which the core reports no dimensions at all.
        (*steps_ptr).push(("gif", show!("Gif", gif_for_show.as_str(), "dot.gif")));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // An old GIF: the still and the mark, and no movie.
        (*steps_ptr).push(("gif-showing", call!("showing")));
        (*steps_ptr).push(("gif-marked", call!("marked")));
        (*steps_ptr).push(("gif-height", call!("get", QString::from("height"))));
        // The mark tapped: the movie, over the still, and the mark gone.
        (*steps_ptr).push(("gif-replay", call!("replay")));
        (*steps_ptr).push(("gif-replayed", call!("showing")));
        (*steps_ptr).push(("gif-replayed-marked", call!("marked")));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        // Three runs of 80 ms are long over: the movie has gone, and the
        // mark is back.
        (*steps_ptr).push(("gif-ran-showing", call!("showing")));
        (*steps_ptr).push(("gif-ran-marked", call!("marked")));
        (*steps_ptr).push(("video", show!("Video", "/tmp/clip.mp4", "clip.mp4")));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("video-showing", call!("showing")));
        (*steps_ptr).push(("video-height", call!("get", QString::from("height"))));
        // Told the video's shape -- taken upright -- the box stands up.
        call!("set", QString::from("imageWidth"), 720);
        call!("set", QString::from("imageHeight"), 1280);
        (*steps_ptr).push((
            "video-upright-height",
            call!("get", QString::from("height")),
        ));
        (*steps_ptr).push(("video-width", call!("get", QString::from("contentWidth"))));
        (*steps_ptr).push(("voice", show!("Voice", "/tmp/note.mp3", "note.mp3")));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("voice-showing", call!("showing")));
        (*steps_ptr).push((
            "voice-name",
            call!("textOf", QString::from("audioLabel"), QString::from("text")),
        ));
        // No duration until the player has one; a zero length is a claim.
        (*steps_ptr).push((
            "voice-time",
            call!("textOf", QString::from("audioTime"), QString::from("text")),
        ));
        (*steps_ptr).push(("card", show!("Vcard", "/tmp/ada.vcf", "ada.vcf")));
        (*steps_ptr).push((
            "card-name",
            call!(
                "set",
                QString::from("vcardName"),
                QString::from("Ada Lovelace")
            ),
        ));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("card-showing", call!("showing")));
        (*steps_ptr).push((
            "card-label",
            call!("textOf", QString::from("vcardName"), QString::from("text")),
        ));
        (*steps_ptr).push(("xdc", show!("Webxdc", "/tmp/game.xdc", "game.xdc")));
        (*steps_ptr).push((
            "xdc-bytes",
            call!("set", QString::from("fileBytes"), 2_400_000),
        ));
    });

    let gif_for_new = gif.clone();
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("xdc-showing", call!("showing")));
        (*steps_ptr).push((
            "xdc-label",
            call!(
                "textOf",
                QString::from("attachmentLabel"),
                QString::from("text")
            ),
        ));
        (*steps_ptr).push(("sizes", call!("sizeOf", 999)));
        (*steps_ptr).push(("sizes-mb", call!("sizeOf", 2_400_000)));
        (*steps_ptr).push(("sizes-none", call!("sizeOf", 0)));

        // A picture the core sized and nothing has decoded -- the file is
        // not even there -- takes its shape from the core's answer, so
        // the row is the right height before the decode and does not
        // move when it lands.
        call!("set", QString::from("imageWidth"), 4);
        call!("set", QString::from("imageHeight"), 3);
        (*steps_ptr).push((
            "reserved",
            show!("Image", "/nowhere/to/be/found.jpg", "found.jpg"),
        ));
        (*steps_ptr).push(("reserved-height", call!("get", QString::from("height"))));

        // A GIF that has just arrived plays by itself.
        call!("set", QString::from("isNew"), true);
        (*steps_ptr).push(("new-gif", show!("Gif", gif_for_new.as_str(), "dot.gif")));
        (*steps_ptr).push(("new-gif-showing", call!("showing")));
        (*steps_ptr).push(("new-gif-marked", call!("marked")));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        // And stops by itself.
        (*steps_ptr).push(("new-gif-ran-showing", call!("showing")));
        (*steps_ptr).push(("new-gif-ran-marked", call!("marked")));

        // The same .xdc once the core has read the app inside it: its own
        // card rather than the paperclip row, with the name and the line
        // the app keeps about itself. Only where apps are on, though --
        // off, an app the core read perfectly well is still a file, and
        // the row it draws is the paperclip one above.
        (*steps_ptr).push(("app", show!("Webxdc", "/tmp/game.xdc", "game.xdc")));
        call!(
            "set",
            QString::from("webxdcName"),
            QString::from("Checkers")
        );
        call!(
            "set",
            QString::from("webxdcSummary"),
            QString::from("3 votes")
        );
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        (*steps_ptr).push(("app-off-showing", call!("showing")));
        (*steps_ptr).push(("app-off-is-app", call!("get", QString::from("isApp"))));
        call!("set", QString::from("appsEnabled"), true);
    });

    single_shot(Duration::from_secs(11), move || unsafe {
        (*steps_ptr).push(("app-showing", call!("showing")));
        (*steps_ptr).push((
            "app-name",
            call!("textOf", QString::from("webxdcName"), QString::from("text")),
        ));
        (*steps_ptr).push((
            "app-state",
            call!(
                "textOf",
                QString::from("webxdcSummary"),
                QString::from("text")
            ),
        ));
        // A tap runs it rather than handing the file to another app.
        (*steps_ptr).push(("app-is-app", call!("get", QString::from("isApp"))));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// What the run has to show for itself, out of the test body: what a Qt
/// harness can do in one function is bounded.
// One assertion per thing checked, in the order the steps ran.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");
    let height = |label: &str| value(label).parse::<f64>().unwrap_or(-1.0);

    assert_eq!(value("load"), "ok", "the preview did not load. {context}");

    assert_eq!(
        value("empty-showing"),
        "",
        "a message with no attachment still drew one. {context}"
    );
    assert!(
        height("empty-height") <= 0.0,
        "a message with no attachment still took room, so every text \
         message gains a gap. {context}"
    );

    // Exactly one renderer per kind. Two at once is the bug this is for.
    // A GIF is the still alone until it is playing, and the movie is laid
    // over the still while it is: the still is what sizes the row.
    for (label, expected) in [
        ("image-showing", "attachmentImage"),
        ("gif-showing", "attachmentImage"),
        ("gif-replayed", "attachmentImage,attachmentAnimation"),
        ("gif-ran-showing", "attachmentImage"),
        ("new-gif-showing", "attachmentImage,attachmentAnimation"),
        ("new-gif-ran-showing", "attachmentImage"),
        ("video-showing", "attachmentVideo"),
        ("voice-showing", "attachmentAudio"),
        ("card-showing", "attachmentVcard"),
        ("xdc-showing", "attachmentLabel"),
        ("app-off-showing", "attachmentLabel"),
        ("app-showing", "attachmentApp"),
    ] {
        assert_eq!(
            value(label),
            expected,
            "{label} drew the wrong set of renderers. {context}"
        );
    }

    // The app card is the core's answer drawn: its name, and what it says
    // about itself now.
    assert_eq!(
        value("app-name"),
        "Checkers",
        "the app card does not name the app. {context}"
    );
    assert_eq!(
        value("app-state"),
        "3 votes",
        "the app card does not say what the app says about itself. {context}"
    );
    assert_eq!(
        value("app-is-app"),
        "true",
        "a webxdc is not marked as one, so a tap would hand the .xdc to \
         another app rather than run it. {context}"
    );
    assert_eq!(
        value("app-off-is-app"),
        "false",
        "a webxdc is marked as an app where apps are off, so a tap would \
         run one the reader never asked to be able to run. {context}"
    );

    assert!(
        height("image-height") > 0.0,
        "a picture measured nothing. {context}"
    );
    assert_eq!(
        value("image-wide"),
        "true",
        "a picture did not ask the bubble for the full width. {context}"
    );
    // The core reports no dimensions for a GIF and AnimatedImage reports
    // none either until its movie decodes, so this height can only have
    // come from the still poster or from the square fallback. Zero here is
    // an attachment the reader cannot see.
    assert!(
        height("gif-height") > 0.0,
        "a GIF measured nothing, so a picture the core gave no dimensions \
         for collapses the row. {context}"
    );
    // A GIF from before plays only when asked; one that has just come in
    // plays by itself. The mark is the ask, so it is up exactly when the
    // movie is not.
    assert_eq!(
        value("gif-marked"),
        "true",
        "a GIF that is not playing carries no mark to play it. {context}"
    );
    assert_eq!(value("gif-replay"), "ok", "no replay. {context}");
    assert_eq!(
        value("gif-replayed-marked"),
        "false",
        "the mark stayed up over a playing GIF. {context}"
    );
    assert_eq!(
        value("new-gif-marked"),
        "false",
        "a GIF that has just arrived was left waiting for a tap. {context}"
    );
    // Three runs and no more, counted from the movie going round.
    for label in ["gif-ran-marked", "new-gif-ran-marked"] {
        assert_eq!(
            value(label),
            "true",
            "a GIF kept playing past its three runs, or the mark did not \
             come back when it stopped ({label}). {context}"
        );
    }
    // 540 wide at 4:3 is 405 high, from the core's dimensions alone: the
    // file is not there to decode.
    assert_eq!(value("reserved"), "ok", "no reserved picture. {context}");
    assert!(
        (height("reserved-height") - 405.0).abs() < 0.5,
        "a picture the core sized was not measured from that size before \
         it decoded, so the row changes height when it does and the list \
         jumps under the reader. {context}"
    );
    assert!(
        height("video-height") > 0.0,
        "a video frame measured nothing. {context}"
    );
    assert!(
        height("video-upright-height") > height("video-width"),
        "a video taken upright is boxed on its side. {context}"
    );
    assert!(
        height("video-upright-height") > 2.0 * height("video-height"),
        "the box did not take the video's own shape. {context}"
    );

    assert_eq!(
        value("voice-name"),
        "Voice message",
        "a voice message was named by its file rather than by what it is. {context}"
    );
    assert_eq!(
        value("voice-time"),
        "",
        "a sound file claimed a length before the player had one. {context}"
    );

    assert_eq!(
        value("card-label"),
        "Ada Lovelace",
        "the shared contact's name is not on the card. {context}"
    );

    assert!(
        value("xdc-label").contains("game.xdc") && value("xdc-label").contains("2.4 MB"),
        "the fallback row does not name and size the file: {:?}. {context}",
        value("xdc-label")
    );

    assert_eq!(value("sizes"), "999 B", "small sizes are wrong. {context}");
    assert_eq!(
        value("sizes-mb"),
        "2.4 MB",
        "large sizes are wrong. {context}"
    );
    assert_eq!(
        value("sizes-none"),
        "",
        "an unknown size was rendered as one. {context}"
    );
}
