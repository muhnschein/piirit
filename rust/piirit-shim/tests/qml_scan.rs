//! Scanning a QR code: view runs camera only while on screen; frame with a code comes
//! back as its text; view then stays for host to take down.
//!
//! Frame made here, not by a camera: test's `QrCode` encodes an invite, test draws
//! its modules into the PGM Qt would write; view's `QrScanner` reads it back via the
//! same call as viewfinder timer. Whole shim path under Qt event loop, less optics.
//!
//! Other way in: typed. Button under viewfinder opens panel over it, with clipboard
//! in it when it looks like an invite; entered text handed back as a scanned code.
//!
//! Focus left to continuous autofocus; no search until viewfinder tapped: in
//! continuous video focus a search locks lens where it is (issue #122). Tap focuses
//! on its point in plain autofocus, released after hold.

// Qt harness: `unsafe` `env::set_var` before Qt starts (`unused_unsafe`: unsafe only from
// edition 2024), `borrow_as_ptr` for engine pointer.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    // qt_method! params must match dispatcher's by-value params; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::path::Path;
use std::time::{Duration, Instant};

use qmetaobject::*;

mod common;

const INVITE: &str = "https://i.delta.chat/#0123456789ABCDEF&a=them%40example.org&n=Them";

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        property string heard: ''
        Loader { id: loader }
        // What the page draws for the test to photograph.
        QrCode { id: code }
        function encode(text) {
            code.text = text
            return code.size + ':' + code.modules
        }
        function load(url) {
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.scanned.connect(function(text) { heard = text })
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
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
        // What the host says when the view is the one on screen.
        function setActive(active) { loader.item.active = active; return 'ok' }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function setClipboard(text) { Clipboard.text = text; return 'ok' }
        // The Loader sizes what it holds, so this is the viewfinder's
        // own size too.
        function sizeView(width, height) {
            loader.width = parseInt(width, 10)
            loader.height = parseInt(height, 10)
            return 'ok'
        }
        function grabShape() {
            if (!loader.item) { return 'no-view' }
            var size = loader.item.grabSize()
            return size.width + 'x' + size.height
        }
        function cameraOf() {
            return loader.item ? findIn(loader.item, 'camera') : null
        }
        // What the view asked the camera to run its viewfinder at.
        function askedResolution() {
            var camera = cameraOf()
            if (!camera) { return 'no-camera' }
            return camera.viewfinder.resolution.width + 'x'
                 + camera.viewfinder.resolution.height
        }
        // A camera that cannot say what it has: the view must leave it
        // alone rather than ask for a size it never offered.
        function offerNothing() {
            var camera = cameraOf()
            if (!camera) { return 'no-camera' }
            camera.viewfinderResolutions = []
            loader.item.viewfinderChosen = false
            loader.item.chooseViewfinder()
            return 'ok'
        }
        // What the camera was told about focus, and how often it was
        // asked to search and to let go.
        function focusState() {
            var camera = cameraOf()
            if (!camera) { return 'no-camera' }
            var point = camera.focus.customFocusPoint
            return 'capture:' + camera.captureMode
                 + ' focus:' + camera.focus.focusMode
                 + ' point:' + camera.focus.focusPointMode
                 + ' at:' + point.x + ',' + point.y
                 + ' searches:' + camera.searches
                 + ' unlocks:' + camera.unlocks
        }
        // A tap on the viewfinder, at a fraction of its width and height.
        function tapAt(x, y) {
            if (typeof loader.item.focusAt !== 'function') { return 'no-focusAt' }
            loader.item.focusAt(parseFloat(x), parseFloat(y))
            return 'ok'
        }
        // A timer running out, without waiting for it.
        function expire(name) {
            var timer = findIn(loader.item, name)
            if (!timer) { return 'missing:' + name }
            timer.stop()
            timer.triggered()
            return 'ok'
        }
        function decode(path) {
            var scanner = findIn(loader.item, 'scanner')
            if (!scanner) { return 'missing:scanner' }
            scanner.decode(path)
            return 'ok'
        }
        function heardText() { return heard }
    }
";

/// Run `then` once `idle` says so, asked each event-loop turn; at `deadline`, runs it whatever
/// `idle` says. Step reading what an async call left waits on the call, not a time guess: 2s
/// was one, and under coverage the decoder outran it.
// Zero wait: next event-loop turn. Rechecks only once last check ran, so no fixed time to
// overrun.
#[allow(clippy::disallowed_methods)]
fn when_idle<I, T>(idle: I, deadline: Instant, then: T)
where
    I: Fn() -> bool + 'static,
    T: FnOnce() + 'static,
{
    if idle() || Instant::now() >= deadline {
        then();
        return;
    }
    let mut pending = Some((idle, then));
    single_shot(Duration::ZERO, move || {
        if let Some((idle, then)) = pending.take() {
            when_idle(idle, deadline, then);
        }
    });
}

