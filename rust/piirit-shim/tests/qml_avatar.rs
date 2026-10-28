//! Chat picture reads as a circle, like the initials beside it.
//!
//! `Image` doesn't inherit corner radius; `clip` cuts only to bounding box. So app's image provider
//! cuts picture to circle (`src/pictures.rs`, checked by `qml_pictures.rs`). Avatar asks for that
//! circle at drawn size, not raw file.
//!
//! Also what avatar is drawn in: own colour on a page, ambience's where cover lights up; and its
//! look before picture loads.

// Qt harness: see qml_chat_row.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

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
fn a_picture_avatar_is_drawn_cut_to_a_circle() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    piirit_shim::install_pictures(&engine);
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
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

        // No picture: initial stands in; none drawn.
        record!("plain-picture", get!("avatarImage", "visible"));
        record!("plain-initial", get!("avatarInitial", "visible"));

        // Picture from frame path arrives in. Loaded off main thread: this frame has path, no
        // picture; chat lists are built in this frame.
        record!(
            "set-picture",
            call!(
                "set",
                QString::from("avatarPath"),
                QString::from(common::a_real_picture())
            )
        );
        record!("loading-initial", get!("avatarInitial", "visible"));
        record!("loading-picture", get!("avatarImage", "visible"));
    });

    // Picture loaded, however slow the load (`Image.Ready` is 1), drawn.
    plan.until(1, 30, move |last| unsafe {
        if get!("avatarImage", "status") != "1" && !last {
            return false;
        }
        record!("picture-status", get!("avatarImage", "status"));
        record!("picture-shown", get!("avatarImage", "visible"));
        record!("picture-initial", get!("avatarInitial", "visible"));
        record!("picture-source", get!("avatarImage", "source"));
        // What provider made, which is drawn.
        record!("made-width", get!("avatarImage", "implicitWidth"));
        record!("made-height", get!("avatarImage", "implicitHeight"));
        record!("face-width", get!("avatarImage", "width"));
        record!("face-height", get!("avatarImage", "height"));

        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    assert_avatar(&steps);
}

/// What was read off avatar, before and after picture.
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
        value("plain-picture"),
        "false",
        "a chat with no picture still drew a picture. {context}"
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
    // `Image.Ready` is 1.
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
        value("loading-picture"),
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
        value("picture-shown"),
        "true",
        "a chat with a picture did not draw it. {context}"
    );
    let source = value("picture-source");
    assert!(
        source.starts_with("image://piirit/face?file="),
        "the avatar draws its file as it is, square, rather than the face \
         the app cuts to a circle: {source}. {context}"
    );
    assert!(
        !source.contains("grey=1") && !source.contains("tint="),
        "a chat-list avatar asked for its picture without its colours: \
         {source}. {context}"
    );

    // Circle, not oval or disc in square: square at drawn size, so one-pixel rim is one pixel on
    // screen.
    let number = |label: &str| value(label).parse::<f64>().unwrap_or_default();
    let (width, height) = (number("face-width"), number("face-height"));
    let (made_width, made_height) = (number("made-width"), number("made-height"));
    assert!(width > 0.0, "the face has no width. {context}");
    assert!(
        (width - height).abs() < 0.5
            && (made_width - width).abs() < 1.0
            && (made_height - height).abs() < 1.0,
        "the face is not a circle the size of the avatar: made {made_width} \
         by {made_height} for a face {width} by {height}. {context}"
    );
}
