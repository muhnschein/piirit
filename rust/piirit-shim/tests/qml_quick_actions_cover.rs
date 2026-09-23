//! The cover's quick actions: what the home screen is handed, and what the
//! grid does to make room for them.
//!
//! None, one or two, whichever the settings hold: a list for two, a list
//! for one, and the one that fits switched on. Each action's picture is a
//! file the home screen reads for itself, so it has to be a whole path to
//! a file that is there, at the size and in the ink the ambience wants. A
//! tap only says which side it was; what it does is the window's
//! (`qml_quick_actions_window.rs`).
//!
//! With actions on, the grid fades out into their strip, and the faces
//! that matter stay above it.

// Qt harness: see qml_cover.rs.
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

/// The probe, with the components directory filled in so it can write the
/// `Settings` the cover reads.
fn probe_qml() -> String {
    let components =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    PROBE_QML.replace("__COMPONENTS__", &components.display().to_string())
}

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://__COMPONENTS__'
    Item {
        id: probe
        property string sides: ''
        Loader { id: loader }
        Connections {
            target: loader.item
            onQuickAction: probe.sides += side + ';'
        }
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
        function count(n) { Settings.quickActionCount = n; return 'ok' }
        function set(side, kind, account, chat, icon) {
            var prefix = side === 'right' ? 'quickActionRight' : 'quickActionLeft'
            Settings[prefix] = kind
            Settings[prefix + 'Account'] = account
            Settings[prefix + 'Chat'] = chat
            Settings[prefix + 'Icon'] = icon
            return 'ok'
        }
        // Which lists are on, as `two|one`.
        function lists() {
            return findIn(loader.item, 'twoActions').enabled + '|'
                   + findIn(loader.item, 'oneAction').enabled
        }
        // The picture of one list's action, as the home screen gets it.
        function icon(list, index) {
            return '' + findIn(loader.item, list).actions[index].iconSource
        }
        function tap(list, index) {
            findIn(loader.item, list).actions[index].triggered()
            return probe.sides
        }
        function allow(allowed) {
            loader.item.quickActionsAllowed = allowed === 'true'
            return 'ok'
        }
        function ink(colour) { Theme.primaryColor = colour; return 'ok' }
        function fade() { return '' + findIn(loader.item, 'avatarGrid').layer.enabled }
        function floor() { return '' + loader.item.floor }
        function markUnread(list, chatId) {
            allIn(loader.item, 'coverChats', [])[list].mark_unread(chatId)
            return 'ok'
        }
        // Whether every lit face is drawn whole above the strip.
        function litPlaces() {
            var cells = allIn(loader.item, 'gridCell', [])
            var out = []
            for (var i = 0; i < cells.length; i++) {
                if (!cells[i].highlight) { continue }
                out.push(cells[i].y >= 0
                         && cells[i].y + cells[i].height <= loader.item.floor
                         ? 'above' : 'in-strip')
            }
            return out.join(';')
        }
    }
";

