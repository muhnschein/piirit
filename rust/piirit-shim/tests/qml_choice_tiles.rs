//! Onboarding choice tiles. Checked: tile per choice, named, icon and words; no second line without
//! words; same height side by side; off choice off; tap says which. Placement tested per page
//! (`qml_pages.rs`, `qml_restore.rs`).

// Qt harness: `unsafe` env::set_var pre-Qt (`unused_unsafe`: unsafe only from edition 2024),
// `borrow_as_ptr` for engine pointer.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;
use std::cell::RefCell;
use std::rc::Rc;

mod common;

/// Loaded at phone width; Loader given width, not height: row is as tall as its tiles need, and
/// that is read back.
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

/// Two ways on: one with second line, one without; one off.
const CHOICES: &str = r#"[
    { "name": "about", "icon": "icon-m-about", "text": "About Delta Chat" },
    { "name": "setup", "mark": "account", "text": "Set up your profile",
      "hint": "A new address, on a relay that carries chat mail and nothing else at all.",
      "enabled": false }
]"#;

/// Three ways in: too many side by side on phone.
const THREE: &str = r#"[
    { "name": "createProfile", "icon": "icon-m-add", "text": "Create a profile",
      "hint": "An address made on this phone." },
    { "name": "backupFile", "icon": "icon-m-backup", "text": "Restore from a backup",
      "hint": "A file written by another device." },
    { "name": "secondDevice", "icon": "icon-m-device", "text": "Add as second device",
      "hint": "A profile another device is offering." }
]"#;

type Steps = Rc<RefCell<Vec<(String, String)>>>;

// Timed steps in order, as qml_components.rs: splitting hides order; each tick reads what prior
// set.
#[allow(clippy::too_many_lines)]
#[test]
fn a_row_of_tiles_is_named_drawn_and_answered_for() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    // SAFETY: runs only in `exec()`; `engine` outlives it.
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

    let mut plan = common::Plan::default();

    let r = record.clone();
    plan.after(1, move || {
        r(
            "load",
            call!("load", common::component_url("ChoiceTiles.qml")),
        );
        r("offer", call!("offer", CHOICES));
    });

    // Tick later: tiles measured.
    let r = record.clone();
    plan.after(1, move || {
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
            "setup-mark",
            call!("partOf", "setupTile", "tileMark", "visible"),
        );
        r(
            "about-mark",
            call!("partOf", "aboutTile", "tileMark", "visible"),
        );
        r(
            "setup-icon",
            call!("partOf", "setupTile", "tileIcon", "source"),
        );
        r(
            "setup-hint",
            call!("partOf", "setupTile", "tileHint", "visible"),
        );
        r(
            "about-align",
            call!("partOf", "aboutTile", "tileLabel", "horizontalAlignment"),
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

    // 3s: three choices, stacked.
    let r = record.clone();
    plan.after(1, move || {
        r("three", call!("offer", THREE));
        r("stack", call!("stack", "true"));
    });

    let r = record.clone();
    plan.after(1, move || {
        r("first-box", call!("boxOf", "createProfileTile"));
        r("second-box", call!("boxOf", "backupFileTile"));
        r("third-box", call!("boxOf", "secondDeviceTile"));
        r("stacked-height", call!("own", "height"));
        r("gap", call!("own", "gap"));
        r(
            "row-icon-x",
            call!("partOf", "backupFileTile", "tileIcon", "x"),
        );
        r(
            "row-icon-width",
            call!("partOf", "backupFileTile", "tileIcon", "width"),
        );
        r(
            "row-label-x",
            call!("partOf", "backupFileTile", "tileLabel", "x"),
        );
        r(
            "row-label-align",
            call!(
                "partOf",
                "backupFileTile",
                "tileLabel",
                "horizontalAlignment"
            ),
        );
        r(
            "row-hint-x",
            call!("partOf", "backupFileTile", "tileHint", "x"),
        );
    });

    plan.after(1, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();
    assert_row(&steps.borrow());
    assert_stack(&steps.borrow());
}

/// Three stack, full row width: side by side, a third of screen each crowds a second line (seen
/// on phone, page behind profiles plus).
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
        // Margin 24, row 1080: stacked tile 1032 wide.
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

    // Stacked tile is a row: icon left, words beside, left-aligned (`Text.AlignLeft` = 1).
    let number = |label: &str| -> f64 { value(label).parse().unwrap_or_default() };
    assert!(
        number("row-icon-x") > 0.0 && number("row-icon-x") < 1032.0 / 4.0,
        "the icon in a stacked tile is not at its left: {}. {context}",
        value("row-icon-x")
    );
    assert!(
        number("row-label-x") >= number("row-icon-x") + number("row-icon-width")
            && number("row-icon-width") > 0.0,
        "the words in a stacked tile are not beside the icon: the label \
         starts at {} and the icon ends at {}. {context}",
        number("row-label-x"),
        number("row-icon-x") + number("row-icon-width")
    );
    assert_eq!(
        value("row-label-align"),
        "1",
        "the words in a stacked tile are not ranged left. {context}"
    );
    assert!(
        (number("row-hint-x") - number("row-label-x")).abs() < 1.0,
        "the second line of a stacked tile does not start where the \
         first one does. {context}"
    );
}

/// Step's report, or marker it never ran.
fn step<'a>(steps: &'a [(String, String)], label: &str) -> &'a str {
    steps
        .iter()
        .find(|(name, _)| name == label)
        .map_or("<step did not run>", |(_, value)| value.as_str())
}

/// Each tile: given icon or mark, words, quiet line if any.
fn assert_row(steps: &[(String, String)]) {
    let value = |label: &str| step(steps, label);
    let context = format!("steps: {steps:?}");
    assert_eq!(value("load"), "ok", "the row did not load. {context}");
    assert_eq!(value("offer"), "ok", "the choices were refused. {context}");

    // Icon the choice asked for, in theme colours: any other name is a tile with nothing on it.
    assert!(
        value("about-icon").starts_with("image://theme/icon-m-about?"),
        "the tile does not draw the theme icon it was given. {context}"
    );
    // Named mark: mark, theme not asked; icon: no mark.
    assert_eq!(
        value("setup-mark"),
        "true",
        "the tile given a drawn mark does not draw it. {context}"
    );
    assert_eq!(
        value("setup-icon"),
        "",
        "the tile given a drawn mark still asks the theme for an icon. {context}"
    );
    assert_eq!(
        value("about-mark"),
        "false",
        "the tile given a theme icon draws the account mark too. {context}"
    );
    assert_eq!(
        value("about-text"),
        "About Delta Chat",
        "the tile does not say what it was given to say. {context}"
    );

    // Side by side: words centred (`Text.AlignHCenter` = 4).
    assert_eq!(
        value("about-align"),
        "4",
        "the words under a side-by-side tile's icon are not centred. {context}"
    );

    // Quiet line only when choice has one.
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

    assert_row_layout(steps);
}

/// Side by side in margins, as tall as tallest; off choice off; tap says which tile.
fn assert_row_layout(steps: &[(String, String)]) {
    let value = |label: &str| step(steps, label);
    let context = format!("steps: {steps:?}");

    // Stub margin 24, row 1080: tile 516 wide, second starts at 540.
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
    // As tall as tallest tile.
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

    // Off choice is off; one that never mentioned it is on.
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
