//! What an avatar is drawn *in*: its own colour on a page, the ambience's
//! where the cover lights up whoever has written.
//!
//! A file of its own because a Qt event loop is: one `QmlEngine` per test
//! binary, as every other qml_*.rs here does it.
//!
//! The stub's `Theme.highlightColor` is what the assertions read, so this
//! is about which colour is asked for, not what an ambience makes of it.

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
        // What the face is drawn through: how much colour is taken out,
        // and the tint over it as 0-255 channels and an alpha.
        function face() {
            var face = findIn(loader.item, 'avatarFace')
            if (!face) { return 'missing:avatarFace' }
            var t = face.tint
            return face.desaturation + '|' + [Math.round(t.r * 255), Math.round(t.g * 255),
                                              Math.round(t.b * 255), t.a.toFixed(2)].join(',')
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn an_avatar_with_news_wears_the_ambiences_colour_rather_than_its_own() {
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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::component_url("Avatar.qml")))
        );
        record!(
            "initial",
            call!("set", QString::from("initial"), QString::from("Ada"))
        );
        call!("set", QString::from("ownColor"), QString::from("#ff0000"));
        record!("own", call!("root", QString::from("color")));

        // Grey, as everyone on the cover is drawn who has nothing new.
        call!("set", QString::from("monochrome"), true);
        record!("grey", call!("root", QString::from("color")));

        // Lit: the ambience's highlight, not the person's colour.
        call!("set", QString::from("highlight"), true);
        record!("lit", call!("root", QString::from("color")));

        // A picture goes through the same colour rather than showing its
        // own. Set here and read a second later: it is loaded off the
        // main thread, and an avatar draws the disc until it is there.
        call!(
            "set",
            QString::from("picturePath"),
            QString::from(common::a_real_picture())
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "lit-visible",
            call!("get", QString::from("avatarFace"), QString::from("visible"))
        );
        record!("lit-face", call!("face"));
        record!(
            "lit-raw",
            call!(
                "get",
                QString::from("avatarImage"),
                QString::from("visible")
            )
        );

        call!("set", QString::from("highlight"), false);
        record!("quiet-face", call!("face"));

        call!("set", QString::from("monochrome"), false);
        record!("own-face", call!("face"));
        record!(
            "picture-status",
            call!("get", QString::from("avatarImage"), QString::from("status"))
        );

        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the avatar did not load. {context}");
    assert_eq!(
        value("own"),
        "#ff0000",
        "an avatar left alone is not drawn in the colour it was given. \
         {context}"
    );
    assert_ne!(
        value("grey"),
        "#ff0000",
        "an avatar with nothing new is still drawn in its own colour. \
         {context}"
    );
    assert_eq!(
        value("lit"),
        "#80c0ff",
        "an avatar with something new is not drawn in the ambience's \
         highlight (the stub's Theme.highlightColor). {context}"
    );
    assert_eq!(
        value("lit-visible"),
        "true",
        "a lit picture is not drawn. {context}"
    );
    assert_eq!(
        value("lit-face"),
        "1|128,192,255,0.75",
        "a lit picture is not a grey face through three quarters of the \
         ambience's highlight (the stub's Theme.highlightColor). {context}"
    );
    assert_eq!(
        value("lit-raw"),
        "false",
        "the raw image is on screen, so a lit avatar renders square. \
         {context}"
    );
    assert_eq!(
        value("quiet-face"),
        "1|0,0,0,0.00",
        "a picture with nothing new is not grey, or is still put through \
         the ambience's colour. {context}"
    );
    assert_eq!(
        value("own-face"),
        "0|0,0,0,0.00",
        "a picture on a page is not drawn in its own colours. {context}"
    );
    // Image.Ready is 1. Without it the three above would all be false
    // for the honest reason that there is no picture yet, and would say
    // nothing about the colour it is drawn in.
    assert_eq!(
        value("picture-status"),
        "1",
        "the picture never loaded, so nothing here was read off a drawn \
         face. {context}"
    );
}
