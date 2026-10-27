//! Pictures and videos not yet downloaded are in the gallery: core keeps such as a Text
//! pre-message (name, size, no file) until fetched, so its gallery list never names them.
//!
//! Headless vs stub Silica, recording double; group seeded with picture (10), video (15) and
//! pre-messages: picture 16 (sent before 15), video 17, PDF 18, PNG sent as file 19 (no size),
//! plain text 20. Checks: model holds 16 and 17 in conversation's order (not id order), as kind
//! their file will be, not 18, 19, 20; viewer slide says what conversation row says and offers
//! Download, which fetches and turns slide into picture in place; grid tile marks it with size to
//! fetch, tap opens viewer there, with nothing drawn till list has it; fetch under way keeps slide
//! the video it will be; Download's words per state; Text messages read once per process, not per
//! gallery opened.

// Qt harness: see qml_chat_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        id: probe
        Loader { id: loader }
        /// Silica's, as page finds it: what was pushed, not done.
        property QtObject pageStack: QtObject {
            property string pushed: ''
            function push(url, properties) {
                pushed = ('' + url).split('/').pop() + ':' + properties.messageId + ':'
                         + properties.viewType + ':' + properties.fileUrl
                return null
            }
        }
        // Conversation's order (oldest first), as viewer opened from chat.
        ChatMedia { id: oldest; account_id: 1; chat_id: 2; kind: 'gallery'; oldest_first: true }
        Connections {
            target: core
            onCore_event: oldest.handle_event(context_id, kind, payload_json)
            onStatus_changed: if (core.status === 'ready') { oldest.reload() }
        }
        Repeater {
            id: oldestRows
            model: oldest.rows
            delegate: Item {
                property string row: model.message_id + ':' + model.view_type + ':'
                                     + model.download_state + ':' + model.file_bytes
                property string said: model.text
            }
        }
        // Grid's order, made later: second reader of same chat.
        Loader {
            id: newestLoader
            active: false
            sourceComponent: Item {
                property alias rows: newestRows
                ChatMedia { id: newest; account_id: 1; chat_id: 2; kind: 'gallery' }
                Repeater {
                    id: newestRows
                    model: newest.rows
                    delegate: Item { property string row: '' + model.message_id }
                }
            }
        }
        function rowsOf(repeater) {
            var out = []
            for (var i = 0; i < repeater.count; i++) {
                var item = repeater.itemAt(i)
                out.push(item ? item.row : '?')
            }
            return out.join(',')
        }
        function oldestList() { return rowsOf(oldestRows) }
        function oldestText(index) {
            var item = oldestRows.itemAt(index)
            return item ? item.said : 'missing'
        }
        function makeNewest() { newestLoader.active = true; return 'ok' }
        function newestList() {
            return newestLoader.item ? rowsOf(newestLoader.item.rows) : 'missing'
        }
        function load(url, properties) {
            loader.setSource('', {})
            loader.setSource(url, properties)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadGallery(url) {
            return load(url, {
                accountId: 1, chatId: 2, messageId: 10, viewType: 'Image',
                fileUrl: 'file:///tmp/piirit-fake/photo.jpg', fileName: 'photo.jpg',
                newestFirst: false, status: 1
            })
        }
        /// From grid, at a tile with no file yet.
        function loadGalleryAtHeld(url) {
            return load(url, {
                accountId: 1, chatId: 2, messageId: 17, viewType: 'Video', fileUrl: '',
                fileName: 'trip.mp4', newestFirst: true, status: 1
            })
        }
        /// Where on screen is among list's: list's place, not drawn (Jolla's shows none).
        function place() {
            var list = findIn(loader.item, 'slides')
            return list ? (list.currentIndex + 1) + ' of ' + list.count : 'missing:slides'
        }
        function loadGrid(url) {
            return load(url, { accountId: 1, chatId: 2, kind: 'gallery' })
        }
        function loadHeld(url, state) {
            return load(url, {
                width: 540, height: 960, viewType: 'Video', text: ' [Video – 8 MiB]',
                downloadState: state
            })
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
        function swipeTo(index) {
            slides().currentIndex = index
            slides().positionViewAtIndex(index, ListView.SnapPosition)
            return 'ok'
        }
        /// On the slide now on screen.
        function current(name, property) {
            var slide = slides().currentItem
            if (!slide) { return 'no-current' }
            var item = findIn(slide, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        /// Slide on screen, kept: reread in place, or taken out and rebuilt (gone, null).
        property Item kept: null
        function keepCurrent() { kept = slides().currentItem; return 'ok' }
        function sameCurrent() { return '' + (kept !== null && slides().currentItem === kept) }
        function tapCurrent(name) {
            var slide = slides().currentItem
            var item = slide ? findIn(slide, name) : null
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// On one grid tile.
        function onTile(messageId, name, property) {
            var tile = findIn(loader.item, 'mediaTile' + messageId)
            if (!tile) { return 'missing:mediaTile' + messageId }
            var item = name.length > 0 ? findIn(tile, name) : tile
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function tapTile(messageId) {
            var tile = findIn(loader.item, 'mediaTile' + messageId)
            if (!tile) { return 'missing:mediaTile' + messageId }
            tile.clicked()
            return probe.pageStack.pushed
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn pictures_not_yet_downloaded_are_in_the_gallery_with_download() {
    let temp = std::env::temp_dir().join(format!("piirit-gallery-held-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRIT_FAKE_PRE_MESSAGES", "1");
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
    let grid = common::page_url("ChatMediaPage.qml");
    let held = common::component_url("HeldBackView.qml");

    let mut plan = common::Plan::default();

    // Model read; second reader, viewer from chat at picture 10.
    plan.after(2, move || unsafe {
        record!("oldest", call!("oldestList"));
        record!("oldest-said", call!("oldestText", 1));
        record!("make-newest", call!("makeNewest"));
        record!("load-gallery", call!("loadGallery", s!(gallery.clone())));
    });

    // Viewer: swipe to not-yet-downloaded picture, Download.
    plan.after(1, move || unsafe {
        record!("newest", call!("newestList"));
        record!("gallery-placed", call!("page", s!("placed")));
        record!("gallery-place", call!("place"));
        record!("swipe-held", call!("swipeTo", 1));
        record!("held-shown", call!("page", s!("shownId")));
        record!("held-place", call!("place"));
        // No file yet: nothing to share or save.
        record!("held-bar", call!("get", s!("iconBar"), s!("visible")));
        record!("held-said", call!("current", s!("heldView"), s!("text")));
        record!(
            "held-kind",
            call!("current", s!("heldView"), s!("viewType"))
        );
        record!(
            "held-button",
            call!("current", s!("downloadButton"), s!("visible"))
        );
        record!(
            "held-no-picture",
            call!("current", s!("pictureView"), s!("fileUrl"))
        );
        record!("download", call!("tapCurrent", s!("downloadButton")));
    });

    // Fetched: same slide now the picture. Then grid.
    plan.after(1, move || unsafe {
        record!("fetched-shown", call!("page", s!("shownId")));
        record!("fetched-place", call!("place"));
        record!("fetched-bar", call!("get", s!("iconBar"), s!("visible")));
        record!(
            "fetched-picture",
            call!("current", s!("pictureView"), s!("fileUrl"))
        );
        record!(
            "fetched-held",
            call!("current", s!("heldView"), s!("visible"))
        );
        record!("fetched-row", call!("oldestList"));
        // Video's fetch: under way, never lands (fake core).
        record!("swipe-video", call!("swipeTo", 3));
        record!("keep-video", call!("keepCurrent"));
        record!("fetch-video", call!("tapCurrent", s!("downloadButton")));
    });

    // Fetch under way: still the video's slide, saying so. Then grid.
    plan.after(1, move || unsafe {
        record!(
            "fetching-state",
            call!("current", s!("heldView"), s!("downloadState"))
        );
        record!(
            "fetching-kind",
            call!("current", s!("heldView"), s!("viewType"))
        );
        record!(
            "fetching-button",
            call!("current", s!("downloadButton"), s!("visible"))
        );
        record!("fetching-row", call!("oldestList"));
        record!("fetching-same-slide", call!("sameCurrent"));
        record!("load-grid", call!("loadGrid", s!(grid.clone())));
    });

    // Grid tiles; tap opens viewer there. Then Download's words per state.
    let held_url = held;
    let gallery_again = common::page_url("GalleryPage.qml");
    plan.after(1, move || unsafe {
        record!(
            "tile-held",
            call!("onTile", 17, s!("heldMark"), s!("visible"))
        );
        record!("tile-size", call!("onTile", 17, s!("heldSize"), s!("text")));
        record!(
            "tile-play",
            call!("onTile", 17, s!("playMark"), s!("visible"))
        );
        record!(
            "tile-fetched",
            call!("onTile", 16, s!("heldMark"), s!("visible"))
        );
        record!(
            "tile-picture",
            call!("onTile", 10, s!("heldMark"), s!("visible"))
        );
        record!("tile-tap", call!("tapTile", 17));
        for (label, state) in [
            ("state-available", "Available"),
            ("state-fetching", "InProgress"),
            ("state-failed", "Failure"),
            ("state-unreadable", "Undecipherable"),
        ] {
            call!("loadHeld", s!(held_url.clone()), s!(state));
            let said = call!("get", s!("heldText"), s!("text"));
            let note = call!("get", s!("heldState"), s!("text"));
            let shown = call!("get", s!("heldState"), s!("visible"));
            let button = call!("get", s!("downloadButton"), s!("visible"));
            let busy = call!("get", s!("heldBusy"), s!("running"));
            record!(
                label,
                format!("{said}|{note}|state={shown}|button={button}|busy={busy}")
            );
        }
        // Viewer opened at tile with no file: nothing alone first.
        record!(
            "load-at-held",
            call!("loadGalleryAtHeld", s!(gallery_again.clone()))
        );
        record!("at-held-seed", call!("get", s!("seed"), s!("active")));
    });

    plan.after(1, move || unsafe {
        record!("at-held-placed", call!("page", s!("placed")));
        record!(
            "at-held-state",
            call!("current", s!("heldView"), s!("downloadState"))
        );
        record!("at-held-place", call!("place"));
        record!("at-held-bar", call!("get", s!("iconBar"), s!("visible")));
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps.iter().find(|(name, _)| *name == label).map_or_else(
            || "<step did not run>".to_string(),
            |(_, value)| value.clone(),
        )
    };
    let context = format!("steps: {steps:?}");

    for (label, expected, complaint) in [
        (
            "oldest",
            "10:Image:Done:0,16:Image:Available:2516582,15:Video:Done:1048576,\
             17:Video:Available:8388608",
            "the gallery is not the chat's pictures and videos with the not yet \
             downloaded ones among them, in the conversation's order, each as the \
             kind its file will be; nor without the PDF, the PNG sent as a file \
             and the plain text",
        ),
        (
            "oldest-said",
            " [Image – 2.4 MiB]",
            "the not yet downloaded picture lost what its conversation row says",
        ),
        (
            "newest",
            "17,15,16,10",
            "the grid's gallery (newest first) is not the same set reversed",
        ),
        ("gallery-placed", "true", "the viewer never took over"),
        (
            "gallery-place",
            "1 of 4",
            "the viewer does not count the not yet downloaded ones",
        ),
        ("swipe-held", "ok", "the probe could not swipe"),
        (
            "held-shown",
            "16",
            "the swipe did not land on the not yet downloaded picture",
        ),
        (
            "held-place",
            "2 of 4",
            "the viewer lost count on the not yet downloaded picture",
        ),
        (
            "held-bar",
            "false",
            "a picture not yet downloaded offers Share and Save, with no file \
             to hand on",
        ),
        (
            "fetched-bar",
            "true",
            "once the picture arrived, the viewer does not offer Share and Save",
        ),
        (
            "at-held-bar",
            "false",
            "opened at a tile with no file, the viewer offers Share and Save",
        ),
        (
            "held-said",
            " [Image – 2.4 MiB]",
            "the slide does not say what the conversation row says",
        ),
        (
            "held-kind",
            "Image",
            "the slide does not know it will be a picture",
        ),
        ("held-button", "true", "the slide offers no Download"),
        (
            "held-no-picture",
            "missing:pictureView",
            "a picture view was built with no file to show",
        ),
        ("download", "ok", "the probe could not tap Download"),
        (
            "fetched-shown",
            "16",
            "the viewer moved off the picture once it arrived",
        ),
        (
            "fetched-place",
            "2 of 4",
            "the viewer's count changed once the picture arrived",
        ),
        (
            "fetched-picture",
            "file:///tmp/piirit-fake/beach.jpg",
            "the fetched picture is not shown where its slide was",
        ),
        (
            "fetched-held",
            "missing:heldView",
            "the slide still offers Download once the picture arrived",
        ),
        (
            "fetched-row",
            "10:Image:Done:0,16:Image:Done:2516582,15:Video:Done:1048576,\
             17:Video:Available:8388608",
            "the model's row did not take in the fetched picture",
        ),
        (
            "swipe-video",
            "ok",
            "the probe could not swipe to the video",
        ),
        (
            "fetch-video",
            "ok",
            "the probe could not tap the video's Download",
        ),
        (
            "fetching-state",
            "InProgress",
            "the slide does not hear that the video is being fetched",
        ),
        (
            "fetching-kind",
            "Video",
            "reread while being fetched, the slide forgot it will be a video",
        ),
        (
            "fetching-button",
            "false",
            "the slide offers Download while the video is being fetched",
        ),
        (
            "fetching-row",
            "10:Image:Done:0,16:Image:Done:2516582,15:Video:Done:1048576,\
             17:Video:InProgress:8388608",
            "reread while being fetched, the model's row lost its kind or state",
        ),
        (
            "keep-video",
            "ok",
            "the probe could not keep the video's slide",
        ),
        (
            "fetching-same-slide",
            "true",
            "reread while being fetched, the video's slide was taken out and put back \
             (rebuilt: a flicker, a swipe under way lost), not reread in place",
        ),
        ("load-grid", "ok", "the grid did not load"),
        (
            "tile-held",
            "true",
            "the grid tile of a not yet downloaded video has no mark",
        ),
        (
            "tile-size",
            "8.4 MB",
            "the grid tile does not say how much there is to fetch",
        ),
        (
            "tile-play",
            "false",
            "the grid tile shows a play mark for a video with nothing to play",
        ),
        (
            "tile-fetched",
            "false",
            "a fetched picture's tile still says to download",
        ),
        (
            "tile-picture",
            "false",
            "a downloaded picture's tile says to download",
        ),
        (
            "tile-tap",
            "GalleryPage.qml:17:Video:",
            "tapping a not yet downloaded tile did not open the viewer at it",
        ),
        (
            "state-available",
            "[Video – 8 MiB]||state=false|button=true|busy=false",
            "a not yet downloaded video does not say its kind and size and \
             offer Download, or says more",
        ),
        (
            "state-fetching",
            "[Video – 8 MiB]|Downloading…|state=true|button=false|busy=true",
            "a fetching video does not say so, or still offers Download",
        ),
        (
            "state-failed",
            "[Video – 8 MiB]|Download failed|state=true|button=true|busy=false",
            "a failed fetch does not say so, or offers no second try",
        ),
        (
            "load-at-held",
            "ok",
            "the viewer did not load at a tile with no file",
        ),
        (
            "at-held-seed",
            "false",
            "opened at a tile with no file, the viewer built a picture or video \
             of nothing",
        ),
        (
            "at-held-placed",
            "true",
            "opened at a tile with no file, the viewer never took over",
        ),
        (
            "at-held-state",
            "InProgress",
            "opened at a tile with no file, the viewer is not on its slide",
        ),
        (
            "at-held-place",
            "1 of 4",
            "opened from the grid, the order is not the grid's",
        ),
        (
            "state-unreadable",
            "[Video – 8 MiB]|Cannot be decrypted|state=true|button=false|busy=false",
            "an unreadable message does not say so, or offers Download",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    let calls = common::calls(&journal);
    let reads_of = |id: u64| {
        calls
            .iter()
            .filter(|(method, params)| {
                method == "get_messages"
                    && params
                        .get(1)
                        .and_then(Value::as_array)
                        .is_some_and(|ids| ids.iter().any(|read| read.as_u64() == Some(id)))
            })
            .count()
    };
    // Four readers (two models, viewer, grid); plain ones read by first only.
    for id in [18, 19, 20] {
        assert_eq!(
            reads_of(id),
            1,
            "message {id} is plain for the gallery yet was read by every gallery \
             opened, not once: a long chat's every text read per open. calls: {calls:?}"
        );
    }
    assert!(
        calls
            .iter()
            .any(|(method, params)| method == "download_full_message"
                && params.get(1).and_then(Value::as_u64) == Some(16)),
        "Download did not ask the core to fetch the picture: {calls:?}"
    );
}
