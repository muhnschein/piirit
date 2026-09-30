//! What the avatar draws, measured: a circle, with the picture in it.
//!
//! The rest of the suite reads properties and none of it says a page
//! looks right. This one says what issue #102 was about, in the two
//! facts the broken cover showed: the avatars were *squares*, and they
//! were *flat* -- the picture's texture gone and the mask with it, so
//! every cell was one colour in a box. So the drawn pixels are measured:
//! the corners of the drawing have to be transparent (a circle, never a
//! square) and the drawing has to vary (a picture, never a flat fill).
//!
//! Every state the cover draws an avatar in gets the same measurement,
//! and so does the cover's own nesting -- a layer running a fade shader
//! over it -- and the two lives a cover's cells lead: made again for
//! every arrival, and hidden and shown with the app.
//!
//! This draws, so it runs where a run asks for it: with `PIIRIT_RENDER`
//! set, on a surface that can render. `ci.yml`'s `render` job is that
//! place. A suite run leaves it be, and the properties the drawing is
//! built from are `qml_avatar.rs`'s and `qml_avatar_colour.rs`'s.

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

const PROBE_QML: &str = r#"
    import QtQuick 2.0
    import QtQuick.Window 2.2
    Window {
        id: win
        width: 140
        height: 140
        visible: true
        property string avatarUrl: ''
        property string pic: ''
        property bool mono: false
        property bool loud: false
        property string pending: ''
        property var shots: ({})

        Item {
            id: host
            x: 10
            y: 10
            width: 120
            height: 120
            Loader { id: loader }
        }

        // The cover's nesting: the avatar under a layer whose effect
        // fades it towards the quick actions at the bottom edge. The
        // shader is the cover's own, pieced together the way
        // qml/js/QuickActions.js pieces it.
        Item {
            id: layerHost
            x: 10
            y: 10
            width: 120
            height: 120
            visible: false
            layer.enabled: true
            layer.effect: ShaderEffect {
                property real fadeFrom: layerHost.height - 2 * 50
                property real fadeTo: layerHost.height
                property real gridHeight: Math.max(1, layerHost.height)
                fragmentShader:
                    "varying highp vec2 qt_TexCoord0;\n" +
                    "uniform sampler2D source;\n" +
                    "uniform highp float fadeFrom;\n" +
                    "uniform highp float fadeTo;\n" +
                    "uniform highp float gridHeight;\n" +
                    "uniform lowp float qt_Opacity;\n" +
                    "void main() {\n" +
                    "    highp float y = qt_TexCoord0.y * gridHeight;\n" +
                    "    highp float sink = clamp((fadeTo - y) / max(1.0, fadeTo - fadeFrom), 0.0, 1.0);\n" +
                    "    gl_FragColor = texture2D(source, qt_TexCoord0) * (sink * sink) * qt_Opacity;\n" +
                    "}\n"
            }
            Loader { id: layered }
        }

        Image {
            id: shot
            visible: false
            asynchronous: false
            onStatusChanged: {
                if (status === Image.Ready && win.pending !== '') {
                    canvas.requestPaint()
                }
            }
        }

        // The measurement, on the drawing's own pixels: the corners'
        // alpha (a circle has none) and how far apart its lightest and
        // darkest points are (a flat fill has none). Beside the avatar
        // rather than over it: a grab takes one item's own drawing.
        Canvas {
            id: canvas
            x: 10
            y: 10
            width: 120
            height: 120
            onPaint: {
                // The canvas paints itself empty once when it is made,
                // before any drawing has been handed to it.
                if (win.pending === '') { return }
                var ctx = getContext('2d')
                ctx.clearRect(0, 0, width, height)
                ctx.drawImage(shot, 0, 0, width, height)
                var d = ctx.getImageData(0, 0, width, height).data
                var middle = d[((60 * 120) + 60) * 4 + 3]
                var lo = 765
                var hi = 0
                for (var i = 0; i < d.length; i += 4) {
                    var lit = d[i] + d[i + 1] + d[i + 2]
                    if (lit < lo) { lo = lit }
                    if (lit > hi) { hi = lit }
                }
                win.shots[win.pending] = 'corner=' + d[3]
                                        + ';middle=' + middle
                                        + ';spread=' + (hi - lo)
                win.pending = ''
            }
        }

        function props() {
            return { width: 120,
                     picturePath: win.pic,
                     monochrome: win.mono,
                     highlight: win.loud }
        }
        function load(url) {
            win.avatarUrl = url
            loader.setSource(url, props())
            layered.setSource(url, props())
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function state(pic, mono, loud) {
            win.pic = pic
            win.mono = mono
            win.loud = loud
            loader.item.picturePath = win.pic
            loader.item.monochrome = win.mono
            loader.item.highlight = win.loud
            layered.item.picturePath = win.pic
            layered.item.monochrome = win.mono
            layered.item.highlight = win.loud
            return 'ok'
        }
        // Destroy and make the avatars again, the way the cover's grid
        // used to do whenever anything anywhere is said.
        function churn(times) {
            for (var i = 0; i < times; i++) {
                loader.setSource('')
                loader.setSource(win.avatarUrl, props())
            }
            return 'ok'
        }
        function hideshow(times) {
            for (var i = 0; i < times; i++) {
                host.visible = false
                host.visible = true
            }
            return 'ok'
        }
        function flip(times) {
            for (var i = 0; i < times; i++) {
                state(win.pic, true, false)
                state(win.pic, false, true)
            }
            state(win.pic, true, false)
            return 'ok'
        }
        function winhide(times) {
            for (var i = 0; i < times; i++) {
                win.visible = false
                win.visible = true
            }
            return 'ok'
        }
        // What a drawing looked like, measured.
        function shoot(label, underLayer) {
            win.pending = label
            if (underLayer) {
                host.visible = false
                layerHost.visible = true
                layerHost.grabToImage(function (result) { shot.source = result.url })
            } else {
                layerHost.visible = false
                host.visible = true
                host.grabToImage(function (result) { shot.source = result.url })
            }
            return 'asked'
        }
        function report() {
            var said = ''
            for (var label in win.shots) {
                said = said + label + '=' + win.shots[label] + ' '
            }
            return said
        }
    }
"#;

#[test]
#[allow(clippy::too_many_lines)]
fn the_picture_is_a_circle_with_the_picture_in_it() {
    // This draws. A run that has not asked for what things look like --
    // `ci.yml`'s test job, `make test` -- leaves it be; `make render` and
    // `ci.yml`'s render job are where it runs.
    if std::env::var_os("PIIRIT_RENDER").is_none() {
        eprintln!("skipping: PIIRIT_RENDER is not set, and this test draws");
        return;
    }

    // SAFETY: single-threaded test binary; set before Qt starts. xcb, not
    // the offscreen platform the suite runs on: a window is what this
    // draws and measures, and offscreen has nowhere to put one.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "xcb");
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

    // One drawing a tick: what a tick draws is measured after it, and
    // two drawings in one tick would measure the last of them.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::component_url("Avatar.qml")))
        );
        record!(
            "plain",
            call!(
                "state",
                QString::from(common::a_real_picture()),
                false,
                false
            )
        );
    });

    // The picture loads off the main thread; everything below draws the
    // picture that has arrived, not the disc standing in for it.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("shoot-plain", call!("shoot", QString::from("plain"), false));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        // The cover's default cell: the face greyed.
        record!(
            "mono",
            call!(
                "state",
                QString::from(common::a_real_picture()),
                true,
                false
            )
        );
        record!("shoot-mono", call!("shoot", QString::from("mono"), false));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        // The cover's cell with news: the greyed face in the ambience's
        // colour.
        record!(
            "loud",
            call!(
                "state",
                QString::from(common::a_real_picture()),
                false,
                true
            )
        );
        record!("shoot-loud", call!("shoot", QString::from("loud"), false));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        // The same greyed face under the cover's own layer and fade.
        record!(
            "shoot-mono-layer",
            call!("shoot", QString::from("mono-layer"), true)
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        // And the lit one.
        record!(
            "loud",
            call!(
                "state",
                QString::from(common::a_real_picture()),
                false,
                true
            )
        );
        record!(
            "shoot-loud-layer",
            call!("shoot", QString::from("loud-layer"), true)
        );
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        // Quiet to loud and back: what every arrival and every read does
        // to a cell.
        record!("flip", call!("flip", 5i32));
        record!("shoot-flip", call!("shoot", QString::from("flip"), false));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        // The cover's cells were made again for every arrival.
        record!("churn", call!("churn", 50i32));
        record!("shoot-churn", call!("shoot", QString::from("churn"), false));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        // And hidden and shown with the app, and with the display.
        record!("hideshow", call!("hideshow", 50i32));
        record!("winhide", call!("winhide", 5i32));
        record!(
            "shoot-after-life",
            call!("shoot", QString::from("after-life"), false)
        );
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!("report", call!("report"));
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

    // Every drawing that was measured, out of the report the probe made.
    let mut seen = 0;
    for field in value("report").split_whitespace() {
        let Some((name, measurements)) = field.split_once('=') else {
            continue;
        };
        seen += 1;
        let read = |key: &str| -> i64 {
            measurements
                .split(';')
                .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
                .and_then(|number| number.parse().ok())
                .unwrap_or(-1)
        };
        assert_eq!(
            read("corner"),
            0,
            "{name} was drawn as a square: its corners are not transparent. \
             {context}"
        );
        assert!(
            read("middle") > 0,
            "{name} was drawn with no picture in the middle of it. {context}"
        );
        assert!(
            read("spread") > 8,
            "{name} was drawn flat ({}) -- one colour in a box where the \
             picture should be, which is what the broken cover showed. \
             {context}",
            read("spread")
        );
    }
    assert!(
        seen >= 8,
        "not everything was measured ({seen} drawings), so this says less \
         than it looks like it does. {context}"
    );
}
