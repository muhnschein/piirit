//! Five-fact walkthrough, one per screen; swipe past last = way to setup. Checks: picture
//! shape, facts complete, last offers way on, pull past asks next page once. Look: manual
//! (docs/HARBOUR.md).
//!
//! Phone turn checked because it went wrong on one: view width changes before scrolled content
//! does, which measured as a pull past last fact and carried reader off page untouched. Only a drag
//! asks now, so a turn (or any move reader didn't make) leaves walk where it is.

// Qt harness: `unsafe` for `env::set_var` pre-Qt. Lints: `unused_unsafe` (edition 2024+),
// `borrow_as_ptr`.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::expect_used,
    // `qt_method!` params match dispatcher: piirit-shim/src/lib.rs.
    clippy::needless_pass_by_value
)]

use qmetaobject::*;

mod common;

/// Pictures; shape shader reads.
const PICTURES: [&str; 5] = [
    "intro-profile.png",
    "intro-invite.png",
    "intro-lock.png",
    "intro-group.png",
    "intro-relay.png",
];
/// Smallest: ~2/5 phone width; smaller drawn bigger than painted.
const SMALLEST_PICTURE: u32 = 400;
/// Largest is packaging: `IntroPage` draws at `min(width * 0.42, height * 0.30)`: 453 px on
/// tallest phone, 614 px Jolla Tablet. PNG already compressed: pixels past that cost twice
/// (download, install). Five 1254-square masters: 3.05 MB of 14.3 MB package.
const LARGEST_PICTURE: u32 = 800;

/// Silica `pageStack`, recorded: where walk's end leads, how often.
#[derive(QObject, Default)]
struct PageStackProbe {
    base: qt_base_class!(trait QObject),
    /// `replace:ProfileStartPage.qml|...`
    log: qt_property!(QString; NOTIFY log_changed),
    log_changed: qt_signal!(),

    push: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    replace: qt_method!(fn(&mut self, page: QString, properties: QVariantMap)),
    pop: qt_method!(fn(&mut self)),
}

impl PageStackProbe {
    fn record(&mut self, action: &str, page: &QString) {
        let page = page.to_string();
        let name = page.rsplit('/').next().unwrap_or(&page).to_string();
        let current = self.log.to_string();
        self.log = format!("{current}{action}:{name}|").into();
        self.log_changed();
    }

    fn push(&mut self, page: QString, _properties: QVariantMap) {
        self.record("push", &page);
    }

    fn replace(&mut self, page: QString, _properties: QVariantMap) {
        self.record("replace", &page);
    }

    fn pop(&mut self) {
        self.record("pop", &QString::from("<none>"));
    }
}

/// Phone-size page; thumb-style: one fact at a time, pull past last.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
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
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The picture's file name, without the checkout path.
        function picture(name) {
            var art = findIn(loader.item, name)
            if (!art) { return 'missing:' + name }
            var url = '' + art.source
            return url.substring(url.lastIndexOf('/') + 1)
        }
        function facts() {
            return loader.item ? '' + loader.item.facts.length : 'no-page'
        }
        // A swipe that lands on one fact.
        function swipeTo(index) {
            var view = findIn(loader.item, 'slides')
            if (!view) { return 'missing:slides' }
            view.contentX = parseInt(index) * view.width
            return '' + view.currentIndex
        }
        // The view carried past the last fact without a finger on it,
        // which is what a turn of the phone amounts to.
        function pullPastEnd(extra) {
            var view = findIn(loader.item, 'slides')
            if (!view) { return 'missing:slides' }
            view.contentX = view.contentWidth - view.width + parseFloat(extra)
            return 'ok'
        }
        // The phone turned, and turned back.
        function turn(width, height) {
            if (!loader.item) { return 'no-page' }
            loader.item.width = parseInt(width)
            loader.item.height = parseInt(height)
            return 'ok'
        }
        // What a drag past the last fact ends in, which is the only way
        // a page can be asked for from here.
        function pullByHand() {
            if (!loader.item) { return 'no-page' }
            loader.item.advanceIfPastEnd()
            return 'ok'
        }
        // Where the walk has navigated to, as the probe recorded it.
        function navigation() { return '' + pageStack.log }
    }
";

