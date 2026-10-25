//! First screen: app mark over name, and ways on.
//! Mark is launcher icon drawn out (`qml/art/`). Checked: picture present in a shape a phone
//! draws, page loads it over name, nothing drawn before core says whether a chat list comes
//! instead. Look: manual check (docs/HARBOUR.md).

// Qt harness: `unsafe` `env::set_var` before Qt starts (`unused_unsafe`: unsafe only from
// edition 2024), `borrow_as_ptr` for engine pointer.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    // Match dispatcher's by-value params (`piirit-shim/src/lib.rs`).
    clippy::needless_pass_by_value
)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use std::ffi::CString;
use std::path::PathBuf;

mod common;

/// `BusyIndicatorSize.Large`: `.qml` stubs lack it.
#[derive(QEnum)]
#[repr(u8)]
enum BusyIndicatorSize {
    Small = 0,
    Medium = 1,
    Large = 2,
}

/// Loads page at phone size, reads it back; components dir substituted, not formatted, so QML
/// braces need no escaping.
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

/// Smallest mark may be painted: drawn at large item size, on phone short of this; smaller
/// would be drawn bigger than painted.
const SMALLEST_LOGO: u32 = 256;

/// Launcher icon drawn out: 8-bit PNG with alpha (stands on ambience), square, not
/// interlaced, not small.
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
    // SAFETY: single-threaded; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
    }

    // Core never started: page shows words only once core answered, so first screen is checked
    // with core in error state, which page says under tiles.
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
    // SAFETY: callbacks fire only in `exec()`; `engine` outlives it.
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

    let mut plan = common::Plan::default();

    let r = record.clone();
    plan.after(1, move || {
        // dconf outlives binary: old state would decide.
        r("fresh", call!("rememberProfile", "0"));
        r("load", call!("load", common::page_url("WelcomePage.qml")));
        r("logo-file", call!("logoFile"));
        // Nothing drawn until core says whether a profile resumes: phone with one goes to chat
        // list, and first screen flashing on the way reads as opening in wrong place.
        r("probing-words", call!("wordsShown"));
        r("probing-spinner", call!("get", "probeSpinner", "running"));
        r("probed", call!("endProbe"));
    });

    // A second is plenty for picture load.
    let r = record.clone();
    plan.after(1, move || {
        r("logo", call!("get", "logo", "status"));
        r("logo-over-title", call!("logoOverTitle"));
        r("title", call!("get", "title", "text"));
        r(
            "setup-mark",
            call!("partOf", "setupTile", "tileMark", "visible"),
        );
    });

    // 3s: stack refusing, as real one does mid-push of this page. Page must neither record a
    // hand-over that didn't happen nor draw itself because refused; core's answer must reach it.
    let r = record.clone();
    plan.after(1, move || {
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

    // 4s: long stop passed: stack refusing that long won't take it; usable screen beats blank.
    //
    // Then same page on phone remembering a profile, over accepting stack. Not told core is
    // ready, and should not wait: dconf is enough to leave on.
    let r = record.clone();
    plan.after(1, move || {
        r("refused-gave-up", call!("pageProperty", "probing"));
        r("accept", call!("refuseNavigation", "false"));
        r("remember", call!("rememberProfile", "7"));
        r("reload", call!("load", common::page_url("WelcomePage.qml")));
    });

    // 5s: did it leave? Read a beat later: offer made from a timer (so it can repeat), whose
    // first turn comes after page build.
    let r = record.clone();
    plan.after(1, move || {
        r("left", call!("pageProperty", "leaving"));
    });

    plan.after(1, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_first_screen(&steps.borrow(), &navigation);
    assert_remembered_profile_opens(&steps.borrow(), &navigation);
    assert_a_refused_hand_over_is_survived(&steps.borrow(), &navigation);
}

/// Refused hand-over must leave page working. Silica drops a stack op mid-transition, and
/// page asks from inside the push putting it up. Both misreadings shipped: as left = page
/// hidden for good (blank); as end of attempt = whole first screen flashed before chat list.
/// It is one refusal: offer made again.
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

/// Remembered profile leaves without core-ready wait (process spawn + round trip): that
/// showed as empty screen before chat list.
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

/// Page draws mark over app name once loaded; nothing before core.
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

/// Stack records reader's destination; can refuse mid-transition.
#[allow(non_snake_case)]
#[derive(QObject, Default)]
struct NoStack {
    base: qt_base_class!(trait QObject),
    /// `replaceAbove:ChatListPage.qml|`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),
    /// Drops request, returns nothing, like busy stack. Every refusal still counted in `log`.
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
            // Any object will do: page only checks it got one.
            QString::from(name).to_qvariant()
        }
    }
}
