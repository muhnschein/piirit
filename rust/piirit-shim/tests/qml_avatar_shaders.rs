//! Nothing between an avatar's picture and the screen is drawn by a
//! shader of the app's own, in any state an avatar is drawn in.
//!
//! The cover's faces kept turning into squares. In 2.0 they were flat
//! grey and tinted ones (issue #102); in 2.1, after the framebuffers were
//! taken out, new faces came out as white squares of their whole picture
//! with only their foot showing. The second is, and the first fits, one
//! bug in Sailfish's Qt 5.6: every time a window is hidden, its render
//! thread deletes the
//! `QSGMaterialType` of every `ShaderEffect` (`invalidateOpenGL()` calls
//! `QQuickShaderEffectMaterial::cleanupMaterialCache()` whether or not the
//! scene graph is kept), while the effects' materials, and the renderer's
//! cache of compiled programs keyed by that type's address, live on. A
//! `ShaderEffect` made afterwards can be given a type at an address an old
//! one had, and is then drawn with the old one's program, its uniforms set
//! by position into the other's slots: the face's `diameter` lands in the
//! cover fade's `qt_Opacity`, a white square. The cover is hidden whenever
//! the home screen is left and makes its faces again for every message,
//! so it gets there in a day. Fixed only in Qt 5.15.
//!
//! So the rule is structural: no `ShaderEffect`, no `ShaderEffectSource`
//! and no `layer` -- and so nothing from `QtGraphicalEffects`, which is
//! built of them -- inside an avatar, read off the whole live tree. A
//! picture is made the way it is seen before it reaches the screen
//! (`src/pictures.rs`), and drawn by a plain `Image` with Qt's own
//! material. `qml_syntax.rs` holds the rest of the app to the same rule;
//! `qml_quick_actions_cover.rs` the cover.

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
        // Every item in the avatar drawn by a program of its own, by what
        // it is: a ShaderEffect or ShaderEffectSource, or an item drawn
        // through a layer.
        function shaders(node, found) {
            if (!node) { return found }
            var what = '' + node
            if (what.indexOf('QQuickShaderEffect') === 0) {
                found.push(what.split('(')[0])
            }
            if (node.layer && node.layer.enabled) {
                found.push('layer:' + what.split('(')[0])
            }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                shaders(kids[i], found)
            }
            return found
        }
        function owned() { return shaders(loader.item, []).join(';') }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn an_avatar_draws_its_picture_without_a_shader_in_any_state() {
    // SAFETY: single-threaded test binary; set before Qt starts.
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
            call!(
                "get",
                QString::from("avatarImage"),
                QString::from("visible")
            )
        );
        // In its own colours, as a page draws it.
        record!("page", call!("owned"));

        // Grey, as the cover draws everyone with nothing new, and fading
        // into the strip its quick actions are drawn in.
        call!("set", QString::from("monochrome"), true);
        call!("set", QString::from("opacity"), 0.6);
        call!("set", QString::from("fadeTo"), 1.5);
        call!("set", QString::from("fadeFrom"), 0.5);
        record!("cover-quiet", call!("owned"));

        // Lit, as the cover draws whoever has written.
        call!("set", QString::from("monochrome"), false);
        call!("set", QString::from("highlight"), true);
        call!("set", QString::from("opacity"), 1.0);
        record!("cover-lit", call!("owned"));

        // And with no picture, the disc fading the same way.
        call!("set", QString::from("picturePath"), QString::from(""));
        record!("cover-disc", call!("owned"));

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
    for state in [
        "no-picture",
        "page",
        "cover-quiet",
        "cover-lit",
        "cover-disc",
    ] {
        assert_eq!(
            value(state),
            "",
            "the avatar ({state}) is drawn by a shader of its own, which \
             Qt 5.6 can draw with another one's program once its window \
             has been hidden: a square, white or flat. {context}"
        );
    }
    assert_eq!(
        value("face"),
        "true",
        "the loaded picture is not what is drawn. {context}"
    );
}
