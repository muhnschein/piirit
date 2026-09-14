//! Pictures and video open in Postivene.
//!
//! Reported from a device: tapping either handed the file to the system,
//! which took the reader out of the app to something that then failed to
//! show it. Voice messages already played where they sat; these had nowhere
//! to go.
//!
//! So there are two pages, and the conversation decides which kinds reach
//! them. Everything else still goes to whatever else on the phone handles
//! it -- a page here that could only say "cannot show this" would be worse
//! than the handover it replaced.
//!
//! What a headless run can check is the wiring: which kind gets which page,
//! that a picture page draws a picture and has somewhere to pan to once it
//! is zoomed, that the video page's seek bar follows the player without
//! fighting the reader for it, and that Save to device puts a copy where
//! the platform says pictures go.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Silica's `pageStack`, recorded rather than performed. A context
/// property, as in the app: an object declared in the probe below would not
/// be visible inside a separately loaded page. Its own, as in every other
/// test that needs one -- each records only what it asks about, and this
/// one asks only which page a tap opened.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:PicturePage.qml|push:VideoPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
}

impl PageStackProbe {
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see postivene-shim/src/lib.rs.
    #[allow(clippy::needless_pass_by_value)]
    fn push(&mut self, page: QString, _properties: QVariantMap) {
        // Only the file name matters; the rest is a checkout path.
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page);
        let current = self.log.to_string();
        self.log = format!("{current}push:{name}|").into();
        self.log_changed();
    }
}

/// A 1x1 PNG, which is a picture as far as any of this is concerned.
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
        // Where the platform says pictures go, pointed at a directory of
        // this test's own.
        function setPictures(folder) { StandardPaths.pictures = folder; return 'ok' }
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
        /// What a tap on an attachment of this kind does.
        function route(viewType) {
            loader.item.openAttachment('file:///tmp/whatever', 'whatever',
                                       viewType)
            return 'ok'
        }
        function call(name) { return '' + loader.item[name]() }
        /// A double tap at a point in the view, and where the view ends up.
        function zoomAt(viewX, viewY) {
            loader.item.toggleZoom(viewX, viewY)
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
    let temp = std::env::temp_dir().join(format!("postivene-media-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let png = temp.join("dot.png");
    std::fs::write(&png, ONE_PIXEL_PNG).expect("write png");
    let png_url = format!("file://{}", png.display());
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    postivene_shim::register_qml_types();

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

    // Which kind reaches which page.
    let conversation = common::page_url_in(&tree, "ConversationPage.qml");
    // Resolved once: each closure below takes what it needs by value.
    let picture_page = common::page_url_in(&tree, "PicturePage.qml");
    let gif_page = picture_page.clone();
    let row_page = picture_page.clone();
    let video_page = common::page_url_in(&tree, "VideoPage.qml");
    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "conversation",
            call!("loadConversation", QString::from(conversation.clone())),
        ));
        for kind in ["Image", "Gif", "Sticker", "Video", "File", "Vcard"] {
            (*steps_ptr).push(("routed", call!("route", QString::from(kind))));
        }
    });

    // The picture page, on a still.
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
    let pictures = temp.join("Pictures");
    let pictures_for_probe = pictures.to_string_lossy().into_owned();
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push((
            "picture-shown",
            call!(
                "flagOf",
                QString::from("fullPicture"),
                QString::from("visible")
            ),
        ));
        // A copy for the gallery, into the folder the platform names.
        call!("setPictures", QString::from(pictures_for_probe.clone()));
        (*steps_ptr).push(("save", call!("click", QString::from("saveToDevice"))));
        (*steps_ptr).push((
            "saved-notice",
            call!(
                "flagOf",
                QString::from("noticeLabel"),
                QString::from("text")
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
        // Fitted: nothing to pan, because the whole picture is on screen.
        (*steps_ptr).push((
            "fitted-content",
            call!(
                "of",
                QString::from("pictureFlick"),
                QString::from("contentWidth")
            ),
        ));
        // Zoomed in on the bottom right of the picture, which is where
        // the reader put their fingers.
        (*steps_ptr).push(("zoomed", call!("zoomAt", 480, 700)));
        (*steps_ptr).push((
            "zoomed-content",
            call!(
                "of",
                QString::from("pictureFlick"),
                QString::from("contentWidth")
            ),
        ));
    });

    // The same page on a GIF, which is the one kind that has an animation
    // over the still.
    let animated = png_url.clone();
    single_shot(Duration::from_secs(5), move || unsafe {
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
    });

    // Opened from a row, still transitioning in: the row's own decode is
    // asked for at the row's width, and the full picture waits for the
    // page to be in place.
    let from_row = png_url.clone();
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
        // The transition ends.
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

    // The video page.
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
        (*steps_ptr).push(("play", call!("call", QString::from("toggle"))));
        (*steps_ptr).push((
            "playing",
            call!(
                "flagOf",
                QString::from("player"),
                QString::from("playbackState")
            ),
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
        // The seek bar follows the video...
        (*steps_ptr).push((
            "position",
            call!(
                "setOn",
                QString::from("player"),
                QString::from("position"),
                5000
            ),
        ));
        (*steps_ptr).push((
            "followed",
            call!("of", QString::from("seek"), QString::from("value")),
        ));
        // ...and stops following it while the reader has hold of it.
        (*steps_ptr).push((
            "held",
            call!("setOn", QString::from("seek"), QString::from("down"), true),
        ));
        (*steps_ptr).push((
            "moved-while-held",
            call!(
                "setOn",
                QString::from("player"),
                QString::from("position"),
                9000
            ),
        ));
        (*steps_ptr).push((
            "held-value",
            call!("of", QString::from("seek"), QString::from("value")),
        ));
        (*engine_ptr).quit();
    });

    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_outcome(&steps, &navigation);

    let copy = pictures.join("dot.png");
    assert_eq!(
        std::fs::read(&copy).ok().as_deref(),
        Some(ONE_PIXEL_PNG),
        "Save to device did not put a copy of the picture in the Pictures \
         folder: {}",
        copy.display()
    );
    let _ = std::fs::remove_dir_all(&temp);
}

