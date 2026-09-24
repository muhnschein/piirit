//! A photo is shown the way it was taken.
//!
//! Reported from a device: an image sent from Piirit arrived the right
//! way up for the person receiving it and lay on its side in our own
//! message view. Cameras do that -- the sensor reads out landscape however
//! the phone is held, and the file says which way to turn it afterwards, in
//! an EXIF tag rather than in the pixels. Every other client honours the
//! tag. `Image` does not, unless it is asked to.
//!
//! Two things follow from asking it, and this pins both. `autoTransform`
//! makes the decoded image the size it is meant to be seen at -- eight
//! wide by sixteen high, for a file stored sixteen by eight and marked
//! "rotate 90". And the row has to be measured from *that* rather than from
//! the dimensions the core read out of the file's header, which are the
//! stored ones: a picture turned a quarter turn in a box shaped for the
//! other orientation is a picture with most of its bubble empty.
//!
//! The fixture below is a whole baseline JPEG, written out by hand, because
//! nothing in the tree can encode one and the only honest way to test what
//! a decoder does with a tag is to hand it a file carrying the tag.

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

/// 16x8, one grey block, EXIF orientation 6 -- "rotate 90 clockwise", so it
/// is meant to be seen 8 wide and 16 high. Minimal on purpose: a single
/// Huffman code per table, a flat quantisation table, and two bits of scan
/// data per block. `file(1)` reads it as
/// "JPEG image data, Exif standard: [TIFF image data, big-endian,
/// direntries=1, orientation=upper-right], baseline, precision 8, 16x8".
const SIDEWAYS_JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe1, 0x00, 0x22, 0x45, 0x78, 0x69, 0x66, 0x00, 0x00, 0x4d, 0x4d, 0x00, 0x2a,
    0x00, 0x00, 0x00, 0x08, 0x00, 0x01, 0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x06,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
    0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0xff, 0xc0, 0x00, 0x0b, 0x08,
    0x00, 0x08, 0x00, 0x10, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xc4,
    0x00, 0x14, 0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00, 0x0f, 0xff,
    0xd9,
];

/// What the bubble gives the picture to fill.
const BUBBLE: i32 = 160;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        // Loaded with the dimensions the core reports, which it reads out
        // of the file's header: the stored ones, before the turn.
        function load(url, path) {
            loader.setSource(url, {
                contentWidth: 160,
                viewType: 'Image',
                filePath: path,
                fileName: 'photo.jpg',
                imageWidth: 16,
                imageHeight: 8
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function heightOfRow() { return '' + Math.round(loader.item.height) }
    }
";

#[test]
fn a_photo_with_an_orientation_tag_is_shown_turned_and_measured_turned() {
    let temp = std::env::temp_dir().join(format!("piirit-orientation-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let jpeg = temp.join("sideways.jpg");
    std::fs::write(&jpeg, SIDEWAYS_JPEG).expect("write jpeg");
    let jpeg = jpeg.to_string_lossy().into_owned();

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

    single_shot(Duration::from_millis(200), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::component_url("AttachmentPreview.qml")),
                QString::from(jpeg.clone())
            ),
        ));
    });
    // The image is decoded off the Qt thread, so its own size is not known
    // in the pass that asks for it.
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push((
            "status",
            call!(
                "get",
                QString::from("attachmentImage"),
                QString::from("status")
            ),
        ));
        (*steps_ptr).push((
            "decoded-width",
            call!(
                "get",
                QString::from("attachmentImage"),
                QString::from("implicitWidth")
            ),
        ));
        (*steps_ptr).push((
            "decoded-height",
            call!(
                "get",
                QString::from("attachmentImage"),
                QString::from("implicitHeight")
            ),
        ));
        (*steps_ptr).push(("row-height", call!("heightOfRow")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// What the run has to show for itself, out of the test body.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the preview did not load. {context}");
    // Image.Ready is 1. Anything else and the rest of this proves nothing:
    // an image that failed to decode reports no size either.
    assert_eq!(
        value("status"),
        "1",
        "the JPEG did not decode, so nothing below is about orientation. \
         {context}"
    );

    // Twice as high as it is wide, whatever the size: the row bounds the
    // decode with `sourceSize`, and the host's Qt (5.15) scales a picture
    // to fit that bound in both directions, where the device's (5.6) only
    // ever scales down. The turn is what is being checked, and it shows
    // in the shape either way.
    let width: f64 = value("decoded-width").parse().unwrap_or(0.0);
    let height: f64 = value("decoded-height").parse().unwrap_or(0.0);
    assert!(
        width > 0.0 && (height - 2.0 * width).abs() < 0.01,
        "the picture was decoded at its stored shape rather than the shape it \
         is meant to be seen at: `autoTransform` is what reads the \
         orientation tag, and without it a photo taken in portrait lies on \
         its side. {context}"
    );

    // 160 wide, twice as high as it is wide once turned.
    assert_eq!(
        value("row-height"),
        (BUBBLE * 2).to_string(),
        "the row was measured from the dimensions the core read out of the \
         file's header, which are the ones before the turn. The picture \
         fits itself into the box, so a box shaped the other way round \
         leaves most of the bubble empty. {context}"
    );
}