/// Modules as page's `QrCode` reports, drawn into P5 PGM with quiet zone, four pixels
/// per module: what Qt writes for `.pgm`, less camera.
fn write_frame(encoded: &str, path: &Path) -> Result<(), String> {
    let (size, modules) = encoded
        .split_once(':')
        .ok_or_else(|| format!("no size in {encoded:?}"))?;
    let size: usize = size.parse().map_err(|err| format!("size: {err}"))?;
    if size == 0 {
        return Err("the code has no modules".to_string());
    }
    let (quiet, scale) = (4, 4);
    let edge = (size + 2 * quiet) * scale;
    let mut pixels = vec![u8::MAX; edge * edge];
    for (row, line) in modules.lines().enumerate() {
        for (column, module) in line.chars().enumerate() {
            if module != '1' {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    pixels[((row + quiet) * scale + dy) * edge + (column + quiet) * scale + dx] = 0;
                }
            }
        }
    }
    let mut bytes = format!("P5\n{edge} {edge}\n255\n").into_bytes();
    bytes.extend_from_slice(&pixels);
    std::fs::write(path, bytes).map_err(|err| err.to_string())
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_code_held_up_to_the_page_comes_back_as_its_text() {
    let temp = std::env::temp_dir().join(format!("piirit-scan-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("create temp dir");
    let frame = temp.join("frame.pgm");

    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("XDG_CACHE_HOME", &temp);
    }

    piirit_shim::register_qml_types();

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

    let mut plan = common::Plan::default();

    let frame_path = frame;
    plan.after(1, move || unsafe {
        // The picture: page's own encoder, drawn into a file.
        let encoded = call!("encode", QString::from(INVITE));
        record!(
            "frame",
            write_frame(&encoded, &frame_path).map_or_else(|err| err, |()| "ok".to_string())
        );
        record!(
            "load",
            call!("load", QString::from(common::component_url("ScanView.qml")))
        );
        // Not on screen yet: camera not running, nothing grabbed.
        record!("camera-before", get!("camera", "running"));
        record!("grabbing-before", get!("grabber", "running"));
        call!("setActive", true);
        record!("camera-active", get!("camera", "running"));
        record!("grabbing-active", get!("grabber", "running"));
        // Continuous focus, left running: camera not told to search.
        record!("focus-idle", call!("focusState"));
        record!("acting-before", get!("acting", "running"));
        // Viewfinder timer's own call, with frame made above.
        record!(
            "decode",
            call!(
                "decode",
                QString::from(frame_path.to_string_lossy().into_owned())
            )
        );
        record!("busy", get!("scanner", "busy"));
    });

    // Decode over: read while running, `heard` is empty.
    plan.after(2, move || {
        when_idle(
            move || unsafe { get!("scanner", "busy") } != "true",
            Instant::now() + Duration::from_secs(20),
            move || unsafe {
                record!("heard", call!("heardText"));
                record!("camera-after", get!("camera", "running"));
                record!("grabbing-after", get!("grabber", "running"));
                record!("acting-after", get!("acting", "running"));
                // Seconds on screen, no search asked: continuous video focus
                // search would lock lens.
                record!("searches", get!("camera", "searches"));
                // Other way in, fresh view: link typed not scanned, clipboard offering it first.
                record!(
                    "reload",
                    call!("load", QString::from(common::component_url("ScanView.qml")))
                );
                call!("setActive", true);
                // Tap: plain autofocus on point under finger for a while,
                // then lock off, continuous back.
                record!(
                    "tap",
                    call!("tapAt", QString::from("0.25"), QString::from("0.75"))
                );
                record!("focus-tapped", call!("focusState"));
                record!("hold-over", call!("expire", QString::from("focusHold")));
                record!("focus-released", call!("focusState"));
                record!("panel-before", get!("linkPanel", "visible"));
                record!(
                    "shopping",
                    call!("setClipboard", QString::from("milk, eggs"))
                );
                record!("open", call!("click", QString::from("typeLinkButton")));
                record!("panel-open", get!("linkPanel", "visible"));
                record!("not-pasted", get!("linkField", "text"));
                record!(
                    "reload-again",
                    call!("load", QString::from(common::component_url("ScanView.qml")))
                );
                call!("setActive", true);
                record!("copied", call!("setClipboard", QString::from(TYPED)));
                record!(
                    "open-again",
                    call!("click", QString::from("typeLinkButton"))
                );
                record!("pasted", get!("linkField", "text"));
                record!("connect", call!("click", QString::from("followButton")));
                record!("typed-heard", call!("heardText"));
                record!("typed-camera", get!("camera", "running"));
                record!("typed-acting", get!("acting", "running"));
                // Frame grabbed in viewfinder's shape, long side capped;
                // viewfinder smaller than cap taken as is, not blown up.
                call!("sizeView", QString::from("1080"), QString::from("1920"));
                record!("grab-tall", call!("grabShape"));
                call!("sizeView", QString::from("1920"), QString::from("1080"));
                record!("grab-wide", call!("grabShape"));
                call!("sizeView", QString::from("540"), QString::from("800"));
                record!("grab-small", call!("grabShape"));
                // Camera asked for viewfinder big enough to read a dense code;
                // left alone when it offers nothing.
                record!("asked", call!("askedResolution"));
                record!("offer-nothing", call!("offerNothing"));
                record!("asked-after", call!("askedResolution"));
                (*engine_ptr).quit();
            },
        );
    });

    plan.run();
    engine.exec();

    assert_scan(&steps);
    assert_typed(&steps);
}

/// Invite the reader types, or copies and lets panel paste.
const TYPED: &str = "https://i.delta.chat/#FEDCBA9876543210&a=typed%40example.org&n=Typed";

/// Panel opens from button, takes clipboard only if it looks like an invite, hands link
/// back as a code is.
fn assert_typed(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    for label in [
        "reload",
        "tap",
        "hold-over",
        "open",
        "reload-again",
        "open-again",
        "connect",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    // Tap searches once, plain autofocus on point under finger: in continuous video focus a
    // search locks lens without looking.
    assert_eq!(
        value("focus-tapped"),
        "capture:2 focus:8 point:3 at:0.25,0.75 searches:1 unlocks:1",
        "a tap did not focus on the point under the finger. {context}"
    );
    // Hold over: lock comes off, continuous focus takes lens back; not fixed for good.
    assert_eq!(
        value("focus-released"),
        "capture:2 focus:16 point:0 at:0.25,0.75 searches:1 unlocks:2",
        "focus stayed locked after a tap. {context}"
    );
    assert_eq!(
        value("panel-before"),
        "false",
        "the link panel is up before anyone asked for it. {context}"
    );
    assert_eq!(
        value("panel-open"),
        "true",
        "the button did not open the link panel. {context}"
    );
    assert_eq!(
        value("not-pasted"),
        "",
        "a clipboard that holds no invite was pasted into the field. {context}"
    );
    assert_eq!(
        value("pasted"),
        TYPED,
        "an invite on the clipboard was not offered in the field. {context}"
    );
    assert_eq!(
        value("typed-heard"),
        TYPED,
        "the typed link was not handed back the way a scanned code is. {context}"
    );
    // 1280 on long side, short in proportion. Square grab of oblong viewfinder stretches
    // modules; decoder reading square symbol as oblong has less to work with.
    assert_eq!(
        value("grab-tall"),
        "720x1280",
        "a tall viewfinder was not grabbed in its own shape. {context}"
    );
    assert_eq!(
        value("grab-wide"),
        "1280x720",
        "a wide viewfinder was not grabbed in its own shape. {context}"
    );
    assert_eq!(
        value("grab-small"),
        "540x800",
        "a viewfinder smaller than the cap was blown up, which invents \
         no detail. {context}"
    );

    // Smallest camera offers that is big enough. 640x480 is platform default, too coarse for
    // dense code; 1920x1440 is more frames than needed.
    assert_eq!(
        value("asked"),
        "1280x960",
        "the view did not ask the camera for a viewfinder big enough to \
         read a dense code out of. {context}"
    );
    assert_eq!(
        value("offer-nothing"),
        "ok",
        "the camera could not be emptied of resolutions. {context}"
    );
    assert_eq!(
        value("asked-after"),
        "1280x960",
        "a camera that offers no resolutions was asked for one anyway, \
         which leaves the view running on a size it never had. {context}"
    );
    assert_eq!(
        value("typed-camera"),
        "false",
        "the camera keeps running after a link was entered. {context}"
    );
    assert_eq!(
        value("typed-acting"),
        "true",
        "the view does not show that the link is being acted on. {context}"
    );
}

/// Camera follows page status, frame decodes to invite, page hands it on and waits.
fn assert_scan(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    for label in ["frame", "load", "decode"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }
    assert_eq!(
        value("camera-before"),
        "false",
        "the camera runs before the view is on screen. {context}"
    );
    assert_eq!(
        value("grabbing-before"),
        "false",
        "frames are grabbed before the view is on screen. {context}"
    );
    assert_eq!(
        value("camera-active"),
        "true",
        "the camera does not start with the view. {context}"
    );
    assert_eq!(
        value("grabbing-active"),
        "true",
        "frames are not grabbed while the view is on screen. {context}"
    );
    assert_eq!(
        value("focus-idle"),
        "capture:2 focus:16 point:0 at:0,0 searches:0 unlocks:0",
        "the camera is not left in continuous focus while scanning. {context}"
    );
    assert_eq!(
        value("searches"),
        "0",
        "a focus search was asked for with nothing tapped. In continuous \
         video focus a search locks the lens where it is, which is how a \
         viewfinder stays blurred (issue #122). {context}"
    );
    assert_eq!(
        value("busy"),
        "true",
        "the scanner does not say it is busy while decoding, so frames \
         would queue behind it. {context}"
    );
    assert_eq!(
        value("heard"),
        INVITE,
        "the frame did not come back as the invite it carried. {context}"
    );
    assert_eq!(
        value("camera-after"),
        "false",
        "the camera keeps running after a code was read. {context}"
    );
    assert_eq!(
        value("grabbing-after"),
        "false",
        "frames keep being grabbed after a code was read. {context}"
    );
    assert_eq!(
        value("acting-before"),
        "false",
        "the view says it is acting on a code before one was read. {context}"
    );
    assert_eq!(
        value("acting-after"),
        "true",
        "the view does not show that the code is being acted on. {context}"
    );
}
