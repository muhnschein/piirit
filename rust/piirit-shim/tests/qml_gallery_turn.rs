//! Viewer turned and tapped: page follows phone (landscape too), list keeps slide on screen;
//! video turned by page only (screen-following turn doubled it: sideways, distorted); video's
//! time from row until player knows it (Qt 5.6's -1 read "-1:0-1" mid-swipe); header names
//! sender with day, "Back" top left pops page; single tap on picture hides whole header (back
//! arrow too), again brings it; double tap zooms, header stays; video slide keeps header.
//!
//! Headless vs stub Silica; group seeded with picture (10, Ada Lovelace) and video (15, 7 s).
//! Turn = page's width and height swapped, as Silica does. Steps a second apart (`single_shot`
//! counts whole seconds).

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        id: probe
        Loader { id: loader }
        /// Silica's, as page finds it: pops counted.
        property QtObject pageStack: QtObject {
            property int pops: 0
            function pop() { pops += 1; return null }
        }
        function pops() { return '' + probe.pageStack.pops }
        function load(url, messageId, viewType, file) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: 1, chatId: 2, messageId: messageId, viewType: viewType,
                fileUrl: file, fileName: 'tapped', newestFirst: false, status: 2
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadVideo(url, durationMs) {
            loader.setSource('', {})
            loader.setSource(url, { width: 540, height: 960, durationMs: durationMs })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            if (node.item) { return findIn(node.item, name) }
            return null
        }
        function slides() { return findIn(loader.item, 'slides') }
        function page(property) { return '' + loader.item[property] }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function setItem(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function current(name, property) {
            var slide = slides().currentItem
            if (!slide) { return 'no-current' }
            var item = name.length > 0 ? findIn(slide, name) : slide
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function setCurrent(name, property, value) {
            var slide = slides().currentItem
            var item = slide ? findIn(slide, name) : null
            if (!item) { return 'missing:' + name }
            item[property] = value
            return 'ok'
        }
        function swipeTo(index) {
            slides().currentIndex = index
            slides().positionViewAtIndex(index, ListView.SnapPosition)
            return 'ok'
        }
        /// Phone turned, as Silica: transition on, page's width and height swapped, frames
        /// drawn (list laid out anew; headless has no frames: forced), transition over.
        function turn(width, height) {
            var page = loader.item
            page.orientationTransitionRunning = true
            page.width = width
            page.height = height
            slides().forceLayout()
            page.orientationTransitionRunning = false
            return 'ok'
        }
        /// Where list stands: offset from its start / slide width.
        function standing() {
            var list = slides()
            return Math.round(list.contentX - list.originX) + '/' + Math.round(list.width)
        }
        /// Single tap on picture now on screen; double: tap, then second within interval.
        function tapCurrent() {
            var tap = findIn(slides().currentItem, 'pictureTap')
            if (!tap) { return 'missing:pictureTap' }
            tap.clicked(null)
            return 'ok'
        }
        function doubleTapCurrent() {
            var slide = slides().currentItem
            var tap = findIn(slide, 'pictureTap')
            var view = findIn(slide, 'pictureView')
            if (!tap || !view) { return 'missing:pictureTap' }
            tap.clicked(null)
            view.zoomTap(100, 100)
            return '' + view.zoomed
        }
        function tapBack() {
            var back = findIn(loader.item, 'backButton')
            if (!back) { return 'missing:backButton' }
            back.clicked(null)
            return 'ok'
        }
        /// Header and Silica's back arrow both shown, or both gone.
        function chrome() {
            return get('header', 'visible') + '/' + page('showNavigationIndicator')
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_viewer_turns_with_the_phone_and_a_tap_hides_its_header() {
    let temp = std::env::temp_dir().join(format!("piirit-gallery-turn-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_MEDIA", "1");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

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
    macro_rules! s {
        ($text:expr) => {
            QString::from($text)
        };
    }

    let gallery = common::page_url("GalleryPage.qml");
    let video = common::component_url("VideoView.qml");

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                s!(gallery.clone()),
                10,
                s!("Image"),
                s!("file:///tmp/piirit-fake/photo.jpg")
            )
        );
    });

    // Header: sender and day, Back. Video: page turns it; time from row mid-swipe.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("allowed", call!("page", s!("allowedOrientations")));
        record!(
            "sender-day",
            call!("get", s!("pageHeader"), s!("description"))
        );
        record!("back-label", call!("get", s!("backLabel"), s!("text")));
        record!("tap-back", call!("tapBack"));
        record!("popped", call!("pops"));
        record!("swipe-video", call!("swipeTo", 1));
        record!(
            "video-turned-by-screen",
            call!("current", s!("videoOutput"), s!("autoOrientation"))
        );
        // Qt 5.6's player before it has file: -1.
        record!(
            "unknown",
            call!("setCurrent", s!("player"), s!("duration"), -1)
        );
        record!("time-unknown", call!("current", s!("elapsed"), s!("text")));
        record!(
            "known",
            call!("setCurrent", s!("player"), s!("duration"), 125_000)
        );
        record!("time-known", call!("current", s!("elapsed"), s!("text")));
        // Turned to landscape on the video.
        record!("turn", call!("turn", 960, 540));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "turned-index",
            call!("get", s!("slides"), s!("currentIndex"))
        );
        record!("turned-shown", call!("page", s!("shownId")));
        record!("turned-standing", call!("standing"));
        record!("turned-slide", call!("current", s!(""), s!("width")));
        // Back upright, on to the picture.
        record!("turn-back", call!("turn", 540, 960));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "upright-index",
            call!("get", s!("slides"), s!("currentIndex"))
        );
        record!("upright-standing", call!("standing"));
        record!("swipe-picture", call!("swipeTo", 0));
        record!("chrome-before", call!("chrome"));
        record!("tap", call!("tapCurrent"));
        // Tap told apart from double tap only once a second can't follow.
        record!("chrome-tapped-at-once", call!("chrome"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("chrome-tapped", call!("chrome"));
        // Video slide: tap plays, so header there whatever.
        record!("swipe-video-hidden", call!("swipeTo", 1));
        record!("chrome-on-video", call!("chrome"));
        record!("swipe-picture-hidden", call!("swipeTo", 0));
        record!("chrome-back-on-picture", call!("chrome"));
        record!("tap-again", call!("tapCurrent"));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("chrome-tapped-again", call!("chrome"));
        record!("double-tap", call!("doubleTapCurrent"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("chrome-double-tapped", call!("chrome"));
        // Video alone: neither row nor player knows length.
        record!("load-video", call!("loadVideo", s!(video.clone()), 0));
        record!("nothing-known", call!("get", s!("elapsed"), s!("text")));
        record!("from-row", call!("setItem", s!("durationMs"), 65_000));
        record!("from-row-time", call!("get", s!("elapsed"), s!("text")));
        record!(
            "from-row-seek",
            call!("get", s!("seek"), s!("maximumValue"))
        );
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps.iter().find(|(name, _)| *name == label).map_or_else(
            || "<step did not run>".to_string(),
            |(_, value)| value.clone(),
        )
    };
    let context = format!("steps: {steps:?}");

    for label in [
        "load",
        "tap-back",
        "swipe-video",
        "unknown",
        "known",
        "turn",
        "turn-back",
        "swipe-picture",
        "tap",
        "swipe-video-hidden",
        "swipe-picture-hidden",
        "tap-again",
        "load-video",
        "from-row",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    // Day in phone's short format: not this test's to spell.
    let said = value("sender-day");
    assert!(
        said.strip_prefix("Ada Lovelace, ")
            .is_some_and(|day| !day.is_empty() && !day.contains(',')),
        "the header's second line does not name who sent the picture, then the day: \
         {said:?}. {context}"
    );

    for (label, expected, complaint) in [
        (
            "allowed",
            "15",
            "the viewer stays upright when the phone turns: pictures never turn",
        ),
        ("back-label", "Back", "the header has no Back top left"),
        ("popped", "1", "tapping Back does not go back"),
        (
            "video-turned-by-screen",
            "false",
            "the video turns with the screen inside the page that turns too: sideways or \
             distorted",
        ),
        (
            "time-unknown",
            "0:00 / 0:07",
            "before the player has the file, the video's time is not the length its message \
             gives (Qt 5.6 says -1: \"0:00 / -1:0-1\")",
        ),
        (
            "time-known",
            "0:00 / 2:05",
            "once the player knows the length, the video's time does not say it",
        ),
        (
            "turned-index",
            "1",
            "turned, the viewer moved to another slide",
        ),
        (
            "turned-shown",
            "15",
            "turned, the viewer forgot what it showed",
        ),
        (
            "turned-standing",
            "960/960",
            "turned, the list stands between slides, or on another one",
        ),
        (
            "turned-slide",
            "960",
            "turned, the slide is not the screen's new width",
        ),
        ("upright-index", "1", "turned back, the viewer moved"),
        (
            "upright-standing",
            "540/540",
            "turned back, the list stands between slides",
        ),
        (
            "chrome-before",
            "true/true",
            "the header is not there to begin with",
        ),
        (
            "chrome-tapped-at-once",
            "true/true",
            "the header went at the first touch: a double tap to zoom would flash it",
        ),
        (
            "chrome-tapped",
            "false/false",
            "a single tap on the picture does not hide the whole header, back arrow too",
        ),
        (
            "chrome-on-video",
            "true/true",
            "a video slide (tap plays it) lost its header with no tap to bring it back",
        ),
        (
            "chrome-back-on-picture",
            "false/false",
            "swiped back to a picture, the hidden header came back on its own",
        ),
        (
            "chrome-tapped-again",
            "true/true",
            "a second tap does not bring the header back",
        ),
        ("double-tap", "true", "a double tap does not zoom"),
        (
            "chrome-double-tapped",
            "true/true",
            "a double tap to zoom also hid the header",
        ),
        (
            "nothing-known",
            "",
            "a video whose length nobody knows shows a made-up time",
        ),
        (
            "from-row-time",
            "0:00 / 1:05",
            "a video's length from its message is not shown before the player knows it",
        ),
        (
            "from-row-seek",
            "65000",
            "the seek bar does not span the video's length from its message",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    let _ = std::fs::remove_dir_all(&temp);
}
