//! Tap on a picture or video opens chat's whole gallery, swiped one per screen, at what was
//! tapped: from conversation (oldest left) and from contact/group gallery grid (newest left).
//!
//! Headless vs stub Silica, recording double; group seeded with picture (10) and video (15),
//! two more pictures arriving (first with its events lost). Swipe = list's current index moved,
//! as flick snapping does. Checks: tapped one shown alone until list has it, then list placed
//! there; video plays only on screen and once page in place; zoomed picture holds list still;
//! restart of model, a delete and an arrival elsewhere leave reader where they were (zoom too:
//! slide not rebuilt); message not in gallery stays alone.

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
        Loader { id: loader }
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
        /// Swipe: list's current moves, as snapping on a flick does.
        function swipeTo(index) {
            slides().currentIndex = index
            return 'ok'
        }
        /// Swipe by rows: + toward older.
        function swipeBy(rows) {
            slides().currentIndex = slides().currentIndex + rows
            return 'ok'
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
        // First unannounced: only restart finds it. Second while reader on grid's picture.
        std::env::set_var("PIIRIT_FAKE_PICTURES", "1500:quiet,4500");
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
        // Qt.RightToLeft: index 0 (newest) rightmost.
        record!("chat-direction", call!("list", s!("layoutDirection")));
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
        // Swipe to newer: the video, playing.
        record!("swipe-newer", call!("swipeTo", 0));
        record!("shown-after-swipe", call!("page", s!("shownId")));
        record!("video-file", call!("current", s!("player"), s!("source")));
        record!(
            "video-state",
            call!("current", s!("player"), s!("playbackState"))
        );
        record!(
            "picture-off-screen",
            call!("onSlide", 10, s!("pictureView"), s!("active"))
        );
        // Back: video stops, lets go of file.
        record!("swipe-back", call!("swipeTo", 1));
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
        record!("zoom-out", call!("zoomCurrent"));
        record!("fitted-swipeable", call!("list", s!("interactive")));
        // Model starts over, finds unannounced picture: rows below shift by one.
        record!("overflow", call!("overflow"));
    });

    let page = gallery.clone();
    single_shot(Duration::from_secs(3), move || unsafe {
        record!("restart-current", call!("current", s!(""), s!("messageId")));
        record!("restart-shown", call!("page", s!("shownId")));
        record!("restart-index", call!("list", s!("currentIndex")));
        // From grid (newest first, unannounced picture above): video 15 tapped, page still
        // arriving.
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

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("grid-placed", call!("page", s!("placed")));
        record!("grid-direction", call!("list", s!("layoutDirection")));
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
        // Older: the picture; zoomed, then the video deleted, a picture arrives.
        record!("grid-swipe", call!("swipeBy", 1));
        record!("grid-zoom", call!("zoomCurrent"));
        record!("delete-video", call!("remove", 15));
    });

    let page = gallery.clone();
    let gallery_again = gallery;
    single_shot(Duration::from_secs(5), move || unsafe {
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
    single_shot(Duration::from_secs(6), move || unsafe {
        record!("sticker-placed", call!("page", s!("placed")));
        record!(
            "sticker-seed",
            call!("get", s!("seedPicture"), s!("fileUrl"))
        );
        record!("sticker-list", call!("list", s!("visible")));
        // Video not in gallery (deleted above), page in place: plays alone, at once.
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
        "overflow",
        "load-grid",
        "arrived",
        "grid-swipe",
        "delete-video",
        "load-sticker",
        "load-lone-video",
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
            "1",
            "from the conversation, newer is not to the right as it is \
             below in the chat",
        ),
        (
            "chat-index",
            "1",
            "the gallery did not open at the tapped picture",
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
        // Swipe: video on screen plays; picture off screen idles.
        (
            "shown-after-swipe",
            "15",
            "the page lost track of what is on screen after a swipe",
        ),
        (
            "video-file",
            "file:///tmp/piirit-fake/clip.mp4",
            "swiping to the video did not give the player its file",
        ),
        ("video-state", "1", "the video swiped to did not play"),
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
        ("zoom-out", "false", "a second double tap did not zoom out"),
        (
            "fitted-swipeable",
            "true",
            "the gallery cannot be swiped after zooming out",
        ),
        // Restart: back where reader was.
        (
            "restart-current",
            "10",
            "the gallery jumped to the newest picture when its list was \
             read again",
        ),
        ("restart-shown", "10", "the page forgot what was on screen"),
        (
            "restart-index",
            "2",
            "after the reread the reader's picture is not where the list \
             holds it",
        ),
        // From grid: grid's order, at tapped video; plays once page in place.
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
            "grid-direction",
            "0",
            "from the grid, the gallery is not in the grid's order, \
             newest first",
        ),
        (
            "grid-index",
            "1",
            "the gallery from the grid did not open at the tapped video",
        ),
        (
            "grid-video-arriving",
            "0",
            "the video played during the page transition",
        ),
        (
            "grid-video-arrived",
            "1",
            "the video did not play once the page was in place",
        ),
        ("grid-zoom", "true", "the picture did not zoom in"),
        // Delete and arrival elsewhere: same picture, still zoomed (slide not rebuilt).
        (
            "after-delete-count",
            "3",
            "the gallery did not lose the deleted video and gain the new picture",
        ),
        (
            "after-delete-index",
            "2",
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
            "lone-video",
            "true",
            "a video opened on a page already in place did not play",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    // Gallery read from core, chat's pictures, GIFs, videos.
    assert!(
        methods.iter().any(|method| method == "get_chat_media"),
        "the gallery never asked the core for the chat's media: {methods:?}"
    );
}
