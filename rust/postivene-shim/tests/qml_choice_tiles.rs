//! The row of tiles the onboarding pages ask their questions with.
//!
//! What is checked here is the row on its own: that every choice becomes
//! a tile named after it, drawn with the theme icon and the words it was
//! given, that a choice with nothing more to say draws no second line,
//! that the tiles stand side by side at one height however much each of
//! them says, that a choice that is off is off, and that tapping one
//! says which was tapped. Where the tiles go is each page's business and
//! is checked with the page (`qml_pages.rs`, `qml_restore.rs`).

// Qt harness: needs `unsafe` for `env::set_var` before Qt starts
// (`unused_unsafe` because it is only unsafe from edition 2024 on),
// `borrow_as_ptr` for the engine pointer, and `single_shot` with
// whole-second Durations.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use qmetaobject::*;

mod common;

/// The row, loaded at a phone's width. The Loader is given a width and
/// not a height: the row is as tall as its tiles need, and that is one
/// of the things being read back.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: probe
        width: 1080
        height: 2520
        property string picked: ''
        Loader { id: loader; width: probe.width }
        function load(url) {
            loader.setSource(url, {})
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            probe.picked = ''
            loader.item.chosen.connect(function (name) {
                probe.picked += name + ';'
            })
            return 'ok'
        }
        // The choices as a page writes them, over the wire as JSON.
        function offer(choices) {
            if (!loader.item) { return 'no-row' }
            loader.item.choices = JSON.parse(choices)
            return 'ok'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
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
        function own(property) {
            return loader.item ? '' + loader.item[property] : 'no-row'
        }
        // What a tile has under it, by the name the row gives it: the
        // icon's source, the words, and whether the quiet line is there.
        function partOf(tile, part, property) {
            var item = findIn(loader.item, tile)
            if (!item) { return 'missing:' + tile }
            var found = findIn(item, part)
            if (!found) { return 'missing:' + tile + '/' + part }
            return '' + found[property]
        }
        // Where a tile stands and how big it is: x,y,width,height.
        function boxOf(tile) {
            var item = findIn(loader.item, tile)
            if (!item) { return 'missing:' + tile }
            return Math.round(item.x) + ',' + Math.round(item.y) + ','
                   + Math.round(item.width) + ',' + Math.round(item.height)
        }
        // One under another rather than side by side.
        function stack(on) {
            if (!loader.item) { return 'no-row' }
            loader.item.stacked = (on === 'true')
            return 'ok'
        }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        function pickedSoFar() { return probe.picked }
    }
";

/// Two ways on, as a page writes them: one of them with a second line
/// and the other without, one of them off.
const CHOICES: &str = r#"[
    { "name": "about", "icon": "icon-m-about", "text": "Tell me about Delta Chat" },
    { "name": "setup", "icon": "icon-m-person", "text": "Set up my profile",
      "hint": "A new address, on a relay that carries chat mail and nothing else at all.",
      "enabled": false }
]"#;

/// Three ways in, as the page behind the profiles plus writes them: too
/// many to stand side by side on a phone.
const THREE: &str = r#"[
    { "name": "createProfile", "icon": "icon-m-add", "text": "Create a profile",
      "hint": "A new address on a chatmail relay." },
    { "name": "backupFile", "icon": "icon-m-backup", "text": "Restore from a backup",
      "hint": "A backup file copied onto this phone." },
    { "name": "secondDevice", "icon": "icon-m-device", "text": "Add as second device",
      "hint": "The other device keeps it. Both get everything new." }
]"#;

type Steps = Rc<RefCell<Vec<(String, String)>>>;