/// Pictures: shader reads two channels of 8-bit RGB PNG, square, not interlaced. Gray or alpha
/// re-export loses accents or tints them wrong;
/// non-square stretches (`InkArt` no letterbox). Size bounded both ends: under floor drawn
/// bigger than painted; over ceiling is weight no phone shows.
#[test]
fn the_intro_pictures_are_the_shape_the_shader_reads() {
    for file in PICTURES {
        let (width, height, depth, colour, interlace) = common::png_header(file);
        assert_eq!(depth, 8, "qml/art/{file} is not 8 bits per channel");
        assert_eq!(
            colour, 2,
            "qml/art/{file} is not RGB: the shader reads red and green"
        );
        assert_eq!(interlace, 0, "qml/art/{file} is interlaced");
        assert_eq!(
            width, height,
            "qml/art/{file} is {width}x{height}, and a picture that is not \
             square is drawn stretched"
        );
        assert!(
            width >= SMALLEST_PICTURE,
            "qml/art/{file} is only {width} square; a phone would draw it \
             bigger than it was painted"
        );
        assert!(
            width <= LARGEST_PICTURE,
            "qml/art/{file} is {width} square; no phone draws one bigger \
             than {LARGEST_PICTURE}, so the rest is package weight"
        );
    }
}

#[test]
fn the_introduction_walks_the_facts_and_ends_in_the_setup_path() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let stack_box = QObjectBox::new(PageStackProbe::default());

    let mut engine = QmlEngine::new();
    piirit_shim::install_pictures(&engine);
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("pageStack".into(), stack_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    // SAFETY: callbacks run only inside `exec()`; `engine` outlives it.
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
        r("load", call!("load", common::page_url("IntroPage.qml")));
        r("facts", call!("facts"));
        r("first-picture", call!("picture", "slideArt0"));
    });

    // Pictures load off main thread.
    let r = record.clone();
    plan.after(1, move || {
        r("drawn", call!("get", "slideArt0", "ready"));
        r("title", call!("get", "slideTitle0", "text"));
        r("body", call!("get", "slideBody0", "text"));
        r("hint-early", call!("get", "swipeHint", "visible"));
        r("swipe", call!("swipeTo", "4"));
        r("hint-late", call!("get", "swipeHint", "visible"));
    });

    // Last fact read only now (views make delegates near). Past-end: phone turn is no request,
    // drag is; second drag must not ask again.
    let r = record.clone();
    plan.after(1, move || {
        r("last-picture", call!("picture", "slideArt4"));
        r("turn", call!("turn", "2520", "1080"));
        r("after-turn", call!("navigation"));
        r("turn-back", call!("turn", "1080", "2520"));
        r("after-turn-back", call!("navigation"));
        r("pull", call!("pullPastEnd", "240"));
        r("after-pull", call!("navigation"));
        r("by-hand", call!("pullByHand"));
        r("after-hand", call!("navigation"));
        r("by-hand-again", call!("pullByHand"));
    });

    plan.after(1, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let navigation = stack_box.pinned().borrow().log.to_string();
    assert_walk(&steps.borrow(), &navigation);
}

/// Each fact: picture and words; last offers way on; pull past asks setup once.
fn assert_walk(steps: &[(String, String)], navigation: &str) {
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
        "the introduction did not load. {context}"
    );
    assert_eq!(
        value("facts"),
        "5",
        "the introduction is not five facts long. {context}"
    );
    assert_eq!(
        value("first-picture"),
        "intro-profile.png",
        "the first fact does not draw its own picture. {context}"
    );
    assert_eq!(
        value("last-picture"),
        "intro-relay.png",
        "the last fact does not draw its own picture. {context}"
    );
    assert_eq!(
        value("drawn"),
        "true",
        "a picture never loaded, so the fact is words alone. {context}"
    );
    assert!(
        !value("title").is_empty() && !value("body").is_empty(),
        "a fact was drawn without anything to say. {context}"
    );
    assert_eq!(
        value("hint-early"),
        "false",
        "the way on was offered before the last fact. {context}"
    );
    assert_eq!(
        value("swipe"),
        "4",
        "a swipe to the last fact did not land on it. {context}"
    );
    assert_eq!(
        value("hint-late"),
        "true",
        "the last fact does not say what one more swipe does. {context}"
    );
    assert_eq!(
        value("after-turn"),
        "",
        "turning the phone on the last fact left the walk on its own. \
         {context}"
    );
    assert_eq!(
        value("after-turn-back"),
        "",
        "turning the phone back left the walk on its own. {context}"
    );
    assert_eq!(
        value("after-pull"),
        "",
        "the view carried past the end with no finger on it asked for the \
         next page. {context}"
    );
    assert_eq!(
        value("after-hand"),
        "replace:ProfileStartPage.qml|",
        "a pull past the last fact did not land in the setup path. {context}"
    );
    assert_eq!(
        navigation, "replace:ProfileStartPage.qml|",
        "pulling past the last fact asked for the setup path more than \
         once. {context}"
    );
}
