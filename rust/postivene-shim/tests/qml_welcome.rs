//! The first screen's field of faces.
//!
//! The field is a picture (`qml/art/`), tinted by a shader in
//! `components/FaceField.qml`; what is checked here is that the picture
//! is there in the shape the shader reads -- an 8-bit RGB PNG, one per
//! orientation, at a phone's size -- that the page loads it and swaps
//! it with the orientation, and that the words sit over a cleared box
//! that follows them. What it looks like is a manual check: `make
//! faces`, then look at what it wrote.

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
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

use std::ffi::CString;
use std::path::PathBuf;
use std::time::Duration;

use postivene_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// `BusyIndicatorSize.Large`, which a `.qml` stub cannot provide.
#[derive(QEnum)]
#[repr(u8)]
enum BusyIndicatorSize {
    Small = 0,
    Medium = 1,
    Large = 2,
}

/// The probe that loads the page at a phone's size and reads it back,
/// with the components directory filled in. Substituted rather than
/// formatted, so the QML's own braces need no escaping.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import 'file://__COMPONENTS__'
    // The screen, with the page in it: a page is not the whole of what
    // it is drawn on, and on a phone that keeps a band for its camera it
    // is a band short of it. `frame` is what holds the page, so a test
    // can move it in the way that band does.
    Item {
        id: probe
        width: 1080
        height: 2520
        Item {
            id: frame
            width: probe.width
            height: probe.height
            Loader { id: loader }
        }
        function load(url) {
            loader.setSource(url, { width: 1080, height: 2520 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // The screen this size, with the page put in it short of a band
        // of `band` pixels down one edge -- moved off that edge, and
        // that much narrower, which is what the page is given on a phone
        // with a camera cutout.
        function cutout(width, height, band) {
            probe.width = parseInt(width, 10)
            probe.height = parseInt(height, 10)
            frame.x = parseInt(band, 10)
            frame.width = probe.width - frame.x
            frame.height = probe.height
            if (!loader.item) { return 'no-page' }
            loader.item.width = frame.width
            loader.item.height = frame.height
            return 'ok'
        }
        // Where the field lies on the screen, as `x,y,width,height`.
        function fieldBox() {
            var field = findIn(loader.item, 'faceField')
            if (!field) { return 'missing:faceField' }
            var at = probe.mapFromItem(field, 0, 0)
            return Math.round(at.x) + ',' + Math.round(at.y) + ','
                   + Math.round(field.width) + ',' + Math.round(field.height)
        }
        function turn(width, height) {
            loader.item.width = parseInt(width)
            loader.item.height = parseInt(height)
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
        function pageProperty(property) {
            return loader.item ? '' + loader.item[property] : 'no-page'
        }
        // Whether the stack takes what it is handed.
        function refuseNavigation(on) {
            pageStack.refusing = (on === 'true')
            return 'ok'
        }
        // What the core saying there is a profile to resume does.
        function coreFoundProfiles(count) {
            core.accounts_refreshed(parseInt(count, 10), 4)
            return 'ok'
        }
        // The mask's file name, without the checkout path.
        function maskFile() {
            var mask = findIn(loader.item, 'faceMask')
            if (!mask) { return 'missing:faceMask' }
            var url = '' + mask.source
            return url.substring(url.lastIndexOf('/') + 1)
        }
        // What this phone remembers about the profile it was last on.
        function rememberProfile(id) {
            Settings.lastAccountId = parseInt(id, 10)
            return 'ok'
        }
        // The long stop, turned down: a test that waited the real four
        // seconds out would be four seconds of waiting.
        function hurry(ms) {
            if (!loader.item) { return 'no-page' }
            loader.item.handOverDeadline = parseInt(ms, 10)
            return 'ok'
        }
        // What the core answering with no profiles does.
        function endProbe() {
            if (!loader.item) { return 'no-page' }
            loader.item.probing = false
            return 'ok'
        }
        // Whether the cleared box is the column of words.
        function clearsTheWords() {
            var field = findIn(loader.item, 'faceField')
            var title = findIn(loader.item, 'title')
            if (!field || !title) { return 'missing' }
            var words = title.parent
            // The box is cut in the field, and the field starts where
            // the page does only where nothing is in its way.
            var centred =
                Math.abs(field.x + field.clearX - (words.x + words.width / 2)) < 1
                && Math.abs(field.y + field.clearY - (words.y + words.height / 2)) < 1
            var sized = field.clearWidth === words.width
                        && field.clearHeight === words.height
            return '' + (centred && sized && words.height > 0)
        }
    }
";

fn art_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/art")
}

/// Width, height, bit depth, colour type and interlacing, off the
/// header chunk every PNG starts with.
fn png_header(file: &str) -> (u32, u32, u8, u8, u8) {
    let bytes = std::fs::read(art_dir().join(file))
        .unwrap_or_else(|err| panic!("qml/art/{file} is missing ({err}); it is committed art"));
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "qml/art/{file} is not a PNG"
    );
    assert_eq!(
        &bytes[12..16],
        b"IHDR",
        "qml/art/{file} does not start with IHDR"
    );
    let at = |offset: usize| {
        u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    (at(16), at(20), bytes[24], bytes[25], bytes[28])
}

/// A phone's short side. The masters are only ever scaled down, so
/// anything narrower than this would be drawn bigger than it was
/// painted.
const SHORT_SIDE: u32 = 800;

/// The masks are what the shader reads: two channels of an 8-bit RGB
/// PNG, not interlaced (Qt loads either, but a mask re-exported as
/// grayscale or with an alpha channel would tint the field wrong), one
/// master per orientation and neither of them small.
///
/// The exact size is not pinned: the field is redrawn from time to time
/// and the shader crops rather than stretches, so what matters is the
/// shape of the channels, which way up each master runs, and that there
/// are pixels enough.
#[test]
fn the_face_masks_are_the_shape_the_shader_reads() {
    for (file, upright) in [("faces-portrait.png", true), ("faces-landscape.png", false)] {
        let (width, height, depth, colour, interlace) = png_header(file);
        assert_eq!(depth, 8, "qml/art/{file} is not 8 bits per channel");
        assert_eq!(
            colour, 2,
            "qml/art/{file} is not RGB: the shader reads red and green"
        );
        assert_eq!(interlace, 0, "qml/art/{file} is interlaced");
        let (long, short) = if upright {
            (height, width)
        } else {
            (width, height)
        };
        assert!(
            long > short,
            "qml/art/{file} is {width}x{height}, which is not the master \
             for the orientation it is named after"
        );
        assert!(
            short >= SHORT_SIDE,
            "qml/art/{file} is only {short} across its short side; a phone \
             would draw it bigger than it was painted"
        );
    }
}

#[test]
fn the_welcome_page_draws_the_field_and_turns_with_the_phone() {
    let temp = std::env::temp_dir().join(format!("postivene-qml-welcome-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // SAFETY: single-threaded, and set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("POSTIVENE_ACCOUNTS_DIR", temp.join("accounts"));
    }

    // Never started: the page is what is under test, and it shows its
    // words only once the core has answered -- so the field is checked
    // with the core in its error state, which the page shows over the
    // field as it shows everything else.
    let core_box = QObjectBox::new(DeltaChatCore::default());
    let stack_box = QObjectBox::new(NoStack::default());

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    let uri = CString::new("Sailfish.Silica").expect("static uri");
    qml_register_enum::<BusyIndicatorSize>(
        &uri,
        1,
        0,
        &CString::new("BusyIndicatorSize").expect("static name"),
    );
    engine.set_object_property("core".into(), core_box.pinned());
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(probe_qml().as_str()));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    // SAFETY: these callbacks fire only while `exec()` is running on this
    // thread, and `engine` outlives it.
    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = unsafe {
                (*engine_ptr).invoke_method(
                    $name.into(),
                    &[$(QVariant::from(QString::from($arg))),*],
                )
            };
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    let steps = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(String, String)>::new()));
    let record = {
        let steps = steps.clone();
        move |label: &str, value: QString| {
            steps
                .borrow_mut()
                .push((label.to_string(), value.to_string()));
        }
    };

    let r = record.clone();
    single_shot(Duration::from_secs(1), move || {
        // dconf outlives the test binary, so what a previous run left
        // behind would otherwise decide what this page does.
        r("fresh", call!("rememberProfile", "0"));
        r("load", call!("load", common::page_url("WelcomePage.qml")));
        r("upright", call!("maskFile"));
        // Nothing is drawn until the core has said whether there is a
        // profile to resume: a phone that has one goes straight to the
        // chat list, and a screenful of faces on the way reads as the
        // app opening in the wrong place.
        r("probing-field", call!("get", "faceField", "visible"));
        r("probing-spinner", call!("get", "probeSpinner", "running"));
        r("probed", call!("endProbe"));
    });

    // The mask loads off the main thread; a second is plenty.
    let r = record.clone();
    single_shot(Duration::from_secs(2), move || {
        r("mask", call!("get", "faceMask", "status"));
        r("shader", call!("get", "faceShader", "visible"));
        r("clears", call!("clearsTheWords"));
        r("title", call!("get", "title", "text"));
        r("turn", call!("turn", "2520", "1080"));
        r("sideways", call!("maskFile"));
        // Upright again, and then turned by handing the page a sideways
        // screen: one it has all of, and one where a band down the edge
        // belongs to the camera. A page is told about the band by being
        // given less, which is the only way it hears of it.
        r("upright-again", call!("cutout", "1080", "2520", "0"));
        r("whole", call!("cutout", "2520", "1080", "0"));
        r("whole-box", call!("fieldBox"));
        r("cutout", call!("cutout", "2520", "1080", "120"));
        r("cutout-box", call!("fieldBox"));
    });

    // 3s: a stack that refuses, which is what the real one does while
    // the push that puts this page up is still running. The page must
    // neither record a hand-over that did not happen nor draw itself
    // because one was refused, and the core's answer has to reach it.
    let r = record.clone();
    single_shot(Duration::from_secs(3), move || {
        r("refuse", call!("refuseNavigation", "true"));
        r("remember", call!("rememberProfile", "7"));
        r(
            "reload-refused",
            call!("load", common::page_url("WelcomePage.qml")),
        );
        r("hurry", call!("hurry", "300"));
        r("refused-left", call!("pageProperty", "leaving"));
        r("refused-probing", call!("pageProperty", "probing"));
        r("refused-core", call!("coreFoundProfiles", "2"));
        r("refused-after-core", call!("pageProperty", "probing"));
    });

    // 4s: the long stop has passed. A stack that has refused for that
    // long is not going to take it, and a reader is better off on a
    // screen they can use than on a blank one.
    //
    // Then the same page on a phone that remembers being on a profile,
    // over a stack that takes what it is handed. Nothing has told this
    // page the core is ready, and it should not wait to be told: what
    // it knows from dconf is enough to leave on.
    let r = record.clone();
    single_shot(Duration::from_secs(4), move || {
        r("refused-gave-up", call!("pageProperty", "probing"));
        r("accept", call!("refuseNavigation", "false"));
        r("remember", call!("rememberProfile", "7"));
        r("reload", call!("load", common::page_url("WelcomePage.qml")));
    });

    // 5s: whether it left. Read a beat later rather than in the same
    // breath as the load: the offer is made from a timer, so that it can
    // be made again, and a timer's first turn comes after the one the
    // page was built in.
    let r = record.clone();
    single_shot(Duration::from_secs(5), move || {
        r("left", call!("pageProperty", "leaving"));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_field_drawn(&steps.borrow(), &navigation);
    assert_the_field_covers_the_screen(&steps.borrow());
    assert_remembered_profile_opens(&steps.borrow(), &navigation);
    assert_a_refused_hand_over_is_survived(&steps.borrow(), &navigation);
}

/// The field is the screen's, not the page's.
///
/// A Silica page is centred in what holds it, and on a phone that keeps
/// a band of its screen for the camera the page is that band short of
/// the screen: upright it is a strip across the top, turned on its side
/// it is a strip down the edge the camera is on. A field that fills the
/// page leaves that strip bare, which on the first screen is the first
/// thing the reader sees.
fn assert_the_field_covers_the_screen(steps: &[(String, String)]) {
    let value = |label: &str| -> &str {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map_or("<step did not run>", |(_, value)| value.as_str())
    };
    let context = format!("steps: {steps:?}");
    assert_eq!(
        value("upright-again"),
        "ok",
        "the page could not be put back upright. {context}"
    );
    assert_eq!(
        value("whole"),
        "ok",
        "the page could not be put on a screen of its own. {context}"
    );
    assert_eq!(
        value("whole-box"),
        "0,0,2520,1080",
        "the field does not cover a screen the page has all of. {context}"
    );
    assert_eq!(
        value("cutout"),
        "ok",
        "the page could not be put on a screen with a cutout. {context}"
    );
    assert_eq!(
        value("cutout-box"),
        "0,0,2520,1080",
        "the page was moved off the edge the camera is on and the field \
         went with it, leaving the band beside it bare. {context}"
    );
}

/// A hand-over the stack refuses must leave the page working.
///
/// Silica drops a stack operation asked for while a transition is
/// running, and the first thing this page does is ask, from inside the
/// push that puts it up. Both ways of reading that one refusal were
/// wrong and both were shipped: recording it as having left hid the page
/// for a hand-over that never happened -- a blank screen, for good --
/// and taking it as the end of the attempt drew the whole first screen,
/// buttons and all, for the half second before the chat list arrived.
/// It is neither. It is one refusal, and the offer is made again.
fn assert_a_refused_hand_over_is_survived(steps: &[(String, String)], navigation: &str) {
    let value = |label: &str| -> &str {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map_or("<step did not run>", |(_, value)| value.as_str())
    };
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");
    assert_eq!(
        value("reload-refused"),
        "ok",
        "the page did not load against a refusing stack. {context}"
    );
    assert_eq!(
        value("refused-left"),
        "false",
        "the page recorded a hand-over the stack refused, which hides it \
         for good. {context}"
    );
    assert!(
        navigation.contains("refused:ChatListPage.qml"),
        "the page did not even try to hand over. {context}"
    );
    assert_eq!(
        value("refused-probing"),
        "true",
        "the page drew itself the moment one hand-over was refused, so a \
         phone with a profile shows the whole first screen -- field, \
         name, buttons -- on its way to the chat list. {context}"
    );
    assert_eq!(
        value("refused-core"),
        "ok",
        "the core's answer could not be delivered. {context}"
    );
    assert_eq!(
        value("refused-after-core"),
        "true",
        "the core naming a profile ended the attempt instead of feeding \
         it: the stack refusing this instant says nothing about the \
         next, and the page drew itself rather than ask again. {context}"
    );
    assert!(
        navigation.matches("refused:ChatListPage.qml").count() > 1,
        "the page offered the hand-over once and gave up. The one moment \
         a stack will not take anything is the push that puts this page \
         up, which is exactly when the first offer is made. {context}"
    );
    assert_eq!(
        value("refused-gave-up"),
        "false",
        "the page is still hiding itself long after the stack stopped \
         taking anything, which leaves the reader on a blank screen with \
         nothing coming. {context}"
    );
}

/// A phone that remembers a profile leaves for it without waiting to be
/// told the core is ready: that wait is a process spawn and a round
/// trip, and it showed as an empty screen before the chat list.
fn assert_remembered_profile_opens(steps: &[(String, String)], navigation: &str) {
    let value = |label: &str| -> &str {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map_or("<step did not run>", |(_, value)| value.as_str())
    };
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");
    assert_eq!(
        value("remember"),
        "ok",
        "the remembered profile could not be written. {context}"
    );
    assert_eq!(
        value("reload"),
        "ok",
        "the page did not load a second time. {context}"
    );
    assert_eq!(
        value("left"),
        "true",
        "the page stayed put on a phone that remembers a profile, which \
         leaves the reader on an empty screen until the core answers. \
         {context}"
    );
    assert!(
        navigation.contains("replaceAbove:ChatListPage.qml"),
        "the page did not open the chat list it remembered. {context}"
    );
}

/// The page upright draws the portrait master, the mask loads and the
/// shader shows over it, the box is cleared where the words are, the
/// name leads, and the page on its side swaps to the landscape master.
fn assert_field_drawn(steps: &[(String, String)], navigation: &str) {
    let value = |label: &str| -> &str {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map_or("<step did not run>", |(_, value)| value.as_str())
    };
    let context = format!("steps: {steps:?}\nnavigation: {navigation}");

    assert_eq!(
        value("load"),
        "ok",
        "the welcome page did not load. {context}"
    );
    assert_eq!(
        value("upright"),
        "faces-portrait.png",
        "the page upright does not draw the portrait master. {context}"
    );
    // Image.Ready is 1.
    assert_eq!(
        value("mask"),
        "1",
        "the mask never loaded, so the field is not drawn. {context}"
    );
    assert_eq!(
        value("probing-field"),
        "false",
        "the field is drawn before the core has said whether there is a \
         profile, so a phone with one flashes the welcome on its way to \
         the chat list. {context}"
    );
    assert_eq!(
        value("probing-spinner"),
        "false",
        "the spinner is up the moment the page appears, so a phone that \
         is about to leave for its chat list flashes one on the way out. \
         {context}"
    );
    assert_eq!(
        value("probed"),
        "ok",
        "the probe could not be ended. {context}"
    );
    assert_eq!(
        value("shader"),
        "true",
        "the shader is not shown once the mask is there. {context}"
    );
    assert_eq!(
        value("clears"),
        "true",
        "the field is not cleared where the words are. {context}"
    );
    assert_eq!(
        value("title"),
        "Postivene",
        "the app's name is not what the page leads with. {context}"
    );
    assert_eq!(
        value("sideways"),
        "faces-landscape.png",
        "the page on its side does not swap to the landscape master. {context}"
    );
}

/// A page stack that records where the page sent the reader, and can
/// refuse -- which is what Silica's own does while a transition is
/// running, including the push that puts the first page up.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct NoStack {
    base: qt_base_class!(trait QObject),
    /// `replaceAbove:ChatListPage.qml|`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// Drop what is asked for and hand back nothing, as a busy stack
    /// does. Every refusal is still counted in `log`.
    refusing: qt_property!(bool),

    replaceAbove: qt_method!(
        fn(&mut self, target: QVariant, page: QString, properties: QVariantMap) -> QVariant
    ),
}

#[allow(non_snake_case)]
impl NoStack {
    fn replaceAbove(
        &mut self,
        _target: QVariant,
        page: QString,
        _properties: QVariantMap,
    ) -> QVariant {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let current = self.log.to_string();
        let verb = if self.refusing {
            "refused"
        } else {
            "replaceAbove"
        };
        self.log = format!("{current}{verb}:{name}|").into();
        self.log_changed();
        if self.refusing {
            QVariant::default()
        } else {
            // Any object will do: the page only asks whether it got one.
            QString::from(name).to_qvariant()
        }
    }
}
