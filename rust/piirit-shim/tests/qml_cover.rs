//! The cover, with people in it, across two profiles.
//!
//! Two of its three states: the staggered grid of everyone in grey,
//! once each, with empty circles for the cells left over and no pill,
//! while nothing is new; and whoever wrote lit up in the ambience's own
//! highlight -- in the cells worth having, under the pill that counts --
//! once something is; counted and drawn across both profiles the fake
//! core is told to have. The third state -- nobody yet -- is
//! `qml_cover_empty.rs`, since it takes a core seeded differently.

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

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

/// Loads the cover at a size, since the stub `CoverBackground` has none of
/// its own, and reads it back.
pub const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 240, height: 360 })
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
        function allIn(node, name, found) {
            if (!node) { return found }
            if (node.objectName === name) { found.push(node) }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                allIn(kids[i], name, found)
            }
            return found
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        // The grid's cells, and how many of them are drawn in colour.
        function drawn() {
            return '' + allIn(loader.item, 'gridCell', []).length
        }
        function lit() {
            var cells = allIn(loader.item, 'gridCell', [])
            var total = 0
            for (var i = 0; i < cells.length; i++) {
                if (cells[i].highlight) { total += 1 }
            }
            return '' + total
        }
        // Anyone drawn in their own colour rather than the phone's:
        // neither grey nor the ambience's highlight.
        function ownColoured() {
            var cells = allIn(loader.item, 'gridCell', [])
            var total = 0
            for (var i = 0; i < cells.length; i++) {
                if (!cells[i].monochrome && !cells[i].highlight) {
                    total += 1
                }
            }
            return '' + total
        }
        // Whether each lit cell is drawn whole inside the grid: the
        // shifted rows hang off both edges, and the bottom row is cut.
        function litPlaces() {
            var cells = allIn(loader.item, 'gridCell', [])
            var grid = findIn(loader.item, 'avatarGrid')
            var out = []
            for (var i = 0; i < cells.length; i++) {
                if (!cells[i].highlight) { continue }
                var whole = cells[i].x >= 0
                            && cells[i].x + cells[i].width <= grid.width
                            && cells[i].y + cells[i].height <= grid.height
                out.push(whole ? 'whole' : 'cut')
            }
            return out.join(';')
        }
        // The worst place a lit face was given against the best a grey
        // one took, by the cover's own reckoning of a cell.
        function litOrder() {
            var cells = loader.item.cells
            var worstLit = -1
            var bestQuiet = 1000000
            for (var i = 0; i < cells.length; i++) {
                var place = loader.item.prominence(cells[i].row, cells[i].col)
                if (cells[i].loud) {
                    worstLit = Math.max(worstLit, place)
                } else {
                    bestQuiet = Math.min(bestQuiet, place)
                }
            }
            return worstLit + '|' + bestQuiet
        }
        // The leftmost cell: a shifted row starts half a cell off the edge.
        function leftmost() {
            var cells = allIn(loader.item, 'gridCell', [])
            var least = 0
            for (var i = 0; i < cells.length; i++) {
                if (cells[i].x < least) { least = cells[i].x }
            }
            return '' + least
        }
        function planned() { return '' + loader.item.cells.length }
        // The pill against the cover: whether it is centred, two cells
        // wide, and drawn over no face -- no face's edge inside it, so
        // the room between it and its neighbours is at least the grid's
        // own gap.
        function pillPlace() {
            var pill = findIn(loader.item, 'unreadPill')
            var size = loader.item.cellSize
            var centred = Math.abs((pill.x + pill.width / 2) - loader.item.width / 2) <= 1
            var twoWide = Math.abs(pill.width - 2 * size) < size / 4
            var cells = allIn(loader.item, 'gridCell', [])
            var clear = true
            for (var i = 0; i < cells.length; i++) {
                var apart = cells[i].x + cells[i].width <= pill.x
                            || cells[i].x >= pill.x + pill.width
                            || cells[i].y + cells[i].height <= pill.y
                            || cells[i].y >= pill.y + pill.height
                if (!apart) { clear = false }
            }
            return (centred ? 'centred' : 'off') + ';'
                   + (twoWide ? 'two-wide' : 'narrow') + ';'
                   + (clear ? 'clear' : 'over-a-face')
        }
        // Whether every lit face sits under the pill.
        function litBelowPill() {
            var pill = findIn(loader.item, 'unreadPill')
            var cells = allIn(loader.item, 'gridCell', [])
            for (var i = 0; i < cells.length; i++) {
                if (cells[i].highlight && cells[i].y < pill.y + pill.height) {
                    return 'above'
                }
            }
            return 'below'
        }
        // The smallest gap between any two circles, or between a circle
        // and the pill when it is there: what keeps them from touching.
        function closest() {
            var items = allIn(loader.item, 'gridCell', [])
            var pill = findIn(loader.item, 'unreadPill')
            if (pill.visible) { items.push(pill) }
            var least = 1000000
            for (var i = 0; i < items.length; i++) {
                for (var j = i + 1; j < items.length; j++) {
                    var a = items[i], b = items[j]
                    var dx = Math.max(0, Math.max(a.x, b.x) - Math.min(a.x + a.width, b.x + b.width))
                    var dy = Math.max(0, Math.max(a.y, b.y) - Math.min(a.y + a.height, b.y + b.height))
                    if (dx === 0 && dy === 0) { return 'touching' }
                    // Two circles on nested rows meet corner to corner.
                    least = Math.min(least, Math.sqrt(dx * dx + dy * dy))
                }
            }
            return '' + least
        }
        // How many cells hold someone, and how many different someones.
        function placed() {
            var cells = loader.item.cells
            var total = 0
            var keys = {}
            var distinct = 0
            for (var i = 0; i < cells.length; i++) {
                if (!cells[i].person) { continue }
                total += 1
                if (!keys[cells[i].person.key]) { keys[cells[i].person.key] = true; distinct += 1 }
            }
            return total + '|' + distinct
        }
        // The worst place anyone was given against the best an empty
        // circle took: people before holes.
        function peopleFirst() {
            var cells = loader.item.cells
            var worstPerson = -1
            var bestHole = 1000000
            for (var i = 0; i < cells.length; i++) {
                var place = loader.item.prominence(cells[i].row, cells[i].col)
                if (cells[i].person) {
                    worstPerson = Math.max(worstPerson, place)
                } else {
                    bestHole = Math.min(bestHole, place)
                }
            }
            // Not strictly: two cells the same way out from the middle
            // are as good as each other.
            return worstPerson <= bestHole ? 'people-first' : 'a-hole-first'
        }
        // The topmost cell: the grid starts above the cover.
        function topmost() {
            var cells = allIn(loader.item, 'gridCell', [])
            var least = 0
            for (var i = 0; i < cells.length; i++) {
                if (cells[i].y < least) { least = cells[i].y }
            }
            return '' + least
        }
        // The order people take cells in, read off the cover's own rule.
        function ranks() {
            var rank = loader.item.rank
            return rank({ unread_count: 2, avatar_path: '' }) + ';'
                   + rank({ unread_count: 0, avatar_path: '/p/a.png' }) + ';'
                   + rank({ unread_count: 0, avatar_path: '' }) + ';'
                   + rank({ unread_count: 1, avatar_path: '/p/a.png' })
        }
        function people() { return '' + loader.item.people.length }
        // The lists, one per profile, in the profiles' order.
        function lists() {
            return '' + allIn(loader.item, 'coverChats', []).length
        }
        function markUnread(list, chatId) {
            allIn(loader.item, 'coverChats', [])[list].mark_unread(chatId)
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_cover_draws_everyone_and_lights_whoever_wrote() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-cover-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. Two configured profiles, each with the
    // fake's two chats.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        std::env::set_var("PIIRIT_FAKE_ACCOUNTS", "1,2");
    }

    piirit_shim::register_qml_types();
    common::register_cover_enum();

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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!("load", call!("load", QString::from(cover_url())));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("lists", call!("lists"));
        record!("pill-quiet", get!("unreadPill", "visible"));
        record!("people", call!("people"));
        record!("planned", call!("planned"));
        record!("drawn-quiet", call!("drawn"));
        record!("placed-quiet", call!("placed"));
        record!("people-first", call!("peopleFirst"));
        record!("lit-quiet", call!("lit"));
        record!("own-quiet", call!("ownColoured"));
        record!("leftmost", call!("leftmost"));
        record!("topmost", call!("topmost"));
        record!("closest-quiet", call!("closest"));
        record!("ranks", call!("ranks"));
        // Someone writes under each profile.
        record!("mark-first", call!("markUnread", 0, 1));
        record!("mark-second", call!("markUnread", 1, 2));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("count-loud", get!("unreadTotal", "text"));
        record!("pill-loud", get!("unreadPill", "visible"));
        record!("pill-place", call!("pillPlace"));
        record!("lit-below-pill", call!("litBelowPill"));
        record!("closest-loud", call!("closest"));
        record!("drawn-loud", call!("drawn"));
        record!("planned-loud", call!("planned"));
        record!("placed-loud", call!("placed"));
        record!("lit-loud", call!("lit"));
        record!("own-loud", call!("ownColoured"));
        record!("lit-places", call!("litPlaces"));
        record!("lit-order", call!("litOrder"));
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the cover did not load. {context}");
    assert_eq!(
        value("lists"),
        "2",
        "the cover does not keep a list per profile. {context}"
    );
    assert_eq!(
        value("pill-quiet"),
        "false",
        "the pill is drawn with nothing unread; the grid alone says as much. \
         {context}"
    );
    assert_eq!(
        value("pill-place"),
        "centred;two-wide;clear",
        "the pill is not a centred, two-cell-wide cell of the grid's own. \
         {context}"
    );
    assert_eq!(
        value("lit-below-pill"),
        "below",
        "a lit face sits above the pill rather than under it. {context}"
    );
    for label in ["closest-quiet", "closest-loud"] {
        let closest: f64 = value(label).parse().unwrap_or(0.0);
        assert!(
            closest >= 4.0,
            "two circles, or a circle and the pill, are too close ({label}: {}). \
             {context}",
            value(label)
        );
    }
    assert_eq!(
        value("ranks"),
        "0;1;2;0",
        "people are not ranked new first, then pictured, then the rest. {context}"
    );
    assert_eq!(
        value("people-first"),
        "people-first",
        "an empty circle was given a better cell than someone. {context}"
    );
    assert_eq!(
        value("people"),
        "4",
        "the people are not everyone across both profiles. {context}"
    );
    let planned: u32 = value("planned").parse().unwrap_or(0);
    assert!(
        planned >= 7,
        "the cover's shape gives the grid fewer than two rows. {context}"
    );
    assert_eq!(
        value("drawn-quiet"),
        planned.to_string(),
        "the grid is not drawn cell for cell. {context}"
    );
    // Four people, each drawn once; every other cell is an empty circle
    // rather than someone drawn again.
    for label in ["placed-quiet", "placed-loud"] {
        assert_eq!(
            value(label),
            "4|4",
            "the people are not drawn once each ({label}). {context}"
        );
    }
    assert_eq!(
        value("lit-quiet"),
        "0",
        "someone is lit with nothing unread. {context}"
    );
    let leftmost: f64 = value("leftmost").parse().unwrap_or(0.0);
    assert!(
        leftmost < 0.0,
        "no row is shifted off the edge, so the rows do not stagger. {context}"
    );
    let topmost: f64 = value("topmost").parse().unwrap_or(0.0);
    assert!(
        topmost < 0.0,
        "the first row is drawn whole rather than cut by the top edge. {context}"
    );
    for label in ["mark-first", "mark-second"] {
        assert_eq!(
            value(label),
            "ok",
            "marking a chat unread failed. {context}"
        );
    }
    assert_eq!(
        value("count-loud"),
        "2 new",
        "the count is not every unread message across both profiles. {context}"
    );
    assert_eq!(
        value("pill-loud"),
        "true",
        "the pill is not there once something is new. {context}"
    );
    assert_eq!(
        value("drawn-loud"),
        value("planned-loud"),
        "the grid is not drawn cell for cell once someone wrote. {context}"
    );
    assert_eq!(
        value("lit-loud"),
        "2",
        "the two who wrote are not the two lit, once each, in the grid. \
         {context}"
    );
    // Nobody is ever drawn in their own colour here: a cover belongs to
    // the phone, so the two who wrote wear the ambience's highlight and
    // everyone else is grey.
    for label in ["own-quiet", "own-loud"] {
        assert_eq!(
            value(label),
            "0",
            "someone on the cover is drawn in their own colour rather \
             than the ambience's ({label}). {context}"
        );
    }
    assert_eq!(
        value("lit-places"),
        "whole;whole",
        "a lit face was put in a cell the cover only draws part of. \
         {context}"
    );
    let order = value("lit-order");
    let (worst_lit, best_quiet) = order
        .split_once('|')
        .map_or((f64::MAX, 0.0), |(lit, quiet)| {
            (
                lit.parse::<f64>().unwrap_or(f64::MAX),
                quiet.parse::<f64>().unwrap_or(0.0),
            )
        });
    // Not strictly: the top row's two outer cells are as good as each
    // other, and a second lit face takes one of them.
    assert!(
        worst_lit <= best_quiet,
        "a grey face was given a better place than a lit one ({order}). \
         {context}"
    );
}

fn cover_url() -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    )
}
