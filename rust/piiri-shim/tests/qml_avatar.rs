//! A chat with a picture has to read as a circle, like the generated
//! initials beside it do.
//!
//! An `Image` does not inherit its parent's corner radius, and `clip` cuts
//! only to the bounding box, so the picture is drawn through an
//! `OpacityMask` instead. This pins that: the raw image must not be what
//! is on screen.
//!
//! And what an avatar is drawn *in*: its own colour on a page, the
//! ambience's where the cover lights up whoever has written -- and what
//! it is drawn as in the moment before the picture has loaded, which is
//! a moment every row of a chat list spends.

// Qt harness: see qml_chat_row.rs.
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
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
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
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The loaded component itself, which has no name to be found by.
        function root(property) { return '' + loader.item[property] }
    }
";

#[test]
fn a_picture_avatar_is_drawn_through_a_round_mask() {
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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("ChatListDelegate.qml"))
            )
        );
        record!(
            "name",
            call!("set", QString::from("chatName"), QString::from("Ada"))
        );

        // No picture: the initial stands in, and nothing is masked.
        record!("plain-masked", get!("avatarMasked", "visible"));
        record!("plain-initial", get!("avatarInitial", "visible"));

        // A picture, from the frame the path arrives in. It is loaded
        // off the main thread, so this frame is the one where there is
        // a path and no picture -- which is the frame a chat list is
        // built in.
        record!(
            "set-picture",
            call!(
                "set",
                QString::from("avatarPath"),
                QString::from(common::a_real_picture())
            )
        );
        record!("loading-initial", get!("avatarInitial", "visible"));
        record!("loading-masked", get!("avatarMasked", "visible"));
    });

    // A second later the picture has loaded, and it is what is drawn.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("picture-status", get!("avatarImage", "status"));
        record!("picture-masked", get!("avatarMasked", "visible"));
        record!("picture-raw", get!("avatarImage", "visible"));
        record!("picture-initial", get!("avatarInitial", "visible"));
        record!("mask-radius", get!("avatarMask", "radius"));
        record!("mask-width", get!("avatarMask", "width"));

        (*engine_ptr).quit();
    });

    engine.exec();

    assert_avatar(&steps);
}

/// What was read off the avatar, before its picture and after it.
fn assert_avatar(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the row did not load. {context}");
    assert_eq!(
        value("plain-masked"),
        "false",
        "a chat with no picture still drew a masked image. {context}"
    );
    assert_eq!(
        value("plain-initial"),
        "true",
        "a chat with no picture drew no initial either, so the avatar is \
         an empty disc. {context}"
    );
    assert_eq!(
        value("set-picture"),
        "ok",
        "the picture could not be set. {context}"
    );
    // Image.Ready is 1.
    assert_eq!(
        value("picture-status"),
        "1",
        "the picture never loaded, so what follows proves nothing about \
         what is drawn once it has. {context}"
    );
    assert_eq!(
        value("loading-initial"),
        "true",
        "the initial stood down the moment a path arrived, before the \
         picture behind it had loaded: a chat list arrives as a column of \
         holes that fill in one by one. {context}"
    );
    assert_eq!(
        value("loading-masked"),
        "false",
        "a picture that has not loaded is on screen, which is nothing. \
         {context}"
    );
    assert_eq!(
        value("picture-initial"),
        "false",
        "the initial is still drawn under the loaded picture. {context}"
    );
    assert_eq!(
        value("picture-masked"),
        "true",
        "a chat with a picture did not draw it through the mask. {context}"
    );
    assert_eq!(
        value("picture-raw"),
        "false",
        "the unmasked image is on screen, so the avatar renders square. {context}"
    );

    // A circle, not a rounded rectangle: the radius is half the width.
    let radius: f64 = value("mask-radius").parse().unwrap_or_default();
    let width: f64 = value("mask-width").parse().unwrap_or_default();
    assert!(width > 0.0, "the mask has no width. {context}");
    assert!(
        (radius - width / 2.0).abs() < 0.5,
        "the mask is not a circle: radius {radius} of width {width}. {context}"
    );
}