#[test]
fn a_row_of_tiles_is_named_drawn_and_answered_for() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

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

    let steps: Steps = Rc::new(RefCell::new(Vec::new()));
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
        r(
            "load",
            call!("load", common::component_url("ChoiceTiles.qml")),
        );
        r("offer", call!("offer", CHOICES));
    });

    // A tick later: the tiles are built, and each of them has measured
    // what it needs.
    let r = record.clone();
    single_shot(Duration::from_secs(2), move || {
        r(
            "about-icon",
            call!("partOf", "aboutTile", "tileIcon", "source"),
        );
        r(
            "about-text",
            call!("partOf", "aboutTile", "tileLabel", "text"),
        );
        r(
            "about-hint",
            call!("partOf", "aboutTile", "tileHint", "visible"),
        );
        r(
            "setup-icon",
            call!("partOf", "setupTile", "tileIcon", "source"),
        );
        r(
            "setup-hint",
            call!("partOf", "setupTile", "tileHint", "visible"),
        );
        r("about-box", call!("boxOf", "aboutTile"));
        r("setup-box", call!("boxOf", "setupTile"));
        r("row-height", call!("own", "height"));
        r("about-needed", call!("get", "aboutTile", "needed"));
        r("setup-needed", call!("get", "setupTile", "needed"));

        r("setup-enabled", call!("get", "setupTile", "enabled"));
        r("about-enabled", call!("get", "aboutTile", "enabled"));
        r("tap", call!("tap", "aboutTile"));
        r("picked", call!("pickedSoFar"));
    });

    // 3s: the same row with three choices in it, stacked.
    let r = record.clone();
    single_shot(Duration::from_secs(3), move || {
        r("three", call!("offer", THREE));
        r("stack", call!("stack", "true"));
    });

    let r = record.clone();
    single_shot(Duration::from_secs(4), move || {
        r("first-box", call!("boxOf", "createProfileTile"));
        r("second-box", call!("boxOf", "backupFileTile"));
        r("third-box", call!("boxOf", "secondDeviceTile"));
        r("stacked-height", call!("own", "height"));
        r("gap", call!("own", "gap"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();
    assert_row(&steps.borrow());
    assert_stack(&steps.borrow());
}

/// Three choices stand one under another, the full width of the row.
///
/// Side by side each of them had a third of the screen, which crowds a
/// line of words with a second line under it -- the phone showed it on
/// the page behind the profiles plus.
fn assert_stack(steps: &[(String, String)]) {
    let value = |label: &str| -> &str {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map_or("<step did not run>", |(_, value)| value.as_str())
    };
    let context = format!("steps: {steps:?}");
    assert_eq!(
        value("three"),
        "ok",
        "the three choices were refused. {context}"
    );
    assert_eq!(value("stack"), "ok", "the row did not stack. {context}");

    let part = |label: &str, index: usize| -> f64 {
        value(label)
            .split(',')
            .nth(index)
            .unwrap_or_default()
            .parse()
            .unwrap_or_default()
    };
    let boxes = ["first-box", "second-box", "third-box"];
    for label in boxes {
        // The stub's margin is 24 and the row is 1080 across, so a
        // stacked tile is the whole 1032 between the margins.
        assert_eq!(
            (part(label, 0), part(label, 2)),
            (24.0, 1032.0),
            "a stacked tile does not fill the row between its margins: \
             {label}. {context}"
        );
    }

    let gap = value("gap").parse::<f64>().unwrap_or_default();
    let tile = part("first-box", 3);
    assert!(
        tile > 0.0 && gap > 0.0,
        "a stacked tile has no height, or no room under it. {context}"
    );
    for (index, label) in boxes.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let expected = index as f64 * (tile + gap);
        assert!(
            (part(label, 1) - expected).abs() < 1.0,
            "the stacked tiles do not follow one another down the page: \
             {label} is at {} rather than {expected}. {context}",
            part(label, 1)
        );
    }
    assert!(
        (value("stacked-height").parse::<f64>().unwrap_or_default() - (3.0 * tile + 2.0 * gap))
            .abs()
            < 1.0,
        "the stack is not as tall as the tiles and the room between \
         them. {context}"
    );
}

fn assert_row(steps: &[(String, String)]) {
    let value = |label: &str| -> &str {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map_or("<step did not run>", |(_, value)| value.as_str())
    };
    let context = format!("steps: {steps:?}");
    assert_eq!(value("load"), "ok", "the row did not load. {context}");
    assert_eq!(value("offer"), "ok", "the choices were refused. {context}");

    // The icon a choice asked for, in the theme's colours: an icon named
    // anything else is a tile with nothing on it.
    assert!(
        value("about-icon").starts_with("image://theme/icon-m-about?"),
        "the tile does not draw the theme icon it was given. {context}"
    );
    assert!(
        value("setup-icon").starts_with("image://theme/icon-m-person?"),
        "the second tile does not draw its own icon. {context}"
    );
    assert_eq!(
        value("about-text"),
        "Tell me about Delta Chat",
        "the tile does not say what it was given to say. {context}"
    );

    // The quiet line is there for a choice that has one and gone for a
    // choice that has not -- not an empty line holding the row open.
    assert_eq!(
        value("about-hint"),
        "false",
        "a choice with nothing more to say still draws a second line. {context}"
    );
    assert_eq!(
        value("setup-hint"),
        "true",
        "the choice's second line is not drawn. {context}"
    );

    // Side by side inside the margins, and both as tall as the one that
    // needs most: the stub's margin is 24 and the row is 1080 across, so
    // a tile is 516 wide and the second starts at 540.
    let part = |label: &str, index: usize| -> String {
        value(label)
            .split(',')
            .nth(index)
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(
        (part("about-box", 0), part("about-box", 2)),
        ("24".to_string(), "516".to_string()),
        "the first tile does not start inside the margin at half the row. {context}"
    );
    assert_eq!(
        (part("setup-box", 0), part("setup-box", 2)),
        ("540".to_string(), "516".to_string()),
        "the second tile does not stand beside the first. {context}"
    );
    let height = |label: &str| -> f64 { part(label, 3).parse().unwrap_or_default() };
    assert!(
        height("about-box") > 0.0,
        "the tiles have no height at all. {context}"
    );
    assert!(
        (height("about-box") - height("setup-box")).abs() < 1.0,
        "the tiles are of two heights: the row reads as blocks rather \
         than as a row. {context}"
    );
    // And as tall as the tile that needs most: the one with a second
    // line needs more than the one without, and that is what both are
    // drawn at.
    let needed = |label: &str| -> f64 { value(label).parse().unwrap_or_default() };
    assert!(
        needed("setup-needed") > needed("about-needed"),
        "a tile with a second line does not ask for more room than one \
         without, so this says nothing about which of them the row \
         follows. {context}"
    );
    assert!(
        (needed("row-height") - needed("setup-needed")).abs() < 1.0,
        "the row is not as tall as the tile that needs most. {context}"
    );
    assert!(
        (height("about-box") - needed("setup-needed")).abs() < 1.0,
        "the tile that needs least is drawn at its own height rather \
         than the row's. {context}"
    );

    // A choice that is off is off, and one that never mentioned it is on.
    assert_eq!(
        value("setup-enabled"),
        "false",
        "a choice that is off is still tappable. {context}"
    );
    assert_eq!(
        value("about-enabled"),
        "true",
        "a choice that said nothing about being off is off. {context}"
    );

    assert_eq!(
        value("tap"),
        "ok",
        "the tile could not be tapped. {context}"
    );
    assert_eq!(
        value("picked"),
        "about;",
        "tapping a tile did not say which one it was. {context}"
    );
}
