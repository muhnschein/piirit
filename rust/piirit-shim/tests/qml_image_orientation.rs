//! Photo shown the way taken. Device report: sent image arrived upright for receiver, on its
//! side in our view. Sensor reads out landscape however phone is held; EXIF tag, not pixels, says
//! how to turn it. Other clients honour tag; `Image` doesn't unless asked. Pinned: `autoTransform`
//! makes decoded size the seen size (8x16 for stored 16x8 marked rotate 90); row measured from
//! that, not core's header dims (stored ones) -- else turned picture leaves bubble mostly empty.
//!
//! Fixture: hand-written baseline JPEG: nothing in tree can encode one, and only honest way to
//! test a tag is a file carrying it.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

/// 16x8 grey block, EXIF orientation 6 (rotate 90 clockwise): seen 8 wide, 16 high. Minimal:
/// one Huffman code per table, flat quantisation, two bits of scan per block. `file(1)`:
/// "JPEG image data, Exif standard: [TIFF image data, big-endian, direntries=1,
/// orientation=upper-right], baseline, precision 8, 16x8".
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

/// What bubble gives picture to fill.
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

    // SAFETY: single thread; pre-Qt.
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

    let mut plan = common::Plan::default();

    plan.after(0, move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::component_url("AttachmentPreview.qml")),
                QString::from(jpeg.clone())
            ),
        ));
    });
    // Decoded off Qt thread: own size unknown in the asking pass.
    plan.after(2, move || unsafe {
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

    plan.run();
    engine.exec();

    assert_outcome(&steps);
}

/// What run must show, out of test body.
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
    // Image.Ready is 1; else rest proves nothing: failed decode reports no size either.
    assert_eq!(
        value("status"),
        "1",
        "the JPEG did not decode, so nothing below is about orientation. \
         {context}"
    );

    // Twice as high as wide, any size: row bounds decode with `sourceSize`; Qt 5.15 scales to fit
    // both ways, device Qt 5.6 only down. Turn is what's checked; shape shows it either way.
    let width: f64 = value("decoded-width").parse().unwrap_or(0.0);
    let height: f64 = value("decoded-height").parse().unwrap_or(0.0);
    assert!(
        width > 0.0 && (height - 2.0 * width).abs() < 0.01,
        "the picture was decoded at its stored shape rather than the shape it \
         is meant to be seen at: `autoTransform` is what reads the \
         orientation tag, and without it a photo taken in portrait lies on \
         its side. {context}"
    );

    // 160 wide, twice as high once turned.
    assert_eq!(
        value("row-height"),
        (BUBBLE * 2).to_string(),
        "the row was measured from the dimensions the core read out of the \
         file's header, which are the ones before the turn. The picture \
         fits itself into the box, so a box shaped the other way round \
         leaves most of the bubble empty. {context}"
    );
}
