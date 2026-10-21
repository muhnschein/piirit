//! Viewer turned and tapped, as Jolla Gallery's: page follows phone (landscape too), list keeps
//! slide on screen; video turned by page only (screen-following turn doubled it: sideways,
//! distorted); up or down closes, no back arrow; top line names sender with day, X pops page;
//! single tap on picture fades controls, again brings them, swipes keep them as they are; double
//! tap zooms and puts them away, tap on zoomed one zooms out; video: seek bar spans row's length
//! until player knows it (Qt 5.6's -1 read "-1:0-1" mid-swipe), double tap 10 s on (controls
//! stay), play puts controls away and holds list still, tap brings them, they go again after a
//! while unless seek bar held, seek bar follows video but not under finger and seeks on release;
//! Share hands sheet file (video paused); Save copies; lone video seeks once length known.
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
        // Where the platform says downloads go: this test's own directory.
        function setDownloads(folder) { StandardPaths.download = folder; return 'ok' }
        function load(url, messageId, viewType, file) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: 1, chatId: 2, messageId: messageId, viewType: viewType,
                fileUrl: file, fileName: 'tapped.png', newestFirst: false, status: 2
            })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadVideo(url, durationMs) {
            loader.setSource('', {})
            loader.setSource(url, {
                width: 540, height: 960, durationMs: durationMs,
                fileUrl: 'file:///tmp/piirit-fake/clip.mp4'
            })
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
        function set(name, property, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item[property] = value
            return 'ok'
        }
        function setItem(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        /// Function of what's loaded (lone video), with one number.
        function callItem(name, value) {
            loader.item[name](value)
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
        /// Button pressed: overlay's, or (`current`) on slide on screen.
        function click(name, onCurrent) {
            var item = onCurrent ? findIn(slides().currentItem, name)
                                 : findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// Single tap on what's on screen (picture or video); double: tap, then second within
        /// interval.
        function tapCurrent(name) {
            var tap = findIn(slides().currentItem, name)
            if (!tap) { return 'missing:' + name }
            tap.clicked(null)
            return 'ok'
        }
        function doubleTapPicture() {
            var slide = slides().currentItem
            var tap = findIn(slide, 'pictureTap')
            var view = findIn(slide, 'pictureView')
            if (!tap || !view) { return 'missing:pictureTap' }
            tap.clicked(null)
            view.zoomTap(100, 100)
            return '' + view.zoomed
        }
        function doubleTapVideo() {
            var tap = findIn(slides().currentItem, 'videoTap')
            if (!tap) { return 'missing:videoTap' }
            tap.clicked(null)
            tap.doubleClicked(null)
            return 'ok'
        }
        function controls() { return get('overlay', 'shown') }
        /// Seek bar let go: Silica says released while still down, then not down.
        function releaseSeek() {
            var seek = findIn(loader.item, 'seek')
            if (!seek) { return 'missing:seek' }
            seek.released()
            seek.down = false
            return 'ok'
        }
        /// What share sheet was handed: times, kind, files.
        function shared() {
            var action = findIn(loader.item, 'shareAction')
            if (!action) { return 'missing:shareAction' }
            return action.triggered + '|' + action.mimeType + '|' + action.resources.join(',')
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_viewer_turns_with_the_phone_and_a_tap_fades_its_controls() {
    let temp = std::env::temp_dir().join(format!("piirit-gallery-turn-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // Lone picture's file, real: Save copies it.
    let png = temp.join("tapped.png");
    std::fs::write(&png, b"not really a png").expect("write picture");
    let png_url = format!("file://{}", png.display());
    let downloads = temp.join("Downloads");

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

    let page = gallery.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                s!(page.clone()),
                10,
                s!("Image"),
                s!("file:///tmp/piirit-fake/photo.jpg")
            )
        );
    });

    // Jolla's chrome: up or down closes, X, sender and day. Video: page turns it; seek bar's
    // length from row mid-swipe.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("allowed", call!("page", s!("allowedOrientations")));
        record!("back-way", call!("page", s!("navigationStyle")));
        record!("back-arrow", call!("page", s!("showNavigationIndicator")));
        record!("caption", call!("get", s!("caption"), s!("text")));
        record!("close", call!("click", s!("closeButton"), false));
        record!("popped", call!("pops"));
        record!("picture-seek", call!("get", s!("seek"), s!("visible")));
        record!("swipe-video", call!("swipeTo", 1));
        record!(
            "video-turned-by-screen",
            call!("current", s!("videoOutput"), s!("autoOrientation"))
        );
        record!("video-seek", call!("get", s!("seek"), s!("visible")));
        // Qt 5.6's player before it has file: -1.
        record!(
            "unknown",
            call!("setCurrent", s!("player"), s!("duration"), -1)
        );
        record!(
            "length-unknown",
            call!("get", s!("seek"), s!("maximumValue"))
        );
        record!("time-unknown", call!("get", s!("seek"), s!("valueText")));
        record!(
            "known",
            call!("setCurrent", s!("player"), s!("duration"), 125_000)
        );
        record!("length-known", call!("get", s!("seek"), s!("maximumValue")));
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
        record!("controls-before", call!("controls"));
        record!(
            "hide-on-picture",
            call!("get", s!("hideControls"), s!("running"))
        );
        record!("tap", call!("tapCurrent", s!("pictureTap")));
        // Tap told apart from double tap only once a second can't follow.
        record!("controls-tapped-at-once", call!("controls"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("controls-tapped", call!("controls"));
        record!("tappable", call!("get", s!("overlay"), s!("enabled")));
        // Swipes keep them as they are; video's play button stays (not playing).
        record!("swipe-video-hidden", call!("swipeTo", 1));
        record!("controls-on-video", call!("controls"));
        record!(
            "play-on-hidden",
            call!("current", s!("playButton"), s!("enabled"))
        );
        record!("swipe-picture-hidden", call!("swipeTo", 0));
        record!("controls-back-on-picture", call!("controls"));
        record!("tap-again", call!("tapCurrent", s!("pictureTap")));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("controls-tapped-again", call!("controls"));
        record!("double-tap", call!("doubleTapPicture"));
        record!("controls-zoomed", call!("controls"));
        record!("tap-zoomed", call!("tapCurrent", s!("pictureTap")));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!(
            "zoomed-after-tap",
            call!("current", s!("pictureView"), s!("zoomed"))
        );
        record!("controls-zoomed-out", call!("controls"));
        // Video: double tap 10 s on.
        record!("swipe-video-again", call!("swipeTo", 1));
        record!("double-tap-video", call!("doubleTapVideo"));
        record!(
            "double-tapped-at",
            call!("current", s!("player"), s!("position"))
        );
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("controls-double-tapped", call!("controls"));
        // Play: controls away, list still, play button gone.
        record!("play", call!("click", s!("playButton"), true));
        record!(
            "playing",
            call!("current", s!("player"), s!("playbackState"))
        );
        record!("controls-playing", call!("controls"));
        record!(
            "swipeable-playing",
            call!("get", s!("slides"), s!("interactive"))
        );
        record!(
            "play-button-playing",
            call!("current", s!("playButton"), s!("enabled"))
        );
        record!("tap-video", call!("tapCurrent", s!("videoTap")));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("controls-video-tapped", call!("controls"));
        record!(
            "swipeable-tapped",
            call!("get", s!("slides"), s!("interactive"))
        );
        record!(
            "pause-button",
            call!("current", s!("playButton"), s!("enabled"))
        );
        record!(
            "hide-running",
            call!("get", s!("hideControls"), s!("running"))
        );
        // Seek bar follows video...
        record!(
            "position",
            call!("setCurrent", s!("player"), s!("position"), 30_000)
        );
        record!("followed", call!("get", s!("seek"), s!("value")));
        // ...not under finger; controls stay meanwhile.
        record!("hold", call!("set", s!("seek"), s!("down"), true));
        record!(
            "hide-running-held",
            call!("get", s!("hideControls"), s!("running"))
        );
        record!("drag", call!("set", s!("seek"), s!("value"), 60_000));
        record!(
            "moved-while-held",
            call!("setCurrent", s!("player"), s!("position"), 31_000)
        );
        record!("held-value", call!("get", s!("seek"), s!("value")));
        record!("held-text", call!("get", s!("seek"), s!("valueText")));
        record!("release", call!("releaseSeek"));
        record!(
            "released-at",
            call!("current", s!("player"), s!("position"))
        );
        record!(
            "hide-running-released",
            call!("get", s!("hideControls"), s!("running"))
        );
        // Brisk, not 4 s of test: restarts.
        record!(
            "hide-soon",
            call!("set", s!("hideControls"), s!("interval"), 300)
        );
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!("controls-timed-out", call!("controls"));
        record!(
            "hide-later",
            call!("set", s!("hideControls"), s!("interval"), 4000)
        );
        record!("tap-video-again", call!("tapCurrent", s!("videoTap")));
    });

    let lone = video;
    let page = gallery;
    let picture = png_url;
    let downloads_for_probe = downloads.to_string_lossy().into_owned();
    single_shot(Duration::from_secs(11), move || unsafe {
        record!("controls-back", call!("controls"));
        // Share: sheet gets video's file; video waits.
        record!(
            "share-offered",
            call!("get", s!("shareButton"), s!("visible"))
        );
        record!("share", call!("click", s!("shareButton"), false));
        record!("shared", call!("shared"));
        record!(
            "shared-state",
            call!("current", s!("player"), s!("playbackState"))
        );
        // Lone video: length from row; seek before player knows length waits for it.
        record!("load-lone", call!("loadVideo", s!(lone.clone()), 0));
        record!("lone-length", call!("page", s!("length")));
        record!("lone-from-row", call!("setItem", s!("durationMs"), 65_000));
        record!("lone-row-length", call!("page", s!("length")));
        record!("lone-seek", call!("callItem", s!("seekTo"), 5000));
        record!("lone-pending", call!("page", s!("pendingSeek")));
        record!(
            "lone-paused",
            call!("get", s!("player"), s!("playbackState"))
        );
        record!("lone-loaded", call!("get", s!("player"), s!("source")));
        record!(
            "lone-learns",
            call!("set", s!("player"), s!("duration"), 30_000)
        );
        record!("lone-at", call!("get", s!("player"), s!("position")));
        record!("lone-done", call!("page", s!("pendingSeek")));
        // Save: copy of what's on screen (picture alone, not in gallery: own file).
        record!(
            "load-picture",
            call!(
                "load",
                s!(page.clone()),
                99,
                s!("Image"),
                s!(picture.clone())
            )
        );
        call!("setDownloads", s!(downloads_for_probe.clone()));
        record!("save", call!("click", s!("saveButton"), false));
    });

    single_shot(Duration::from_secs(12), move || unsafe {
        record!("saved", call!("get", s!("viewerNoticeLabel"), s!("text")));
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
        "close",
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
        "tap-zoomed",
        "swipe-video-again",
        "double-tap-video",
        "play",
        "tap-video",
        "position",
        "hold",
        "drag",
        "moved-while-held",
        "release",
        "hide-soon",
        "hide-later",
        "tap-video-again",
        "share",
        "load-lone",
        "lone-from-row",
        "lone-seek",
        "lone-learns",
        "load-picture",
        "save",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    // Day in phone's short format: not this test's to spell.
    let said = value("caption");
    assert!(
        said.strip_prefix("Ada Lovelace, ")
            .is_some_and(|day| !day.is_empty() && !day.contains(',')),
        "the viewer's top line does not name who sent the picture, then the day: \
         {said:?}. {context}"
    );

    for (label, expected, complaint) in [
        (
            "allowed",
            "15",
            "the viewer stays upright when the phone turns: pictures never turn",
        ),
        (
            "back-way",
            "1",
            "the viewer does not close on a swipe up or down, as Jolla's does",
        ),
        (
            "back-arrow",
            "false",
            "Silica's back arrow shows beside the viewer's close button",
        ),
        ("popped", "1", "the close button does not close the viewer"),
        ("picture-seek", "false", "a picture has a seek bar"),
        (
            "video-turned-by-screen",
            "false",
            "the video turns with the screen inside the page that turns too: sideways or \
             distorted",
        ),
        ("video-seek", "true", "the video has no seek bar"),
        (
            "length-unknown",
            "7000",
            "before the player has the file, the seek bar does not span the length the \
             video's message gives (Qt 5.6 says -1: \"-1:0-1\")",
        ),
        (
            "time-unknown",
            "0:00",
            "the seek bar does not say where the video is",
        ),
        (
            "length-known",
            "125000",
            "once the player knows the length, the seek bar does not span it",
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
            "controls-before",
            "true",
            "the controls are not there to begin with",
        ),
        (
            "hide-on-picture",
            "false",
            "the controls over a picture go away on their own (only a playing video's do)",
        ),
        (
            "controls-tapped-at-once",
            "true",
            "the controls went at the first touch: a double tap to zoom would flash them",
        ),
        (
            "controls-tapped",
            "false",
            "a single tap on the picture does not put the controls away",
        ),
        (
            "tappable",
            "false",
            "the controls put away still take taps meant for the picture",
        ),
        (
            "controls-on-video",
            "false",
            "swiping to a video brought the controls back on their own",
        ),
        (
            "play-on-hidden",
            "true",
            "with the controls away, a video not playing has no play button",
        ),
        (
            "controls-back-on-picture",
            "false",
            "swiped back to a picture, the controls came back on their own",
        ),
        (
            "controls-tapped-again",
            "true",
            "a second tap does not bring the controls back",
        ),
        ("double-tap", "true", "a double tap does not zoom"),
        (
            "controls-zoomed",
            "false",
            "zoomed in, the controls stay over the picture (Jolla's puts them away)",
        ),
        (
            "zoomed-after-tap",
            "false",
            "a tap on a zoomed picture does not zoom it out (Jolla's does)",
        ),
        (
            "controls-zoomed-out",
            "true",
            "zoomed out, the controls did not come back",
        ),
        (
            "double-tapped-at",
            "10000",
            "a double tap on the video does not skip 10 s on",
        ),
        (
            "controls-double-tapped",
            "true",
            "a double tap on the video also put the controls away",
        ),
        ("playing", "1", "the play button does not play the video"),
        (
            "controls-playing",
            "false",
            "the controls stay over the video once it plays (Jolla's goes full screen)",
        ),
        (
            "swipeable-playing",
            "false",
            "a video playing full screen swipes away under a stray drag",
        ),
        (
            "play-button-playing",
            "false",
            "the pause button stays over the video playing full screen",
        ),
        (
            "controls-video-tapped",
            "true",
            "a tap on the playing video does not bring the controls back",
        ),
        (
            "swipeable-tapped",
            "true",
            "with the controls back, the gallery cannot be swiped on",
        ),
        (
            "pause-button",
            "true",
            "with the controls back, the playing video has no pause button",
        ),
        (
            "hide-running",
            "true",
            "the controls brought back over a playing video never go away again",
        ),
        (
            "followed",
            "30000",
            "the seek bar does not follow the video",
        ),
        (
            "hide-running-held",
            "false",
            "the controls would go away while the reader holds the seek bar",
        ),
        (
            "held-value",
            "60000",
            "the video moved the seek bar out from under the reader's finger",
        ),
        (
            "held-text",
            "1:00",
            "the seek bar does not say where the reader is dragging to",
        ),
        (
            "released-at",
            "60000",
            "letting go of the seek bar does not move the video there",
        ),
        (
            "hide-running-released",
            "true",
            "once the seek bar is let go, the controls never go away again",
        ),
        (
            "controls-timed-out",
            "false",
            "the controls over a playing video do not go away after a while",
        ),
        (
            "controls-back",
            "true",
            "a tap does not bring the controls back after they went on their own",
        ),
        ("share-offered", "true", "the viewer offers no Share"),
        (
            "shared",
            "1|video/mp4|file:///tmp/piirit-fake/clip.mp4",
            "Share does not hand the share sheet the video's file and kind",
        ),
        (
            "shared-state",
            "2",
            "the video plays on under the share sheet (Jolla's pauses it)",
        ),
        (
            "lone-length",
            "0",
            "a video whose length nobody knows has a made-up length",
        ),
        (
            "lone-row-length",
            "65000",
            "a video's length from its message is not its length before the player knows it",
        ),
        (
            "lone-pending",
            "5000",
            "a seek before the player knows the video's length is lost",
        ),
        (
            "lone-paused",
            "2",
            "a seek on a video not yet loaded leaves it stopped, showing no frame",
        ),
        (
            "lone-loaded",
            "file:///tmp/piirit-fake/clip.mp4",
            "a seek on a video not yet loaded does not load it",
        ),
        (
            "lone-at",
            "5000",
            "once the player knows the length, the seek asked before is not done",
        ),
        (
            "lone-done",
            "-1",
            "the seek asked before is done again later",
        ),
        (
            "saved",
            "Saved to Downloads/Piirit",
            "Save does not say where the copy went",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    let copy = downloads.join("Piirit").join("tapped.png");
    assert_eq!(
        std::fs::read(&copy).ok().as_deref(),
        Some(&b"not really a png"[..]),
        "Save did not put a copy of what is on screen in Piirit's folder in Downloads: {}. \
         {context}",
        copy.display()
    );
    let _ = std::fs::remove_dir_all(&temp);
}
