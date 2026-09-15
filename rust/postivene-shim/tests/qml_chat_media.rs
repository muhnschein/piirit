//! A chat's pictures, sounds, files and apps have pages of their own,
//! reached from tiles on the contact's and the group's page.
//!
//! Loaded headlessly against the stub Silica module and the recording
//! double, with the group seeded with one of each kind. What a headless
//! run can check is the wiring: the gallery draws a tile per picture or
//! video and a tap opens the page the conversation opens for that kind;
//! the files list names a file and its size and keeps a copy where the
//! platform says downloads go; the audio and apps lists draw the
//! conversation's own rows, and a tap on an app runs it; a kind the chat
//! has none of says so; and the tiles on both detail pages open the
//! page for the kind that was tapped, with no Apps tile while apps are
//! off.
//!
//! A long press on a tile or a row offers Show in chat and Delete. The
//! first walks the page stack down to the conversation this page was
//! opened over, tells it the message and pops to it; the stack here is a
//! record that hands back a detail page that cannot be asked and then a
//! conversation that can. The second waits out the platform's countdown
//! and then asks the core, and the row is gone once the core has agreed.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used,
    // qt_method! declarations must match the generated dispatcher's
    // by-value parameters; see postivene-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;
use serde_json::Value;

mod common;

/// Silica's `pageStack`, recorded rather than performed: which page a
/// tap opened, and which kind of media it was asked for.
///
/// The pages under the media page are two objects the probe QML made
/// and handed over -- a `QVariant` carries a QML object -- and
/// `previousPage` answers with them in turn: first a detail page that
/// cannot be asked to show a message, then the conversation, which can.
#[derive(QObject, Default)]
#[allow(non_snake_case)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `push:ChatMediaPage.qml(gallery)|push:PicturePage.qml()|pop|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// The contact's page under the media page, as the stack sees it.
    detail: qt_property!(QVariant),
    /// The conversation under that.
    chat: qt_property!(QVariant),
    /// The page Delete leads to, which asks which kind of delete this
    /// is: `push` hands it back the way Silica hands back the page it
    /// made, and the test picks on it.
    chooser: qt_property!(QVariant),
    /// How many steps down the current walk has taken.
    hops: qt_property!(u32),
    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap) -> QVariant),
    previousPage: qt_method!(fn(&mut self, page: QVariant) -> QVariant),
    pop: qt_method!(fn(&mut self, page: QVariant)),
}

#[allow(non_snake_case)]
impl PageStackProbe {
    fn note(&mut self, entry: &str) {
        let current = self.log.to_string();
        self.log = format!("{current}{entry}|").into();
        self.log_changed();
    }

    fn push(&mut self, page: QString, properties: QVariantMap) -> QVariant {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let kind =
            QString::from_qvariant(properties.value(QString::from("kind"), QVariant::default()))
                .map(|kind| kind.to_string())
                .unwrap_or_default();
        // The delete page carries no kind; what it is asked with is
        // whether the core would take a deletion for everyone of this
        // message, which is the half of it worth reading back.
        let everyone = bool::from_qvariant(
            properties.value(QString::from("canDeleteForEveryone"), QVariant::default()),
        )
        .map(|gate| format!("everyone={gate}"))
        .unwrap_or_default();
        self.note(&format!("push:{name}({kind}{everyone})"));
        if name == "DeleteMessagePage.qml" {
            return self.chooser.clone();
        }
        QVariant::default()
    }

    /// The page below the one asked about: the detail page first, the
    /// conversation under it, and nothing under that.
    fn previousPage(&mut self, _page: QVariant) -> QVariant {
        self.hops += 1;
        match self.hops {
            1 => self.detail.clone(),
            2 => self.chat.clone(),
            _ => QVariant::default(),
        }
    }

