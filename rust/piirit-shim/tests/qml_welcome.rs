//! The first screen: the app's mark over its name, and the ways on.
//!
//! The mark is the launcher icon drawn out into a picture (`qml/art/`);
//! what is checked here is that the picture is there in a shape a phone
//! can draw, that the page loads it and puts it over the name, and that
//! none of it is drawn before the core has said whether there is a chat
//! list to be on instead. What it looks like is a manual check
//! (docs/HARBOUR.md).

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
    // by-value parameters; see piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use std::ffi::CString;
use std::path::PathBuf;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
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
    // The screen, with the page in it at a phone's size.
    Item {
        id: probe
        width: 1080
        height: 2520
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 1080, height: 2520 })
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
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // A property of a part of a named item: the tiles each hold a
        // part of every name, so a part is asked for through its tile.
        function partOf(name, part, property) {
            var item = findIn(findIn(loader.item, name), part)
            if (!item) { return 'missing:' + name + '/' + part }
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
        // The mark's file name, without the checkout path.
        function logoFile() {
            var logo = findIn(loader.item, 'logo')
            if (!logo) { return 'missing:logo' }
            var url = '' + logo.source
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
        // Whether the mark stands over the name: drawn, above it, and
        // on the same centre line.
        function logoOverTitle() {
            var logo = findIn(loader.item, 'logo')
            var title = findIn(loader.item, 'title')
            if (!logo || !title) { return 'missing' }
            var above = logo.height > 0 && logo.y + logo.height <= title.y
            var centred = Math.abs((logo.x + logo.width / 2)
                                   - (title.x + title.width / 2)) < 1
            return '' + (above && centred)
        }
        // Whether anything of the first screen is drawn.
        function wordsShown() {
            var title = findIn(loader.item, 'title')
            if (!title) { return 'missing:title' }
            return '' + title.parent.visible
        }
    }
";

/// The smallest the mark may be painted: it is drawn at a large item's
/// size, which on a phone is short of this, so anything smaller would be
/// drawn bigger than it was painted.
const SMALLEST_LOGO: u32 = 256;

/// The mark is the launcher icon drawn out: an 8-bit PNG with an alpha
/// channel, since it stands on the ambience rather than on a colour of
/// its own, square, not interlaced, and not small.
#[test]
fn the_logo_is_the_launcher_icon_drawn_out() {
    let (width, height, depth, colour, interlace) = common::png_header("logo.png");
    assert_eq!(depth, 8, "qml/art/logo.png is not 8 bits per channel");
    assert_eq!(
        colour, 6,
        "qml/art/logo.png is not RGBA: the mark stands on the ambience, \
         so it needs its alpha channel"
    );
    assert_eq!(interlace, 0, "qml/art/logo.png is interlaced");
    assert_eq!(
        width, height,
        "qml/art/logo.png is {width}x{height}, and the launcher icon is square"
    );
    assert!(
        width >= SMALLEST_LOGO,
        "qml/art/logo.png is only {width} square; a phone would draw it \
         bigger than it was painted"
    );
}

#[test]
fn the_welcome_page_leads_with_the_mark_and_the_name() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-welcome-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    // SAFETY: single-threaded, and set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    // Never started: the page is what is under test, and it shows its
    // words only once the core has answered -- so the first screen is
    // checked with the core in its error state, which the page says
    // under the tiles as it shows everything else.
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
        r("logo-file", call!("logoFile"));
        // Nothing is drawn until the core has said whether there is a
        // profile to resume: a phone that has one goes straight to the
        // chat list, and a first screen on the way reads as the app
        // opening in the wrong place.
        r("probing-words", call!("wordsShown"));
        r("probing-spinner", call!("get", "probeSpinner", "running"));
        r("probed", call!("endProbe"));
    });

    // A second is plenty for the picture to load.
    let r = record.clone();
    single_shot(Duration::from_secs(2), move || {
        r("logo", call!("get", "logo", "status"));
        r("logo-over-title", call!("logoOverTitle"));
        r("title", call!("get", "title", "text"));
        r(
            "setup-mark",
            call!("partOf", "setupTile", "tileMark", "visible"),
        );
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
    assert_first_screen(&steps.borrow(), &navigation);
    assert_remembered_profile_opens(&steps.borrow(), &navigation);
    assert_a_refused_hand_over_is_survived(&steps.borrow(), &navigation);
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
         phone with a profile shows the whole first screen -- mark, \
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

/// The page draws the mark, loaded, over the name, and the name is the
/// app's own; and none of it before the core has answered.
fn assert_first_screen(steps: &[(String, String)], navigation: &str) {
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
        value("logo-file"),
        "logo.png",
        "the page does not draw the launcher icon's picture. {context}"
    );
    assert_eq!(
        value("probing-words"),
        "false",
        "the first screen is drawn before the core has said whether there \
         is a profile, so a phone with one flashes the welcome on its way \
         to the chat list. {context}"
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
    // Image.Ready is 1.
    assert_eq!(
        value("logo"),
        "1",
        "the mark never loaded, so the first screen leads with a hole. \
         {context}"
    );
    assert_eq!(
        value("logo-over-title"),
        "true",
        "the mark does not stand over the name. {context}"
    );
    assert_eq!(
        value("title"),
        "Piirit",
        "the app's name is not what the page leads with. {context}"
    );
    assert_eq!(
        value("setup-mark"),
        "true",
        "the way into a profile is not under the drawn account mark, so \
         it is on the theme for an icon that may not be there. {context}"
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