/// What the run has to show for itself, out of the test body.
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
    // Image, Gif and Sticker to the picture page, Video to the video
    // page -- and nothing for File or Vcard, which are the system's to
    // open. No page here for a file: what an attachment needs and a tap
    // cannot give is a copy, and that is on the row's menu.
    assert_eq!(
        navigation,
        "push:PicturePage.qml|push:PicturePage.qml|push:PicturePage.qml|\
         push:VideoPage.qml|",
        "tapping an attachment did not open the right thing. Nothing at \
         all for a picture means it still leaves the app; a page for a \
         file or a contact means the app took on something it cannot \
         show. {context}"
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
        value("saved-notice"),
        "Saved to Pictures",
        "saving did not say where the copy went. {context}"
    );
    assert_eq!(
        value("animation-shown"),
        "false",
        "a still image was given the animation layer as well, which decodes \
         a movie that is not there. {context}"
    );
    // 540 is the stub page's width, and a picture that fits is a picture
    // with nothing to pan to.
    assert_eq!(
        number("fitted-content"),
        540,
        "the picture did not open fitted to the page. {context}"
    );
    // Toward what was tapped, not toward the corner. Zooming about the
    // picture's own top left is what takes a reader who tapped a face in
    // the bottom right somewhere else entirely.
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
    // MediaPlayer's own values: 1 playing, 2 paused.
    assert_eq!(
        value("playing"),
        "1",
        "the play button did not start the video. {context}"
    );
    assert_eq!(
        value("paused"),
        "2",
        "the button did not pause a playing video, so it only ever plays. \
         {context}"
    );
    assert_eq!(
        number("followed"),
        5000,
        "the seek bar does not follow the video. {context}"
    );
    assert_eq!(
        number("held-value"),
        5000,
        "the video moved the seek bar out from under the reader's finger \
         while they were dragging it. {context}"
    );
}
