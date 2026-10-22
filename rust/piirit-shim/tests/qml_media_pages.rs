//! Pictures and video open in Piirit (system handover failed to show them). Conversation
//! picks kinds for the pages (swiping: `qml_gallery.rs`); others still go to system (a page only
//! saying "cannot show this" would be worse than the handover). Checks wiring: which kind gets
//! which page, picture page draws and pans once zoomed, video waits for its play button (Jolla's)
//! which plays and pauses it, Save to device copies into Piirit's folder in Downloads. Seek bar:
//! viewer's overlay (`qml_gallery_turn.rs`).

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

/// Silica `pageStack`, recorded not performed. Context property: probe-declared object
/// invisible to separate page. Test records only what it asks: which page a tap opened.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:GalleryPage.qml(9)|push:PicturePage.qml|...`: message opened at, if any.
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
}

impl PageStackProbe {
    // `qt_method!` params match dispatcher's by-value ones: piirit-shim/src/lib.rs.
    #[allow(clippy::needless_pass_by_value)]
    fn push(&mut self, page: QString, properties: QVariantMap) {
        // File name, and message gallery opens at.
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page);
        let at =
            i32::from_qvariant(properties.value(QString::from("messageId"), QVariant::default()))
                .map(|id| format!("({id})"))
                .unwrap_or_default();
        let current = self.log.to_string();
        self.log = format!("{current}push:{name}{at}|").into();
        self.log_changed();
    }
}

