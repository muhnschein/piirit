//! Tap on a picture or video opens chat's whole gallery, swiped one per screen, at what was
//! tapped: from conversation (oldest left) and from contact/group gallery grid (newest left).
//!
//! Headless vs stub Silica, recording double; group seeded with picture (10) and video (15),
//! two more pictures arriving (first with its events lost). Swipe = list's current index moved,
//! as flick snapping does, or real flick watched while it runs. Checks: tapped one shown alone
//! until list has it, then list placed there; video waits for its play button (Jolla's), plays
//! only on screen, paused mid-swipe, keeps file till swipe over; playing full screen or zoomed
//! picture holds list still; restart of model, a delete and an arrival elsewhere leave reader
//! where they were (zoom too: slide not rebuilt); slide far off built only once on screen or list
//! still; list clipped (page slides out over chat); Jolla's chrome: up or down closes, no back
//! arrow, light on dark, controls up, sender and day, no pull-down; message not in gallery stays
//! alone.

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
        /// Page's Silica colours light on dark (black page, any ambience)?
        function lightOnDark() {
            return '' + (loader.item.palette.colorScheme === Theme.LightOnDark)
        }
        /// Fake core's sender, send time of every message but the first in phone's short
        /// format.
        function sentDay() {
            return 'Ada Lovelace, '
                   + Qt.formatDate(new Date(1700090000 * 1000), Qt.DefaultLocaleShortDate)
        }
        /// Play button on video now on screen (or alone, `seedVideo`) pressed.
        function play(name) {
            var view = name === 'seedVideo' ? findIn(loader.item, name)
                                            : findIn(slides().currentItem, 'videoView')
            var button = view ? findIn(view, 'playButton') : null
            if (!button) { return 'missing:playButton' }
            button.clicked()
            return 'ok'
        }
        function load(url, messageId, viewType, file, newestFirst, status) {
            loader.setSource('', {})
            loader.setSource(url, {
                accountId: 1,
                chatId: 2,
                messageId: messageId,
                viewType: viewType,
                fileUrl: file,
                fileName: 'tapped',
                previewWidth: 300,
                newestFirst: newestFirst,
                status: status
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
            // Loader's item is among its children; said here too for a
            // loader with nothing yet.
            if (node.item) { return findIn(node.item, name) }
            return null
        }
        function slides() { return findIn(loader.item, 'slides') }
        function page(property) { return '' + loader.item[property] }
        function setPage(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function list(property) { return '' + slides()[property] }
        /// Swipe, landed: list's current moves, view with it, as snapping on a flick does.
        function swipeTo(index) {
            slides().currentIndex = index
            slides().positionViewAtIndex(index, ListView.SnapPosition)
            return 'ok'
        }
        /// Swipe by rows: + rightward.
        function swipeBy(rows) {
            return swipeTo(slides().currentIndex + rows)
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        /// On the slide now on screen; '' for the slide itself.
        function current(name, property) {
            var slide = slides().currentItem
            if (!slide) { return 'no-current' }
            var item = name.length > 0 ? findIn(slide, name) : slide
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        /// On one message's slide.
        function onSlide(messageId, name, property) {
            var slide = findIn(slides(), 'slide' + messageId)
            if (!slide) { return 'missing:slide' + messageId }
            var item = findIn(slide, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        /// Double tap on the picture now on screen.
        function zoomCurrent() {
            var slide = slides().currentItem
            var view = slide ? findIn(slide, 'pictureView') : null
            if (!view) { return 'missing:pictureView' }
            view.toggleZoom(100, 100)
            return '' + view.zoomed
        }
        /// Real swipe: list flicked, snaps a slide on. Watched every few ms while it moves:
        /// what was current, whether video 15 played or lost its file before it settled.
        property bool watching: false
        property bool sawMoving: false
        property int movingCurrent: -1
        property bool playedMidSwipe: false
        property bool droppedMidSwipe: false
        /// Slide two over, watched: built only once list still or on screen.
        property string farName: ''
        property bool farSeenOffScreen: false
        property bool farBuiltOffScreen: false
        property bool farSeenOnScreen: false
        property bool farUnbuiltOnScreen: false
        function onScreen(list, item) {
            return item.x < list.contentX + list.width && item.x + item.width > list.contentX
        }
        function slideState(messageId) {
            var slide = findIn(slides(), 'slide' + messageId)
            if (!slide) { return 'none' }
            return findIn(slide, 'pictureView') || findIn(slide, 'videoView') ? 'built' : 'empty'
        }
        function atStart() {
            var list = slides()
            return '' + (!list.moving && Math.round(list.contentX) === Math.round(list.originX))
        }
        function flickFar(velocity, name) {
            farName = name
            farSeenOffScreen = false
            farBuiltOffScreen = false
            farSeenOnScreen = false
            farUnbuiltOnScreen = false
            sawMoving = false
            watching = true
            // Snapping flick aims at slides laid out at its start, re-aimed as more appear:
            // headless, landing hung on frame timing. Plain flick runs to list's start every
            // time; current stays put (no matter: building under test, not current).
            var list = slides()
            list.highlightRangeMode = ListView.NoHighlightRange
            list.snapMode = ListView.NoSnap
            list.maximumFlickVelocity = velocity
            // Brisk: stopped well inside the second between checks.
            list.flickDeceleration = 6000
            list.flick(velocity, 0)
            return 'ok'
        }
        function videoPlayer() {
            var slide = findIn(slides(), 'slide15')
            return slide ? findIn(slide, 'player') : null
        }
        function flickBy(velocity) {
            sawMoving = false
            movingCurrent = -1
            playedMidSwipe = false
            droppedMidSwipe = false
            watching = true
            // Headless Qt 5.15 won't flick() a SnapOneItem list at all; per-item snap moves
            // alike (two slides: lands one over). Motion is what's under test.
            slides().snapMode = ListView.SnapToItem
            // Brisk enough that Qt aims at a slide. Slower, it keeps aim never set (5.15 leaves
            // it uninitialised) and, by what memory held, bounced back after one frame.
            slides().flickDeceleration = 6000
            slides().flick(velocity, 0)
            return 'ok'
        }
        function watched(property) { return '' + probe[property] }
        Timer {
            interval: 5
            repeat: true
            running: probe.watching
            onTriggered: {
                var list = slides()
                if (!list.moving) {
                    if (probe.sawMoving) { probe.watching = false }
                    return
                }
                probe.sawMoving = true
                if (list.currentItem) { probe.movingCurrent = list.currentItem.messageId }
                var player = videoPlayer()
                if (player && player.playbackState === 1) { probe.playedMidSwipe = true }
                if (player && ('' + player.source).length === 0) {
                    probe.droppedMidSwipe = true
                }
                var far = probe.farName.length > 0 ? findIn(list, probe.farName) : null
                if (far) {
                    var built = findIn(far, 'pictureView') !== null
                    if (onScreen(list, far)) {
                        probe.farSeenOnScreen = true
                        if (!built) { probe.farUnbuiltOnScreen = true }
                    } else {
                        probe.farSeenOffScreen = true
                        if (built) { probe.farBuiltOffScreen = true }
                    }
                }
            }
        }
        function media() { return findIn(loader.item, 'media') }
        /// Core dropped events: model starts over.
        function overflow() {
            media().handle_event(1, 'EventChannelOverflow', '{}')
            return 'ok'
        }
        function remove(messageId) {
            media().delete_message(messageId)
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_tapped_picture_or_video_swipes_through_the_whole_gallery() {
    let temp = std::env::temp_dir().join(format!("piirit-gallery-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_MEDIA", "1");
        // First unannounced: only restart finds it. Three more while reader on grid's picture.
        // Counted from here, not from fake's start (which lags by a varying while).
        std::env::set_var("PIIRIT_FAKE_PICTURES", "5500:quiet,7300,7400,7500");
        std::env::set_var(
            "PIIRIT_FAKE_EPOCH_MS",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after 1970")
                .as_millis()
                .to_string(),
        );
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

    // From conversation: picture 10 tapped. Before ids: alone.
    let page = gallery.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load-chat",
            call!(
                "load",
                s!(page.clone()),
                10,
                s!("Image"),
                s!("file:///tmp/piirit-fake/photo.jpg"),
                false,
                2
            )
        );
        record!("chat-placed-early", call!("page", s!("placed")));
        record!(
            "chat-seed-early",
            call!("get", s!("seedPicture"), s!("fileUrl"))
        );
        record!(
            "chat-seed-preview",
            call!("get", s!("seedPicture"), s!("previewWidth"))
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("chat-placed", call!("page", s!("placed")));
        record!("chat-seed-gone", call!("get", s!("seed"), s!("active")));
        record!("chat-count", call!("list", s!("count")));
        // Always left to right: right-to-left list broke back transition.
        record!(
            "chat-direction",
            call!("list", s!("effectiveLayoutDirection"))
        );
        // Oldest first, as in chat: picture 10 leftmost, video 15 right of it.
        record!("chat-index", call!("list", s!("currentIndex")));
        record!("chat-current", call!("current", s!(""), s!("messageId")));
        record!(
            "chat-current-file",
            call!("current", s!("pictureView"), s!("fileUrl"))
        );
        record!(
            "chat-current-preview",
            call!("current", s!("pictureView"), s!("previewWidth"))
        );
        // Neighbours beside screen don't draw over what's under page as it slides out.
        record!("chat-clipped", call!("list", s!("clip")));
        // Neighbour built while list still: ready before swiped to.
        record!("neighbour-built", call!("slideState", 15));
        // Jolla's: up or down closes (no back arrow), light on dark, controls up, sender
        // and day; no pull-down (drag down is the page's).
        record!("light-on-dark", call!("lightOnDark"));
        record!("controls", call!("get", s!("overlay"), s!("shown")));
        record!("caption", call!("get", s!("caption"), s!("text")));
        record!("caption-expected", call!("sentDay"));
        record!("back-swipe", call!("page", s!("backNavigation")));
        record!("back-way", call!("page", s!("navigationStyle")));
        record!("back-arrow", call!("page", s!("showNavigationIndicator")));
        record!(
            "picture-menu",
            call!("current", s!("pictureMenu"), s!("visible"))
        );
        record!(
            "picture-drags",
            call!("current", s!("pictureFlick"), s!("interactive"))
        );
        // Swipe to newer: the video, waiting for its play button.
        record!("swipe-newer", call!("swipeTo", 1));
        record!("shown-after-swipe", call!("page", s!("shownId")));
        record!("video-file", call!("current", s!("player"), s!("source")));
        record!(
            "video-state",
            call!("current", s!("player"), s!("playbackState"))
        );
        record!(
            "video-poster",
            call!("current", s!("poster"), s!("visible"))
        );
        record!("video-seek", call!("get", s!("seek"), s!("visible")));
        // Play: file in, controls away, list holds still; tap brings them, list swipes.
        record!("play", call!("play", s!("")));
        record!("played-file", call!("current", s!("player"), s!("source")));
        record!(
            "played-state",
            call!("current", s!("player"), s!("playbackState"))
        );
        record!("played-controls", call!("get", s!("overlay"), s!("shown")));
        record!("played-swipeable", call!("list", s!("interactive")));
        record!("show-controls", call!("setPage", s!("controlsShown"), true));
        record!("shown-swipeable", call!("list", s!("interactive")));
        record!(
            "picture-off-screen",
            call!("onSlide", 10, s!("pictureView"), s!("active"))
        );
        // Back: video stops, lets go of file.
        record!("swipe-back", call!("swipeTo", 0));
        record!(
            "video-left-state",
            call!("onSlide", 15, s!("player"), s!("playbackState"))
        );
        record!(
            "video-left-file",
            call!("onSlide", 15, s!("player"), s!("source"))
        );
        // Zoomed: list holds still for pan; out again, swipes again.
        record!("zoom-in", call!("zoomCurrent"));
        record!("zoomed-swipeable", call!("list", s!("interactive")));
        record!(
            "zoomed-drags",
            call!("current", s!("pictureFlick"), s!("interactive"))
        );
        record!("zoomed-controls", call!("get", s!("overlay"), s!("shown")));
        record!("zoom-out", call!("zoomCurrent"));
        record!("fitted-swipeable", call!("list", s!("interactive")));
        record!("fitted-controls", call!("get", s!("overlay"), s!("shown")));
        // Finger flicks on to the video: nothing starts while list moves.
        record!("flick-on", call!("flickBy", -2000.0));
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("flick-on-moved", call!("watched", s!("sawMoving")));
        record!("flick-on-current", call!("watched", s!("movingCurrent")));
        record!("flick-on-played", call!("watched", s!("playedMidSwipe")));
        record!("flick-on-at", call!("current", s!(""), s!("messageId")));
        record!(
            "flick-on-settled",
            call!("current", s!("player"), s!("playbackState"))
        );
        record!(
            "flick-on-poster",
            call!("current", s!("poster"), s!("visible"))
        );
        // Played, controls brought back; off again: video paused, keeps file till list stops.
        record!("play-on", call!("play", s!("")));
        record!(
            "show-controls-on",
            call!("setPage", s!("controlsShown"), true)
        );
        record!(
            "playing-on",
            call!("current", s!("player"), s!("playbackState"))
        );
        record!("flick-off", call!("flickBy", 2000.0));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("flick-off-moved", call!("watched", s!("sawMoving")));
        record!("flick-off-played", call!("watched", s!("playedMidSwipe")));
        record!("flick-off-dropped", call!("watched", s!("droppedMidSwipe")));
        record!("flick-off-at", call!("current", s!(""), s!("messageId")));
        record!(
            "flick-off-state",
            call!("onSlide", 15, s!("player"), s!("playbackState"))
        );
        record!(
            "flick-off-file",
            call!("onSlide", 15, s!("player"), s!("source"))
        );
        // Player let go: platform's still frame stands in, not black.
        record!(
            "flick-off-poster",
            call!("onSlide", 15, s!("poster"), s!("source"))
        );
    });

    // From grid (newest first): video 15 tapped, page still arriving.
    let page = gallery.clone();
    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "load-grid",
            call!(
                "load",
                s!(page.clone()),
                15,
                s!("Video"),
                s!("file:///tmp/piirit-fake/clip.mp4"),
                true,
                1
            )
        );
        record!(
            "grid-seed-state",
            call!("get", s!("seedVideo"), s!("playing"))
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("grid-placed", call!("page", s!("placed")));
        record!("grid-index", call!("list", s!("currentIndex")));
        record!(
            "grid-video-arriving",
            call!("current", s!("player"), s!("playbackState"))
        );
        record!("arrived", call!("setPage", s!("status"), 2));
        record!(
            "grid-video-arrived",
            call!("current", s!("player"), s!("playbackState"))
        );
        // Older: the picture. Model starts over, finds unannounced picture above.
        record!("grid-swipe", call!("swipeBy", 1));
        record!("overflow", call!("overflow"));
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("restart-current", call!("current", s!(""), s!("messageId")));
        record!("restart-shown", call!("page", s!("shownId")));
        record!("restart-index", call!("list", s!("currentIndex")));
        // Zoomed, then the video deleted, a picture arrives.
        record!("grid-zoom", call!("zoomCurrent"));
        record!("delete-video", call!("remove", 15));
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("after-delete-count", call!("list", s!("count")));
        record!("after-delete-index", call!("list", s!("currentIndex")));
        record!(
            "after-delete-current",
            call!("current", s!(""), s!("messageId"))
        );
        record!(
            "after-delete-zoomed",
            call!("current", s!("pictureView"), s!("zoomed"))
        );
        // Out again; flick toward newest, about a slide and a third: slide three over enters
        // cache as list moves, stays off screen.
        record!("unzoom", call!("zoomCurrent"));
        record!("near-before", call!("slideState", 103));
        record!("flick-near", call!("flickFar", 2900.0, s!("slide103")));
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!("near-moved", call!("watched", s!("sawMoving")));
        record!("near-seen-off", call!("watched", s!("farSeenOffScreen")));
        record!("near-built-off", call!("watched", s!("farBuiltOffScreen")));
        record!("near-seen-on", call!("watched", s!("farSeenOnScreen")));
        record!("near-after", call!("slideState", 103));
        // On to newest: slide there enters cache mid-flick, then comes on screen.
        record!("far-before", call!("slideState", 104));
        record!("flick-far", call!("flickFar", 8000.0, s!("slide104")));
    });

    let page = gallery.clone();
    let gallery_again = gallery;
    single_shot(Duration::from_secs(10), move || unsafe {
        record!("far-moved", call!("watched", s!("sawMoving")));
        record!("far-seen-off", call!("watched", s!("farSeenOffScreen")));
        record!("far-built-off", call!("watched", s!("farBuiltOffScreen")));
        record!("far-seen-on", call!("watched", s!("farSeenOnScreen")));
        record!("far-unbuilt-on", call!("watched", s!("farUnbuiltOnScreen")));
        record!("far-at-start", call!("atStart"));
        record!("far-after", call!("slideState", 104));
        // Not in gallery (sticker): alone, nothing to swipe.
        record!(
            "load-sticker",
            call!(
                "load",
                s!(page.clone()),
                99,
                s!("Sticker"),
                s!("file:///tmp/piirit-fake/sticker.webp"),
                false,
                2
            )
        );
    });

    let page = gallery_again;
    single_shot(Duration::from_secs(11), move || unsafe {
        record!("sticker-placed", call!("page", s!("placed")));
        record!(
            "sticker-seed",
            call!("get", s!("seedPicture"), s!("fileUrl"))
        );
        record!("sticker-list", call!("list", s!("visible")));
        record!("sticker-caption", call!("get", s!("caption"), s!("text")));
        record!("sticker-bar", call!("get", s!("iconBar"), s!("visible")));
        record!(
            "sticker-menu",
            call!("get", s!("pictureMenu"), s!("visible"))
        );
        // Video not in gallery (deleted above): alone, waits for play; overlay's seek bar its.
        record!(
            "load-lone-video",
            call!(
                "load",
                s!(page.clone()),
                15,
                s!("Video"),
                s!("file:///tmp/piirit-fake/clip.mp4"),
                false,
                2
            )
        );
        record!("lone-video", call!("get", s!("seedVideo"), s!("playing")));
        record!("lone-seek", call!("get", s!("seek"), s!("visible")));
        record!("lone-play", call!("play", s!("seedVideo")));
        record!("lone-played", call!("get", s!("seedVideo"), s!("playing")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let methods = common::methods(&journal);
    assert_outcome(&steps, &methods);
    let _ = std::fs::remove_dir_all(&temp);
}

/// What run must show.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)], methods: &[String]) {
    let value = |label: &str| {
        steps.iter().find(|(name, _)| *name == label).map_or_else(
            || "<step did not run>".to_string(),
            |(_, value)| value.clone(),
        )
    };
    let context = format!("steps: {steps:?}");

    for label in [
        "load-chat",
        "swipe-newer",
        "swipe-back",
        "play",
        "show-controls",
        "flick-on",
        "play-on",
        "show-controls-on",
        "flick-off",
        "load-grid",
        "arrived",
        "grid-swipe",
        "overflow",
        "delete-video",
        "flick-near",
        "flick-far",
        "load-sticker",
        "load-lone-video",
        "lone-play",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    for (label, expected, complaint) in [
        // Tapped one at once, at row's decode size.
        (
            "chat-placed-early",
            "false",
            "the list was shown before it knew where the tapped picture is",
        ),
        (
            "chat-seed-early",
            "file:///tmp/piirit-fake/photo.jpg",
            "the tapped picture was not shown while the gallery loaded",
        ),
        (
            "chat-seed-preview",
            "300",
            "the tapped picture was not drawn at the width the row drew \
             it, so it decodes again behind the transition",
        ),
        // Then list, at tapped one, whole gallery.
        ("chat-placed", "true", "the gallery never took over"),
        // Back: page slides out over chat, nothing of gallery beside it.
        (
            "chat-clipped",
            "true",
            "the slides beside the screen are drawn outside it: on Back the \
             neighbouring picture slides in over the chat, then cuts",
        ),
        // Jolla's chrome.
        (
            "light-on-dark",
            "true",
            "the viewer's icons take the ambience's colours: on the black \
             page a light ambience's dark ones vanish",
        ),
        (
            "controls",
            "true",
            "the viewer opens with its controls away",
        ),
        (
            "back-swipe",
            "true",
            "the viewer cannot be swiped away at all",
        ),
        (
            "back-way",
            "1",
            "the viewer does not close on a swipe up or down (Jolla's): \
             sideways swipes are the gallery's, so there is no way back",
        ),
        (
            "back-arrow",
            "false",
            "Silica's back arrow shows on the viewer, beside its close button",
        ),
        (
            "picture-menu",
            "false",
            "the picture has a pull-down menu in the viewer, which the swipe \
             down to close fights with",
        ),
        (
            "picture-drags",
            "false",
            "a fitted picture takes drags, so the swipe down to close never \
             reaches the page",
        ),
        (
            "neighbour-built",
            "built",
            "the slide beside the tapped one was not built while the list \
             stood still, so it is built mid-swipe",
        ),
        (
            "chat-seed-gone",
            "false",
            "the lone picture stayed once the gallery had it",
        ),
        (
            "chat-count",
            "2",
            "the gallery does not hold every picture and video of the chat",
        ),
        (
            "chat-direction",
            "0",
            "the gallery is laid out right to left, which breaks the page's \
             back transition into the chat",
        ),
        (
            "chat-index",
            "0",
            "from the conversation, the oldest picture is not leftmost, as \
             it is topmost in the chat",
        ),
        (
            "chat-current",
            "10",
            "the gallery did not open at the tapped picture",
        ),
        (
            "chat-current-file",
            "file:///tmp/piirit-fake/photo.jpg",
            "the slide does not show the tapped picture's file",
        ),
        (
            "chat-current-preview",
            "300",
            "the tapped picture's slide does not reuse the row's decode",
        ),
        // Swipe: video on screen waits for play; picture off screen idles.
        (
            "shown-after-swipe",
            "15",
            "the page lost track of what is on screen after a swipe",
        ),
        (
            "video-file",
            "",
            "swiping to the video loaded it before its play button was pressed",
        ),
        (
            "video-state",
            "0",
            "the video swiped to played before its play button was pressed \
             (Jolla's waits)",
        ),
        (
            "video-poster",
            "true",
            "the video waiting for play shows black, not its still frame",
        ),
        (
            "video-seek",
            "true",
            "the video's controls have no seek bar",
        ),
        (
            "played-file",
            "file:///tmp/piirit-fake/clip.mp4",
            "pressing play did not give the player its file",
        ),
        ("played-state", "1", "pressing play did not play the video"),
        (
            "played-controls",
            "false",
            "the controls stay over the video once it plays (Jolla's goes \
             full screen)",
        ),
        (
            "played-swipeable",
            "false",
            "a video playing full screen swipes away under a stray drag",
        ),
        (
            "shown-swipeable",
            "true",
            "with the controls back over the playing video, the gallery \
             cannot be swiped",
        ),
        (
            "picture-off-screen",
            "false",
            "the picture swiped away from still counts as on screen \
             (a GIF there keeps decoding)",
        ),
        // Away: stopped, file let go.
        (
            "video-left-state",
            "0",
            "the video kept playing after it was swiped away",
        ),
        (
            "video-left-file",
            "",
            "the video swiped away from kept its file open",
        ),
        // Zoom: pan, not swipe.
        ("zoom-in", "true", "a double tap did not zoom the slide in"),
        (
            "zoomed-swipeable",
            "false",
            "a zoomed picture cannot be panned: dragging swipes away",
        ),
        (
            "zoomed-drags",
            "true",
            "a zoomed picture does not take drags, so it cannot be panned",
        ),
        (
            "zoomed-controls",
            "false",
            "the controls stay over a zoomed picture (Jolla's puts them away)",
        ),
        (
            "fitted-controls",
            "true",
            "the controls did not come back once the picture zoomed out",
        ),
        ("zoom-out", "false", "a second double tap did not zoom out"),
        (
            "fitted-swipeable",
            "true",
            "the gallery cannot be swiped after zooming out",
        ),
        // Real flick on: video current mid-swipe, never starts on its own.
        (
            "flick-on-moved",
            "true",
            "the flick did not move the gallery",
        ),
        (
            "flick-on-current",
            "15",
            "the video never became current while the flick ran",
        ),
        (
            "flick-on-played",
            "false",
            "the video started while the swipe was still running (its \
             pipeline setup stalls the swipe)",
        ),
        (
            "playing-on",
            "1",
            "the video flicked to did not play when its button was pressed",
        ),
        ("flick-on-at", "15", "the flick did not land on the video"),
        (
            "flick-on-settled",
            "0",
            "the video played once the swipe was over, before its play \
             button was pressed",
        ),
        // Flick off: paused, file kept till still; then let go.
        (
            "flick-off-moved",
            "true",
            "the flick back did not move the gallery",
        ),
        (
            "flick-off-played",
            "false",
            "the video kept playing while swiped away",
        ),
        (
            "flick-off-dropped",
            "false",
            "the video let go of its file while the swipe was still running \
             (pipeline teardown stalls the swipe)",
        ),
        (
            "flick-off-at",
            "10",
            "the flick back did not land on the picture",
        ),
        (
            "flick-off-state",
            "0",
            "the video swiped away from was not stopped once the swipe was over",
        ),
        (
            "flick-off-file",
            "",
            "the video swiped away from kept its file once the swipe was over",
        ),
        (
            "flick-off-poster",
            "file:///tmp/piirit-fake/clip.mp4",
            "the video swiped away from shows black, not its still frame",
        ),
        (
            "flick-on-poster",
            "true",
            "the video flicked to shows black, not its still frame",
        ),
        // From grid: grid's order (newest left), at tapped video; plays once page in place.
        (
            "grid-seed-state",
            "false",
            "the tapped video played during the page transition",
        ),
        (
            "grid-placed",
            "true",
            "the gallery from the grid never took over",
        ),
        (
            "grid-index",
            "0",
            "from the grid, the gallery is not in the grid's order, newest \
             first",
        ),
        (
            "grid-video-arriving",
            "0",
            "the video played during the page transition",
        ),
        (
            "grid-video-arrived",
            "0",
            "the video played on its own once the page was in place (Jolla's \
             waits for play)",
        ),
        // Restart: back where reader was.
        (
            "restart-current",
            "10",
            "the gallery jumped to another picture when its list was read \
             again",
        ),
        ("restart-shown", "10", "the page forgot what was on screen"),
        (
            "restart-index",
            "2",
            "after the reread the reader's picture is not where the list \
             holds it",
        ),
        ("grid-zoom", "true", "the picture did not zoom in"),
        // Delete and arrival elsewhere: same picture, still zoomed (slide not rebuilt).
        (
            "after-delete-count",
            "5",
            "the gallery did not lose the deleted video and gain the new pictures",
        ),
        (
            "after-delete-index",
            "4",
            "the reader's picture is not where the list now holds it",
        ),
        (
            "after-delete-current",
            "10",
            "deleting or receiving another message moved the reader off their picture",
        ),
        (
            "after-delete-zoomed",
            "true",
            "deleting or receiving another message rebuilt the reader's \
             slide and threw away their zoom",
        ),
        // Flicks: slide far off built only once list still, or on screen.
        ("unzoom", "false", "the picture did not zoom out"),
        (
            "near-before",
            "none",
            "the slide three over already existed, so the flick tests nothing",
        ),
        (
            "near-moved",
            "true",
            "the short flick did not move the gallery",
        ),
        (
            "near-seen-off",
            "true",
            "the slide three over never came into the list mid-flick, so the \
             flick tests nothing",
        ),
        (
            "near-built-off",
            "false",
            "a slide off screen was built while the list still moved (its \
             decode and upload drop the swipe's last frames)",
        ),
        (
            "near-seen-on",
            "false",
            "the short flick went further than meant, so it tests nothing",
        ),
        (
            "near-after",
            "built",
            "a slide that came into the list mid-swipe was not built once the \
             list stopped, so it is built as it is swiped to",
        ),
        (
            "far-before",
            "none",
            "the newest slide already existed, so the long flick tests nothing",
        ),
        (
            "far-moved",
            "true",
            "the long flick did not move the gallery",
        ),
        (
            "far-seen-off",
            "true",
            "the newest slide never came into the list off screen mid-flick, \
             so the flick tests nothing",
        ),
        (
            "far-built-off",
            "false",
            "a slide off screen was built while the list still moved (its \
             decode and upload drop the swipe's last frames)",
        ),
        (
            "far-seen-on",
            "true",
            "the long flick never brought the far slide on screen",
        ),
        (
            "far-unbuilt-on",
            "false",
            "a slide swiped onto the screen was left empty until the list stopped",
        ),
        (
            "far-at-start",
            "true",
            "the long flick did not run to the newest picture",
        ),
        (
            "far-after",
            "built",
            "the slide was not built once the list stopped",
        ),
        // Not in gallery: alone.
        (
            "sticker-placed",
            "false",
            "a message the gallery does not hold was swapped for something \
             else",
        ),
        (
            "sticker-seed",
            "file:///tmp/piirit-fake/sticker.webp",
            "a message the gallery does not hold was not shown",
        ),
        (
            "sticker-list",
            "false",
            "a message the gallery does not hold was put among the gallery's",
        ),
        (
            "sticker-caption",
            "",
            "a message the gallery does not hold got a sender and day from \
             another",
        ),
        (
            "sticker-bar",
            "true",
            "a lone picture offers no Share and Save",
        ),
        (
            "sticker-menu",
            "false",
            "a lone picture has a pull-down menu, which the swipe down to \
             close fights with",
        ),
        (
            "lone-video",
            "false",
            "a lone video played before its play button was pressed",
        ),
        (
            "lone-seek",
            "true",
            "a lone video's controls have no seek bar",
        ),
        (
            "lone-played",
            "true",
            "a lone video's play button does not play it",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    // Day sent, phone's short format.
    assert!(
        !value("caption-expected").is_empty(),
        "the probe could not format the day. {context}"
    );
    assert_eq!(
        value("caption"),
        value("caption-expected"),
        "the viewer does not say who sent the picture and the day, in the \
         phone's date format. {context}"
    );

    // Gallery read from core, chat's pictures, GIFs, videos.
    assert!(
        methods.iter().any(|method| method == "get_chat_media"),
        "the gallery never asked the core for the chat's media: {methods:?}"
    );
}
