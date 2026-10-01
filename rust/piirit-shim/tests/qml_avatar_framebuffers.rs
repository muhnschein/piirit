//! Nothing between an avatar's picture and the screen is drawn into a
//! framebuffer of its own (issue #102).
//!
//! After a long while the cover's faces would turn into flat squares:
//! grey where the faces were, tinted where someone had written, with the
//! pill and the quick actions beside them drawn as ever. Each face was
//! drawn through four or five framebuffers -- the mask and the picture
//! each copied into one for an `OpacityMask`, the mask's result kept in
//! another (`cached`), the greying drawn out of a layer, the tint kept in
//! one more -- and the cover makes every face again for every message.
//! Qt 5.6 never asks whether a framebuffer it made is any good
//! (`QSGDefaultLayer` binds `QOpenGLFramebufferObject::texture()`, which
//! is 0 for one that failed), so when the phone cannot give it one the
//! face draws texture 0: opaque black, a square, grey under the cover's
//! opacity and tinted under the overlay, for as long as that face lives.
//! Forcing framebuffer allocation to fail under a real GL context draws
//! exactly that picture.
//!
//! So the rule is structural: in every state the cover or a page draws
//! an avatar in, no item inside it is a `ShaderEffectSource` and none has
//! `layer.enabled` -- the two things that give a Qt Quick item a
//! framebuffer. `QtGraphicalEffects` builds its effects out of
//! `ShaderEffectSource`s, and this reads the whole tree, theirs included.

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
            loader.setSource(url, { width: 120 })
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
        // Every item in the avatar that owns a framebuffer, by what it
        // is: a ShaderEffectSource, or an item drawn through a layer.
        function framebuffers(node, found) {
            if (!node) { return found }
            var what = '' + node
            if (what.indexOf('QQuickShaderEffectSource') === 0) {
                found.push(what.split('(')[0])
            }
            if (node.layer && node.layer.enabled) {
                found.push('layer:' + what.split('(')[0])
            }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                framebuffers(kids[i], found)
            }
            return found
        }
        function owned() { return framebuffers(loader.item, []).join(';') }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn an_avatar_draws_its_picture_without_a_framebuffer_in_any_state() {
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
        call!("set", QString::from("initial"), QString::from("Ada"));
        call!("set", QString::from("ownColor"), QString::from("#ff0000"));
        record!("no-picture", call!("owned"));
        // Loaded off the main thread, so read a second later.
        call!(
            "set",
            QString::from("picturePath"),
            QString::from(common::a_real_picture())
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "picture-status",
            call!("get", QString::from("avatarImage"), QString::from("status"))
        );
        record!(
            "face",
            call!("get", QString::from("avatarFace"), QString::from("visible"))
        );
        // In its own colours, as a page draws it.
        record!("page", call!("owned"));

        // Grey, as the cover draws everyone with nothing new.
        call!("set", QString::from("monochrome"), true);
        call!("set", QString::from("opacity"), 0.6);
        record!("cover-quiet", call!("owned"));

        // Lit, as the cover draws whoever has written.
        call!("set", QString::from("monochrome"), false);
        call!("set", QString::from("highlight"), true);
        call!("set", QString::from("opacity"), 1.0);
        record!("cover-lit", call!("owned"));

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
    // Image.Ready is 1. Without it the picture's own chain may simply not
    // have been built yet, and an empty answer below would prove nothing.
    assert_eq!(
        value("picture-status"),
        "1",
        "the picture never loaded, so nothing here was read off a drawn \
         face. {context}"
    );
    for state in ["no-picture", "page", "cover-quiet", "cover-lit"] {
        assert_eq!(
            value(state),
            "",
            "the avatar ({state}) draws through a framebuffer of its own, \
             which Qt 5.6 draws as a flat black square when the phone \
             could not give it one (issue #102). {context}"
        );
    }
    assert_eq!(
        value("face"),
        "true",
        "the loaded picture is not what is drawn. {context}"
    );
}