    fn pop(&mut self, _page: QVariant) {
        self.hops = 0;
        self.note("pop");
    }
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        function load(url, properties) {
            loader.setSource('', {})
            loader.setSource(url, properties)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function loadMedia(url, chatId, kind) {
            return load(url, { accountId: 1, chatId: chatId, kind: kind })
        }
        function loadDetail(url, chatId) {
            return load(url, { accountId: 1, chatId: chatId, chatName: 'from the list' })
        }
        function loadTiles(url) {
            return load(url, { width: 540, appsAvailable: true })
        }
        // The pages under the media page, as the stack answers for them:
        // the contact's page, which cannot show a message, and the
        // conversation, which can and remembers which.
        QtObject { id: detailBelow; objectName: 'detailBelow' }
        QtObject {
            id: chatBelow
            objectName: 'chatBelow'
            property int shown: 0
            function showMessage(messageId) { shown = messageId }
        }
        // The page Delete leads to, as the stack hands it back: which
        // kind of delete is asked there, and this is the answer.
        QtObject {
            id: chooser
            objectName: 'chooser'
            signal picked(bool forEveryone)
            function pick(forEveryone) { picked(forEveryone) }
        }
        function armStack() {
            pageStack.detail = detailBelow
            pageStack.chat = chatBelow
            pageStack.chooser = chooser
            return 'ok'
        }
        function pickDelete(forEveryone) { chooser.pick(forEveryone); return 'ok' }
        function shownInChat() { return '' + chatBelow.shown }
        // The wait before a deletion, turned down from four seconds.
        function hurry(ms) { loader.item.pendingDelay = ms; return 'ok' }
        // Where the platform says downloads go, pointed at a directory of
        // this test's own.
        function setDownloads(folder) { StandardPaths.download = folder; return 'ok' }
        // `data` rather than `children`: the model is a plain QObject, so
        // it is not among an Item's visual children at all.
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
            // A row's menu is not among its children.
            if (node.menu) {
                var inMenu = findIn(node.menu, name)
                if (inMenu) { return inMenu }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        // A menu item of one particular row, since every row has one.
        function clickIn(rowName, itemName) {
            var row = findIn(loader.item, rowName)
            if (!row) { return 'missing:' + rowName }
            var item = findIn(row.menu, itemName)
            if (!item) { return 'missing:' + itemName }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_chats_media_has_pages_of_its_own_behind_the_tiles() {
    let temp =
        std::env::temp_dir().join(format!("postivene-chat-media-page-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // The file the fake says the group holds, so that a copy of it can
    // be made.
    let fake_dir = std::path::Path::new("/tmp/postivene-fake");
    std::fs::create_dir_all(fake_dir).expect("create the fake's file dir");
    std::fs::write(fake_dir.join("notes.pdf"), b"%PDF-1.4 notes").expect("write the fake pdf");
    let downloads = temp.join("Downloads");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_FAKE_JOURNAL", &journal);
        std::env::set_var("POSTIVENE_FAKE_MEDIA", "1");
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

    let media_page = common::page_url("ChatMediaPage.qml");
    let contact_page = common::page_url("ContactPage.qml");
    let group_page = common::page_url("GroupPage.qml");
    let tiles = common::component_url("MediaKinds.qml");
    let downloads_for_probe = downloads.to_string_lossy().into_owned();

    // The group's gallery: a picture and a video.
    let page = media_page.clone();
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load-gallery",
            call!(
                "loadMedia",
                QString::from(page.clone()),
                2,
                QString::from("gallery")
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("gallery-loaded", get!("media", "loaded"));
        record!("gallery-count", get!("media", "count"));
        record!("gallery-busy", get!("mediaBusy", "running"));
        record!("gallery-placeholder", get!("placeholder", "enabled"));
        // Newest first: the video above the picture.
        record!("first-tile", get!("mediaTile15", "objectName"));
        record!("second-tile", get!("mediaTile10", "objectName"));
        record!("video-thumb", get!("mediaTile15", "fileUrl"));
        record!("video-mark", get!("mediaTile15", "isVideo"));
        record!("picture-thumb", get!("mediaTile10", "fileUrl"));
        record!("picture-mark", get!("mediaTile10", "isVideo"));
        record!("open-picture", call!("click", QString::from("mediaTile10")));
        record!("open-video", call!("click", QString::from("mediaTile15")));
        // Show in chat, from the video's tile: the conversation two
        // pages down is told which message, and popped to.
        record!("arm-stack", call!("armStack"));
        record!(
            "show-in-chat",
            call!(
                "clickIn",
                QString::from("mediaTile15"),
                QString::from("showItem")
            )
        );
        record!("shown-in-chat", call!("shownInChat"));
        // Delete, from the picture's tile: the platform's countdown goes
        // up over it, the row stays while it runs, and goes after.
        record!("hurry", call!("hurry", 200));
        record!(
            "delete-picture",
            call!(
                "clickIn",
                QString::from("mediaTile10"),
                QString::from("deleteItem")
            )
        );
        // Nothing is deleted on the tap: the page asks which kind of
        // delete this is, and the wait starts on the answer.
        record!("countdown-before-pick", get!("mediaRemorse", "active"));
        record!("pick-delete", call!("pickDelete", false));
        record!("countdown-up", get!("mediaRemorse", "active"));
        record!("count-while-waiting", get!("media", "count"));
    });

    let page = media_page.clone();
    single_shot(Duration::from_secs(5), move || unsafe {
        record!("count-after-delete", get!("media", "count"));
        record!("video-still-there", get!("mediaTile15", "objectName"));
        // The files list: a document, named and sized.
        record!(
            "load-files",
            call!(
                "loadMedia",
                QString::from(page.clone()),
                2,
                QString::from("files")
            )
        );
    });

    let page = media_page.clone();
    let folder = downloads_for_probe.clone();
    single_shot(Duration::from_secs(7), move || unsafe {
        record!("files-count", get!("media", "count"));
        record!("file-name", get!("fileName", "text"));
        record!("file-size", get!("fileDetail", "text"));
        record!("file-icon", get!("fileIcon", "source"));
        record!("file-caption", get!("rowCaption", "text"));
        call!("setDownloads", QString::from(folder.clone()));
        record!("save", call!("click", QString::from("saveItem")));
        record!("saved-notice", get!("noticeLabel", "text"));
        record!(
            "load-audio",
            call!(
                "loadMedia",
                QString::from(page.clone()),
                2,
                QString::from("audio")
            )
        );
    });

    let page = media_page.clone();
    single_shot(Duration::from_secs(9), move || unsafe {
        record!("audio-count", get!("media", "count"));
        record!("audio-row", get!("mediaRow13", "objectName"));
        record!("audio-label", get!("audioLabel", "text"));
        record!(
            "load-apps",
            call!(
                "loadMedia",
                QString::from(page.clone()),
                2,
                QString::from("apps")
            )
        );
    });

    let page = media_page.clone();
    single_shot(Duration::from_secs(11), move || unsafe {
        record!("apps-count", get!("media", "count"));
        record!("app-name", get!("webxdcName", "text"));
        record!("open-app", call!("click", QString::from("mediaRow14")));
        // A chat with nothing of the kind.
        record!(
            "load-empty",
            call!(
                "loadMedia",
                QString::from(page.clone()),
                1,
                QString::from("gallery")
            )
        );
    });

    let page = contact_page.clone();
    single_shot(Duration::from_secs(13), move || unsafe {
        record!("empty-loaded", get!("media", "loaded"));
        record!("empty-count", get!("media", "count"));
        record!("empty-placeholder", get!("placeholder", "enabled"));
        record!("empty-text", get!("placeholder", "text"));
        // The tiles on the contact page, with apps off.
        record!(
            "load-contact",
            call!("loadDetail", QString::from(page.clone()), 1)
        );
    });

    let tiles_url = tiles.clone();
    let page = group_page.clone();
    single_shot(Duration::from_secs(15), move || unsafe {
        record!("contact-tiles", get!("mediaKinds", "visible"));
        record!("contact-apps", get!("appsTile", "objectName"));
        record!("contact-gallery", get!("galleryTile", "objectName"));
        record!("open-gallery", call!("click", QString::from("galleryTile")));
        // The tiles on their own, with apps on.
        record!(
            "load-tiles",
            call!("loadTiles", QString::from(tiles_url.clone()))
        );
        record!("tiles-apps", get!("appsTile", "objectName"));
        record!("tiles-apps-mark", get!("tileMark", "visible"));
        // And on the group page.
        record!(
            "load-group",
            call!("loadDetail", QString::from(page.clone()), 2)
        );
    });

    single_shot(Duration::from_secs(17), move || unsafe {
        record!("group-tiles", get!("mediaKinds", "visible"));
        record!("open-files", call!("click", QString::from("filesTile")));
        (*engine_ptr).quit();
    });

    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_pages(&steps, &navigation, &common::calls(&journal));
    assert_eq!(
        std::fs::read(downloads.join("notes.pdf")).ok().as_deref(),
        Some(&b"%PDF-1.4 notes"[..]),
        "Save to device did not put a copy of the file in the Downloads folder"
    );
    let _ = std::fs::remove_dir_all(&temp);
}

/// What each page showed, what each tap opened, and what the core was
/// asked.
#[allow(clippy::too_many_lines)]
fn assert_pages(steps: &[(&str, String)], navigation: &str, calls: &[(String, Value)]) {
    let names: Vec<&str> = calls.iter().map(|(name, _)| name.as_str()).collect();
    let context = format!("steps: {steps:?}\nnavigation: {navigation}\ncalls: {names:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };

    for label in [
        "load-gallery",
        "open-picture",
        "open-video",
        "arm-stack",
        "show-in-chat",
        "hurry",
        "delete-picture",
        "pick-delete",
        "load-files",
        "save",
        "load-audio",
        "load-apps",
        "open-app",
        "load-empty",
        "load-contact",
        "open-gallery",
        "load-tiles",
        "load-group",
        "open-files",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    // The gallery: both rows, newest first, each with its file, the
    // video marked as one.
    for (label, expected, complaint) in [
        ("gallery-loaded", "true", "the gallery never loaded"),
        (
            "gallery-count",
            "2",
            "the gallery does not hold the picture and the video",
        ),
        (
            "gallery-busy",
            "false",
            "the gallery is still said to be loading",
        ),
        (
            "gallery-placeholder",
            "false",
            "a gallery with pictures in it says it has none",
        ),
        (
            "first-tile",
            "mediaTile15",
            "the newest item is not the first tile",
        ),
        ("second-tile", "mediaTile10", "the picture has no tile"),
        ("video-mark", "true", "the video is not marked as one"),
        ("picture-mark", "false", "the picture is marked as a video"),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    assert!(
        value("video-thumb").ends_with("clip.mp4"),
        "the video's tile does not show the video. {context}"
    );
    assert!(
        value("picture-thumb").ends_with("photo.jpg"),
        "the picture's tile does not show the picture. {context}"
    );

    // Show in chat told the conversation two pages down which message,
    // and the stack was popped to it; Delete put the platform's
    // countdown up, kept the row while it ran, and the row went once
    // the core had agreed, leaving the other tile where it was.
    for (label, expected, complaint) in [
        (
            "shown-in-chat",
            "15",
            "Show in chat did not tell the conversation which message",
        ),
        (
            // Not "false" but nothing at all: the countdown is built the
            // first time a tile asks for one, and the tap that asks is
            // the answer from the page, not the menu entry.
            "countdown-before-pick",
            "missing:mediaRemorse",
            "Delete started a countdown before the reader had said which \
             kind of delete it is",
        ),
        (
            "countdown-up",
            "true",
            "Delete did not put the platform's countdown up over the tile",
        ),
        (
            "count-while-waiting",
            "2",
            "the picture went before its countdown had run",
        ),
        (
            "count-after-delete",
            "1",
            "the picture is still listed after the core deleted it",
        ),
        (
            "video-still-there",
            "mediaTile15",
            "deleting the picture took the video with it",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    assert!(
        navigation.contains("push:DeleteMessagePage.qml(everyone=false)"),
        "Delete did not ask which kind of delete it is, or offered to \
         delete somebody else's picture for everyone -- which the core \
         refuses. {context}"
    );
    assert!(
        calls.contains(&("delete_messages".to_string(), serde_json::json!([1, [10]]))),
        "the core was not asked to delete the picture. {context}"
    );
    assert!(
        !calls
            .iter()
            .any(|(name, _)| name == "delete_messages_for_all"),
        "the picture was deleted for everybody in the chat, and the \
         reader asked for it to go from their own phone. {context}"
    );

    // The files list: the document, its size, its icon, who sent it, and
    // a copy where the platform says downloads go.
    for (label, expected, complaint) in [
        (
            "files-count",
            "1",
            "the files list does not hold the document",
        ),
        ("file-name", "notes.pdf", "the file is not named"),
        ("file-size", "20.5 kB", "the file's size is not said"),
        (
            "saved-notice",
            "Saved to Downloads",
            "saving did not say where the copy went",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
    assert!(
        value("file-icon").contains("icon-m-file-pdf"),
        "a PDF is not drawn with the theme's PDF icon. {context}"
    );
    assert!(
        value("file-caption").starts_with("Ada Lovelace · "),
        "the row does not say who sent the file and when. {context}"
    );

    // The audio list draws the conversation's own player row, and the
    // apps list the app by the name the core read out of it.
    for (label, expected, complaint) in [
        (
            "audio-count",
            "1",
            "the audio list does not hold the voice message",
        ),
        ("audio-row", "mediaRow13", "the voice message has no row"),
        (
            "audio-label",
            "Voice message",
            "the voice message is not drawn as one",
        ),
        ("apps-count", "1", "the apps list does not hold the app"),
        ("app-name", "Checkers", "the app is not named"),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    // Nothing of a kind: loaded, empty, and said so.
    for (label, expected, complaint) in [
        ("empty-loaded", "true", "an empty gallery never loaded"),
        ("empty-count", "0", "a chat with no pictures has some"),
        (
            "empty-placeholder",
            "true",
            "an empty gallery does not say it is empty",
        ),
        (
            "empty-text",
            "No pictures or videos yet",
            "the empty gallery says the wrong thing",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    // The tiles: on both detail pages, no Apps tile while apps are off,
    // and one on the row that was told apps are on.
    for (label, expected, complaint) in [
        (
            "contact-tiles",
            "true",
            "the contact page has no media tiles",
        ),
        (
            "contact-apps",
            "missing:appsTile",
            "the contact page offers apps while they are off",
        ),
        (
            "contact-gallery",
            "galleryTile",
            "the contact page has no gallery tile",
        ),
        (
            "tiles-apps",
            "appsTile",
            "the tiles offer no apps while they are on",
        ),
        ("tiles-apps-mark", "true", "the apps tile draws no mark"),
        ("group-tiles", "true", "the group page has no media tiles"),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }

    // Every tap opened what it should: a picture on the picture page, a
    // video on the video page, an app on its own page, a tile the media
    // page for its kind -- and Delete the page that asks which kind of
    // delete it is, which is where the conversation asks as well.
    assert_eq!(
        navigation,
        "push:PicturePage.qml()|push:VideoPage.qml()|pop|\
         push:DeleteMessagePage.qml(everyone=false)|push:WebxdcPage.qml()|\
         push:ChatMediaPage.qml(gallery)|push:ChatMediaPage.qml(files)|",
        "a tap did not open the right page, or Show in chat did not pop \
         back to the conversation. {context}"
    );

    // Each page asked the core for its own kinds of the chat's messages.
    let asked: Vec<Value> = calls
        .iter()
        .filter(|(name, _)| name == "get_chat_media")
        .map(|(_, params)| params.clone())
        .collect();
    for wanted in [
        serde_json::json!([1, 2, "Image", "Gif", "Video"]),
        serde_json::json!([1, 2, "File", "Vcard", null]),
        serde_json::json!([1, 2, "Audio", "Voice", null]),
        serde_json::json!([1, 2, "Webxdc", null, null]),
        serde_json::json!([1, 1, "Image", "Gif", "Video"]),
    ] {
        assert!(
            asked.contains(&wanted),
            "the core was not asked for {wanted}. {context}"
        );
    }
}