/// 1x1 PNG: a picture here.
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        // Where the platform says downloads go, pointed at a directory of
        // this test's own.
        function setDownloads(folder) { StandardPaths.download = folder; return 'ok' }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function load(url, properties) {
            loader.setSource('', {})
            loader.setSource(url, properties)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadConversation(url) {
            return load(url, {
                accountId: 1,
                chatId: 1,
                status: PageStatus.Activating
            })
        }
        /// What a tap on an attachment of this kind, message 9, does.
        function route(viewType) {
            loader.item.openAttachment('file:///tmp/whatever', 'whatever',
                                       viewType, 0, 9)
            return 'ok'
        }
        function call(name) { return '' + loader.item[name]() }
        /// A double tap at a point in the view, and where the view ends up.
        function zoomAt(viewX, viewY) {
            var view = findIn(loader.item, 'pictureView')
            if (!view) { return 'missing:pictureView' }
            view.toggleZoom(viewX, viewY)
            var flick = findIn(loader.item, 'pictureFlick')
            if (!flick) { return 'missing:pictureFlick' }
            return Math.round(flick.contentX) + ',' + Math.round(flick.contentY)
        }
        function get(property) { return '' + loader.item[property] }
        function set(property, value) {
            loader.item[property] = value
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
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function of(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + Math.round(item[property]) 
        }
        function flagOf(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        /// How wide an image asks to be decoded.
        function decodeWidthOf(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item.sourceSize.width
        }
        /// The box an image asks to be decoded to fit in.
        function decodeBoxOf(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return item.sourceSize.width + 'x' + item.sourceSize.height
        }
        function setOn(name, property, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item[property] = value
            return 'ok'
        }
        /// Whether the output was handed the player rather than a URL: the
        /// two are wired together or nothing appears at all.
        function outputIsWiredToPlayer() {
            var output = findIn(loader.item, 'videoOutput')
            var player = findIn(loader.item, 'player')
            if (!output || !player) { return 'missing' }
            return output.source === player ? 'wired' : 'not-wired'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn pictures_and_video_open_here_and_everything_else_goes_to_the_system() {
    let temp = std::env::temp_dir().join(format!("piirit-media-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let png = temp.join("dot.png");
    std::fs::write(&png, ONE_PIXEL_PNG).expect("write png");
    let png_url = format!("file://{}", png.display());
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(PageStackProbe::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
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

    // Kind to page.
    let conversation = common::page_url_in(&tree, "ConversationPage.qml");
    // Closures take what they need by value.
    let picture_page = common::page_url_in(&tree, "PicturePage.qml");
    let gif_page = picture_page.clone();
    let row_page = picture_page.clone();
    // Gallery's video slide.
    let video_page = common::component_url("VideoView.qml");
    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "conversation",
            call!("loadConversation", QString::from(conversation.clone())),
        ));
        for kind in ["Image", "Gif", "Sticker", "Video", "File", "Vcard"] {
            (*steps_ptr).push(("routed", call!("route", QString::from(kind))));
        }
    });

    // Picture page, on a still.
    let still = png_url.clone();
    single_shot(Duration::from_secs(2), move || unsafe {
        let mut properties = QVariantMap::default();
        properties.insert("fileUrl".into(), QString::from(still.clone()).into());
        properties.insert("fileName".into(), QString::from("dot.png").into());
        properties.insert("viewType".into(), QString::from("Image").into());
        (*steps_ptr).push((
            "picture",
            call!("load", QString::from(picture_page.clone()), properties),
        ));
    });
    let downloads = temp.join("Downloads");
    let downloads_for_probe = downloads.to_string_lossy().into_owned();
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push((
            "picture-shown",
            call!(
                "flagOf",
                QString::from("fullPicture"),
                QString::from("visible")
            ),
        ));
        // Copy kept by reader, in Piirit's Downloads folder.
        call!("setDownloads", QString::from(downloads_for_probe.clone()));
        (*steps_ptr).push(("save", call!("click", QString::from("saveToDevice"))));
        // Copy off Qt thread: entry waits, notice after.
        (*steps_ptr).push((
            "saving",
            call!(
                "flagOf",
                QString::from("saveToDevice"),
                QString::from("enabled")
            ),
        ));
        (*steps_ptr).push((
            "animation-shown",
            call!(
                "flagOf",
                QString::from("fullAnimation"),
                QString::from("visible")
            ),
        ));
        (*steps_ptr).push((
            "full-decode-box",
            call!("decodeBoxOf", QString::from("fullPicture")),
        ));
        // Fitted: nothing to pan.
        (*steps_ptr).push((
            "fitted-content",
            call!(
                "of",
                QString::from("pictureFlick"),
                QString::from("contentWidth")
            ),
        ));
        // Fitted: no zoomed copy decoded.
        (*steps_ptr).push((
            "fitted-sharp",
            call!(
                "flagOf",
                QString::from("sharpPicture"),
                QString::from("source")
            ),
        ));
        // Zoomed bottom right, where fingers went.
        (*steps_ptr).push(("zoomed", call!("zoomAt", 480, 700)));
        (*steps_ptr).push((
            "sharp-decode-box",
            call!("decodeBoxOf", QString::from("sharpPicture")),
        ));
        (*steps_ptr).push((
            "full-file",
            call!(
                "flagOf",
                QString::from("fullPicture"),
                QString::from("source")
            ),
        ));
        (*steps_ptr).push((
            "zoomed-sharp",
            call!(
                "flagOf",
                QString::from("sharpPicture"),
                QString::from("source")
            ),
        ));
        (*steps_ptr).push((
            "zoomed-content",
            call!(
                "of",
                QString::from("pictureFlick"),
                QString::from("contentWidth")
            ),
        ));
    });

    // GIF: only kind with animation over still.
    let animated = png_url.clone();
    single_shot(Duration::from_secs(5), move || unsafe {
        // Save answered by now: read before page goes, notice with it.
        (*steps_ptr).push((
            "saved-notice",
            call!(
                "flagOf",
                QString::from("noticeLabel"),
                QString::from("text")
            ),
        ));
        (*steps_ptr).push((
            "saved-again",
            call!(
                "flagOf",
                QString::from("saveToDevice"),
                QString::from("enabled")
            ),
        ));
        let mut properties = QVariantMap::default();
        properties.insert("fileUrl".into(), QString::from(animated.clone()).into());
        properties.insert("viewType".into(), QString::from("Gif").into());
        (*steps_ptr).push((
            "gif",
            call!("load", QString::from(gif_page.clone()), properties),
        ));
        (*steps_ptr).push((
            "gif-animated",
            call!(
                "flagOf",
                QString::from("fullAnimation"),
                QString::from("visible")
            ),
        ));
        (*steps_ptr).push((
            "gif-cache",
            call!(
                "flagOf",
                QString::from("fullAnimation"),
                QString::from("cache")
            ),
        ));
    });

    // Opened from row mid-transition: row decodes at row width; full picture waits for page.
    let from_row = png_url;
    single_shot(Duration::from_millis(5500), move || unsafe {
        let mut properties = QVariantMap::default();
        properties.insert("fileUrl".into(), QString::from(from_row.clone()).into());
        properties.insert("viewType".into(), QString::from("Image").into());
        properties.insert("previewWidth".into(), 421.into());
        properties.insert("status".into(), 1.into());
        (*steps_ptr).push((
            "arriving",
            call!("load", QString::from(row_page.clone()), properties),
        ));
        (*steps_ptr).push((
            "arriving-preview-width",
            call!("decodeWidthOf", QString::from("previewPicture")),
        ));
        (*steps_ptr).push((
            "arriving-preview-source",
            call!(
                "flagOf",
                QString::from("previewPicture"),
                QString::from("source")
            ),
        ));
        (*steps_ptr).push((
            "arriving-full-source",
            call!(
                "flagOf",
                QString::from("fullPicture"),
                QString::from("source")
            ),
        ));
        // Transition ends.
        (*steps_ptr).push(("arrived", call!("set", QString::from("status"), 2)));
        (*steps_ptr).push((
            "arrived-full-source",
            call!(
                "flagOf",
                QString::from("fullPicture"),
                QString::from("source")
            ),
        ));
    });

    // Video page.
    single_shot(Duration::from_secs(6), move || unsafe {
        let mut properties = QVariantMap::default();
        properties.insert(
            "fileUrl".into(),
            QString::from("file:///tmp/clip.mp4").into(),
        );
        properties.insert("fileName".into(), QString::from("clip.mp4").into());
        (*steps_ptr).push((
            "video",
            call!("load", QString::from(video_page.clone()), properties),
        ));
        (*steps_ptr).push(("wiring", call!("outputIsWiredToPlayer")));
        (*steps_ptr).push((
            "opened",
            call!(
                "flagOf",
                QString::from("player"),
                QString::from("playbackState")
            ),
        ));
        (*steps_ptr).push((
            "opened-poster",
            call!("flagOf", QString::from("poster"), QString::from("visible")),
        ));
        (*steps_ptr).push(("start", call!("call", QString::from("toggle"))));
        (*steps_ptr).push((
            "started",
            call!(
                "flagOf",
                QString::from("player"),
                QString::from("playbackState")
            ),
        ));
        (*steps_ptr).push((
            "started-file",
            call!("flagOf", QString::from("player"), QString::from("source")),
        ));
        (*steps_ptr).push(("pause", call!("call", QString::from("toggle"))));
        (*steps_ptr).push((
            "paused",
            call!(
                "flagOf",
                QString::from("player"),
                QString::from("playbackState")
            ),
        ));
        (*steps_ptr).push(("play", call!("call", QString::from("toggle"))));
        (*steps_ptr).push((
            "playing",
            call!(
                "flagOf",
                QString::from("player"),
                QString::from("playbackState")
            ),
        ));
        (*engine_ptr).quit();
    });

    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_outcome(&steps, &navigation);

    let copy = downloads.join("Piirit").join("dot.png");
    assert_eq!(
        std::fs::read(&copy).ok().as_deref(),
        Some(ONE_PIXEL_PNG),
        "Save to device did not put a copy of the picture in Piirit's \
         folder in Downloads: {}",
        copy.display()
    );
    let _ = std::fs::remove_dir_all(&temp);
}

/// What run must show.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)], navigation: &str) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let number = |label: &str| value(label).parse::<i64>().unwrap_or(-1);
    let context = format!("steps: {steps:?}, navigation: {navigation:?}");

    assert_eq!(
        value("conversation"),
        "ok",
        "the conversation page did not load. {context}"
    );
    // Image, Gif, Sticker: picture page; Video: video page; File, Vcard: system. No page for a
    // file: what it needs and a tap can't give is a copy, which is on row menu.
    // Image, Gif, Video: gallery, at tapped message; Sticker (not in gallery): alone.
    assert_eq!(
        navigation,
        "push:GalleryPage.qml(9)|push:GalleryPage.qml(9)|push:PicturePage.qml|\
         push:GalleryPage.qml(9)|",
        "tapping an attachment did not open the right thing. Nothing at \
         all for a picture means it still leaves the app; a page for a \
         file or a contact means the app took on something it cannot \
         show; a gallery without the message means it opens somewhere \
         else in the chat. {context}"
    );

    assert_eq!(
        value("picture"),
        "ok",
        "the picture page did not load. {context}"
    );
    assert_eq!(
        value("picture-shown"),
        "true",
        "the picture page shows no picture. {context}"
    );
    assert_eq!(value("save"), "ok", "no Save to device entry. {context}");
    assert_eq!(
        value("saving"),
        "false",
        "Save to device could be pulled again while the copy was being \
         made, or the copy was made on the Qt thread before the tap \
         returned. {context}"
    );
    assert_eq!(
        value("saved-notice"),
        "Saved to Downloads/Piirit",
        "saving did not say where the copy went. {context}"
    );
    assert_eq!(
        value("saved-again"),
        "true",
        "Save to device stayed off after the copy was made. {context}"
    );
    assert_eq!(
        value("animation-shown"),
        "false",
        "a still image was given the animation layer as well, which decodes \
         a movie that is not there. {context}"
    );
    // 540 = stub page width; fitting picture has nothing to pan to.
    assert_eq!(
        number("fitted-content"),
        540,
        "the picture did not open fitted to the page. {context}"
    );
    // Zoom toward tap, not corner: corner zoom takes bottom-right tap elsewhere.
    let corner = value("zoomed");
    let (across, down) = corner.split_once(',').unwrap_or(("0", "0"));
    assert!(
        across.parse::<i64>().unwrap_or(0) > 0 && down.parse::<i64>().unwrap_or(0) > 0,
        "zooming in on the bottom right of the picture left the view at \
         {corner:?}: it grew about its own top left corner and took the \
         reader there. {context}"
    );
    assert!(
        number("zoomed-content") > number("fitted-content"),
        "zooming in did not give the view anything to pan over, so the \
         parts of the picture off the edge cannot be reached: {} then {}. \
         {context}",
        number("fitted-content"),
        number("zoomed-content")
    );

    assert_eq!(
        value("gif"),
        "ok",
        "the picture page did not load a GIF. {context}"
    );
    assert_eq!(
        value("gif-animated"),
        "true",
        "a GIF was shown as a still. {context}"
    );
    assert_eq!(
        value("gif-cache"),
        "false",
        "the picture page keeps every frame of a GIF at full size for as \
         long as it is open. {context}"
    );
    // Stub page 540x960, width a third over (sideways-stored 4:3 photo fits exactly). Twice
    // the page on every slide was a 14-25 MB upload per swipe (lag). A bound of the long edge
    // on both sides let a landscape photo through whole.
    assert_eq!(
        value("full-decode-box"),
        "720x960",
        "the full picture is not decoded to about the page it is fitted \
         to. {context}"
    );
    // Twice the page, only once zoomed: zoom 2 stays sharp.
    assert_eq!(
        value("fitted-sharp"),
        "",
        "a picture at fitted size decodes a twice-size copy it does not \
         need. {context}"
    );
    assert!(
        value("full-file").starts_with("file://"),
        "the full picture has no file. {context}"
    );
    assert_eq!(
        value("zoomed-sharp"),
        value("full-file"),
        "a zoomed picture is not decoded again sharp. {context}"
    );
    assert_eq!(
        value("sharp-decode-box"),
        "1080x1920",
        "the zoomed picture's copy is not decoded to twice the page. \
         {context}"
    );

    assert_eq!(
        value("arriving"),
        "ok",
        "the picture page did not load mid-transition. {context}"
    );
    assert_eq!(
        number("arriving-preview-width"),
        421,
        "the page did not ask for the picture at the width the row drew \
         it, which is the size Qt has it cached at: a second decode \
         behind the transition instead of none. {context}"
    );
    assert!(
        value("arriving-preview-source").ends_with("dot.png"),
        "the row's decode is not what the page shows while it arrives. \
         {context}"
    );
    assert_eq!(
        value("arriving-full-source"),
        "",
        "the full picture was decoded during the transition, which is the \
         stutter on opening one. {context}"
    );
    assert!(
        value("arrived-full-source").ends_with("dot.png"),
        "the full picture never loaded once the page was in place. {context}"
    );

    assert_eq!(
        value("video"),
        "ok",
        "the video page did not load. {context}"
    );
    assert_eq!(
        value("wiring"),
        "wired",
        "the video page has a player and a picture and did not connect \
         them. {context}"
    );
    // MediaPlayer values: 0 stopped, 1 playing, 2 paused.
    assert_eq!(
        value("opened"),
        "0",
        "the video started playing before its play button was pressed \
         (Jolla's waits). {context}"
    );
    assert_eq!(
        value("opened-poster"),
        "true",
        "the video waiting for play shows black, not its still frame. {context}"
    );
    assert_eq!(
        value("started"),
        "1",
        "the play button did not start the video. {context}"
    );
    assert_eq!(
        value("started-file"),
        "file:///tmp/clip.mp4",
        "the play button did not give the player the video's file. {context}"
    );
    assert_eq!(
        value("paused"),
        "2",
        "the button did not pause a playing video, so it only ever plays. \
         {context}"
    );
    assert_eq!(
        value("playing"),
        "1",
        "the play button did not start the video again. {context}"
    );
}