fn cover_url() -> String {
    format!(
        "file://{}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../qml/cover/CoverPage.qml")
            .display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_cover_offers_the_actions_that_are_set_and_makes_room_for_them() {
    let temp = std::env::temp_dir().join(format!("piirit-qml-quick-cover-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts and before
    // the server inherits them. Two profiles, so both have a face lit.
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
    engine.load_data(QByteArray::from(probe_qml().as_str()));

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
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }
    macro_rules! set {
        ($side:expr, $kind:expr, $account:expr, $chat:expr, $icon:expr) => {
            call!(
                "set",
                QString::from($side),
                QString::from($kind),
                $account,
                $chat,
                QString::from($icon)
            )
        };
    }

    // SAFETY: the callbacks fire only while `exec()` runs on this thread.
    single_shot(Duration::from_secs(1), move || unsafe {
        // dconf outlives a run on a phone, and the stub's values outlive
        // nothing, but say it anyway: nothing is set to start with.
        record!("clear-left", set!("left", "", 0, 0, ""));
        record!("clear-right", set!("right", "", 0, 0, ""));
        record!("room-for-two", call!("count", 2));
        record!("load", call!("load", QString::from(cover_url())));
        record!("none-lists", call!("lists"));
        record!("none-fade", call!("fade"));
        record!("none-floor", call!("floor"));

        // One action: the list for one, holding it.
        record!("set-left", set!("left", "search", 0, 0, ""));
        record!("one-lists", call!("lists"));
        record!("one-icon", call!("icon", QString::from("oneAction"), 0));
        record!("one-fade", call!("fade"));
        record!("one-floor", call!("floor"));
        record!("one-tap", call!("tap", QString::from("oneAction"), 0));

        // Two: left and right, in their places.
        record!("set-right", set!("right", "chat", 1, 2, "star"));
        record!("two-lists", call!("lists"));
        record!("two-left", call!("icon", QString::from("twoActions"), 0));
        record!("two-right", call!("icon", QString::from("twoActions"), 1));
        record!("two-tap", call!("tap", QString::from("twoActions"), 1));

        // Room for one: the left alone, the right kept but not offered.
        record!("room-for-one", call!("count", 1));
        record!("held-right-lists", call!("lists"));
        record!(
            "held-right-icon",
            call!("icon", QString::from("oneAction"), 0)
        );
        record!("back-to-two", call!("count", 2));
        record!("both-again", call!("lists"));

        // An icon this version does not know is the first one; a kind it
        // does not know is no action at all, which leaves the other alone.
        record!("set-odd-icon", set!("right", "chat", 1, 2, "teapot"));
        record!("odd-icon", call!("icon", QString::from("twoActions"), 1));
        record!("set-odd-kind", set!("left", "teapot", 0, 0, ""));
        record!("odd-lists", call!("lists"));
        record!("odd-only", call!("icon", QString::from("oneAction"), 0));
        record!("odd-tap", call!("tap", QString::from("oneAction"), 0));

        // A light ambience gets the black ink.
        record!("light", call!("ink", QString::from("#000000")));
        record!("light-icon", call!("icon", QString::from("oneAction"), 0));
        record!("dark", call!("ink", QString::from("#ffffff")));

        // The window takes them away while it cannot act on one; the room
        // made for them goes with them.
        record!("disallow", call!("allow", QString::from("false")));
        record!("held-lists", call!("lists"));
        record!("held-fade", call!("fade"));
        record!("held-floor", call!("floor"));
        record!("allow", call!("allow", QString::from("true")));

        // Both back.
        record!("set-both", set!("left", "qr", 0, 0, ""));
    });

    // Once the lists are in, someone writes under each profile.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!("mark-first", call!("markUnread", 0, 1));
        record!("mark-second", call!("markUnread", 1, 2));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("lit-places", call!("litPlaces"));
        record!("final-lists", call!("lists"));
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
        value("none-lists"),
        "false|false",
        "a cover with no quick action set offers one. {context}"
    );
    assert_eq!(
        value("none-fade"),
        "false",
        "the grid fades out with no actions to make room for. {context}"
    );
    assert_eq!(
        value("none-floor"),
        "360",
        "the faces stop short of the bottom edge with nothing below them. \
         {context}"
    );
    assert_eq!(
        value("one-lists"),
        "false|true",
        "one action set is not offered on its own. {context}"
    );
    assert_eq!(
        value("one-fade"),
        "true",
        "the grid runs on under the action strip. {context}"
    );
    // The stub's small item is 60 high.
    assert_eq!(
        value("one-floor"),
        "300",
        "the faces are not kept above the action strip. {context}"
    );
    assert_eq!(
        value("one-tap"),
        "left;",
        "a tap on the only action does not say it was the left one. {context}"
    );
    assert_eq!(
        value("two-lists"),
        "true|false",
        "two actions set are not offered together. {context}"
    );
    assert_eq!(
        value("two-tap"),
        "left;right;",
        "a tap on the right action does not say so. {context}"
    );
    assert_eq!(
        value("held-right-lists"),
        "false|true",
        "with room for one the cover offers more than one. {context}"
    );
    assert!(
        value("held-right-icon").ends_with("search-32-white.png"),
        "with room for one the cover does not offer the left action. {context}"
    );
    assert_eq!(
        value("both-again"),
        "true|false",
        "the right action is lost once there is room for it again. {context}"
    );
    assert_eq!(
        value("odd-lists"),
        "false|true",
        "an action of a kind this version does not know is offered. {context}"
    );
    assert_eq!(
        value("odd-tap"),
        "left;right;right;",
        "the one action left is not the right one. {context}"
    );
    assert_eq!(
        value("held-lists"),
        "false|false",
        "actions are offered while the window cannot take them. {context}"
    );
    assert_eq!(
        value("held-fade"),
        "false",
        "the grid still fades with no actions drawn. {context}"
    );
    assert_eq!(value("held-floor"), "360", "{context}");
    assert_eq!(
        value("final-lists"),
        "true|false",
        "the actions did not come back. {context}"
    );
    assert_eq!(
        value("lit-places"),
        "above;above",
        "a lit face was put in the fading strip rather than above it. {context}"
    );

    // Each picture is a whole path to a file that is there, at the stub's
    // small icon size and in the ink for its ambience.
    for (label, file) in [
        ("one-icon", "search-32-white.png"),
        ("two-left", "search-32-white.png"),
        ("two-right", "star-32-white.png"),
        ("odd-icon", "heart-32-white.png"),
        ("odd-only", "heart-32-white.png"),
        ("light-icon", "heart-32-black.png"),
    ] {
        let url = value(label);
        let path = url.strip_prefix("file://").unwrap_or_else(|| {
            panic!("the home screen is not handed a file URL ({label}: {url}). {context}")
        });
        assert!(
            path.ends_with(&format!("/art/cover/{file}")),
            "{label} is not {file}: {url}. {context}"
        );
        assert!(
            std::path::Path::new(path).is_file(),
            "{label} names a file that is not there: {path}. {context}"
        );
    }
}

/// The strings or numbers in the JavaScript array literal that a line
/// starting `var <name> = [` opens, however many lines it runs to.
fn listed(script: &str, name: &str) -> Vec<String> {
    let start = format!("\nvar {name} = [");
    let at = script
        .find(&start)
        .unwrap_or_else(|| panic!("qml/js/QuickActions.js has no `{}`", start.trim()));
    script[at + start.len()..]
        .split(']')
        .next()
        .unwrap_or_default()
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

/// Every icon an action can wear is drawn at every size the cover picks
/// from, in both inks, as the square RGBA picture the home screen is
/// handed -- and every icon drawn is one of those, from a source that is
/// there. The lists are the ones the cover reads.
#[test]
fn every_quick_action_icon_is_drawn_at_every_size_in_both_inks() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = std::fs::read_to_string(root.join("qml/js/QuickActions.js"))
        .expect("read qml/js/QuickActions.js");
    let mut icons: Vec<String> = listed(&script, "kinds")
        .into_iter()
        .filter(|kind| kind != "chat")
        .collect();
    icons.extend(listed(&script, "chatIcons"));
    let sizes: Vec<u32> = listed(&script, "sizes")
        .iter()
        .map(|size| size.parse().expect("a size is a number"))
        .collect();
    assert_eq!(
        icons.len(),
        16,
        "the icons are not the sixteen offered: {icons:?}"
    );
    assert_eq!(sizes, [32, 40, 48, 56, 64], "the sizes moved");

    let mut expected = std::collections::BTreeSet::new();
    for icon in &icons {
        assert!(
            root.join(format!("icons/cover/{icon}.svg")).is_file(),
            "icons/cover/{icon}.svg, the source of the {icon} icon, is missing"
        );
        for size in &sizes {
            for ink in ["white", "black"] {
                let file = format!("cover/{icon}-{size}-{ink}.png");
                let (width, height, depth, colour, interlace) = common::png_header(&file);
                assert_eq!(
                    (width, height),
                    (*size, *size),
                    "qml/art/{file} is not {size} square"
                );
                assert_eq!(depth, 8, "qml/art/{file} is not 8 bits per channel");
                assert_eq!(colour, 6, "qml/art/{file} is not RGBA");
                assert_eq!(interlace, 0, "qml/art/{file} is interlaced");
                expected.insert(file.trim_start_matches("cover/").to_string());
            }
        }
    }
    let drawn: std::collections::BTreeSet<String> = std::fs::read_dir(root.join("qml/art/cover"))
        .expect("read qml/art/cover")
        .map(|entry| {
            entry
                .expect("read a qml/art/cover entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        drawn, expected,
        "qml/art/cover holds pictures no icon uses, or lacks some; run \
         scripts/render-cover-icons.sh"
    );
}
