//! No app shader between avatar picture and screen, in any avatar state.
//!
//! Cover faces became squares (2.0 grey/tinted, issue #102; 2.1 white squares). Cause, Sailfish Qt
//! 5.6: hidden window's render thread deletes every `ShaderEffect`'s `QSGMaterialType`, but
//! effects' materials and compiled-program cache keyed by that address live on. A new
//! `ShaderEffect` reusing the address draws with the old program, uniforms by position: `diameter`
//! lands in fade's `qt_Opacity`, a white square. Cover is hidden whenever home screen is left and
//! remakes faces per message, so hit within a day. Fixed only in Qt 5.15.
//!
//! Rule: no `ShaderEffect`, `ShaderEffectSource` or `layer` (nor `QtGraphicalEffects`) inside
//! avatar, read off whole live tree. Picture made before screen (`src/pictures.rs`), drawn by plain
//! `Image`. `qml_syntax.rs` holds rest of app to it; `qml_quick_actions_cover.rs` the cover.

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
        // Whether the disc is drawn through a gradient, and how many
        // gradients the avatar holds whether or not it draws one.
        function fill() {
            var held = 0
            var kids = loader.item.data
            for (var i = 0; i < kids.length; i++) {
                if (('' + kids[i]).indexOf('QQuickGradient') === 0) { held++ }
            }
            return (loader.item.gradient ? 'gradient' : 'flat') + ':' + held
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn an_avatar_draws_its_picture_without_a_shader_in_any_state() {
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
        record!("plain-fill", call!("fill"));
        // Loaded off main thread: read a second later.
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
        // In its own colours, as page draws it.
        record!("page", call!("owned"));

        // Grey, as cover draws everyone with nothing new: fades into the strip quick actions
        // draw in.
        call!("set", QString::from("monochrome"), true);
        call!("set", QString::from("opacity"), 0.6);
        call!("set", QString::from("fadeTo"), 1.5);
        call!("set", QString::from("fadeFrom"), 0.5);
        record!("cover-quiet", call!("owned"));

        // Lit, as cover draws whoever wrote.
        call!("set", QString::from("monochrome"), false);
        call!("set", QString::from("highlight"), true);
        call!("set", QString::from("opacity"), 1.0);
        record!("cover-lit", call!("owned"));

        // No picture: disc fades the same way.
        call!("set", QString::from("picturePath"), QString::from(""));
        record!("cover-disc", call!("owned"));
        record!("cover-disc-fill", call!("fill"));

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
    // `Image.Ready` is 1. Without it picture chain may be unbuilt; empty answer proves nothing.
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
    // Fade gradient is cover's alone, built only where avatar fades: every avatar in every list
    // built one for nothing.
    assert_eq!(
        value("plain-fill"),
        "flat:0",
        "an avatar that does not fade built a gradient anyway. {context}"
    );
    assert!(
        value("cover-disc-fill").starts_with("gradient"),
        "a fading avatar with no picture does not fade its disc. {context}"
    );
}
