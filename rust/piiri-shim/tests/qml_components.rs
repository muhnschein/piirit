//! The small pieces the conversation page is built from, loaded on their
//! own: the page itself cannot be, because Silica's `EnterKey` attached
//! property has no stub.

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

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource('', {})
            loader.setSource(url, { width: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function call(name, argument) { return '' + loader.item[name](argument) }
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
        // Colours compare badly as strings; the alpha is what matters.
        function alphaOf(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property].a
        }
        function own(property) { return '' + loader.item[property] }
        // Any themed icon anywhere under the loaded item, by name rather
        // than by objectName: whoever puts one back will not name it after
        // the check that is meant to stop them.
        function themedIcons(node) {
            if (!node) { return 0 }
            var found = 0
            // Coerced, not type-checked: `source` is a url, and what
            // `typeof` calls that is not worth depending on.
            if (node.source !== undefined
                    && ('' + node.source).indexOf('image://theme') === 0) {
                found += 1
            }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                found += themedIcons(kids[i])
            }
            return found
        }
        function themedIconCount() { return '' + themedIcons(loader.item) }
        // What the tray reports when one of its choices is taken, and
        // whether it puts itself away afterwards.
        property string asked: ''
        function watchTray() {
            asked = ''
            loader.item.appRequested.connect(function () { asked += 'app;' })
            loader.item.libraryRequested.connect(function () { asked += 'library;' })
            return 'ok'
        }
        function askedSoFar() { return asked }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

/// Long enough to overflow three lines even at the stub's small font; on a
/// phone it takes far less.
const LONG_BODY: &str = "a quoted message long enough to need three lines of its own in \
     the bar above the field, and then a good deal more after that, so that what does \
     not fit has to be cut off rather than run off the side of the screen, which is what \
     it did before -- the label had no wrapping at all, so a single long line simply ran \
     past the edge and out of sight, taking the rest of the quote with it, and none of \
     that is any use to someone trying to see which message they are about to answer";

// A script of timed steps, in the order they happen; splitting it would
// hide that order for no gain.
#[allow(clippy::too_many_lines)]
#[test]
fn the_reply_bar_wraps_the_jump_button_is_opaque_and_a_notice_is_quiet() {
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

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "bar-load",
            call!("load", QString::from(common::component_url("ReplyBar.qml")))
        );
        call!("set", QString::from("author"), QString::from("Ada"));
        call!("set", QString::from("body"), QString::from(LONG_BODY));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "bar-lines",
            call!(
                "get",
                QString::from("replyLabel"),
                QString::from("lineCount")
            )
        );
        record!(
            "bar-truncated",
            call!(
                "get",
                QString::from("replyLabel"),
                QString::from("truncated")
            )
        );
        record!("bar-height", call!("own", QString::from("height")));

        record!(
            "jump-load",
            call!(
                "load",
                QString::from(common::component_url("JumpButton.qml"))
            )
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "jump-opaque",
            call!("alphaOf", QString::from("jumpDisc"), QString::from("color"))
        );
        record!(
            "jump-chevron",
            call!("get", QString::from("jumpChevron"), QString::from("width"))
        );
        record!("jump-themed-icons", call!("themedIconCount"));
        // Item.Left, not Item.TopLeft: about the corner it is the stroke's
        // top edge that follows the line, and the two halves come out
        // crooked.
        record!(
            "chevron-origin",
            call!(
                "get",
                QString::from("chevronLeft"),
                QString::from("transformOrigin")
            )
        );
        record!(
            "chevron-mirrored",
            call!(
                "get",
                QString::from("chevronRight"),
                QString::from("transformOrigin")
            )
        );
        record!(
            "chevron-left-y",
            call!("get", QString::from("chevronLeft"), QString::from("y"))
        );
        record!(
            "chevron-right-y",
            call!("get", QString::from("chevronRight"), QString::from("y"))
        );
        record!(
            "badge-hidden",
            call!("get", QString::from("jumpBadge"), QString::from("visible"))
        );
        call!("set", QString::from("count"), 3.0);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "badge-shown",
            call!("get", QString::from("jumpBadge"), QString::from("visible"))
        );
        record!(
            "badge-text",
            call!(
                "get",
                QString::from("jumpBadgeLabel"),
                QString::from("text")
            )
        );

        record!(
            "banner-load",
            call!("load", QString::from(common::component_url("Banner.qml")))
        );
        call!(
            "call",
            QString::from("show"),
            QString::from("Copied to clipboard")
        );
        // Read in the tone it defaults to before it is switched, so the
        // assertion below compares two colours rather than reading back a
        // value the test itself wrote.
        record!(
            "tone-error-colour",
            call!("get", QString::from("errorLabel"), QString::from("color"))
        );
        call!("set", QString::from("tone"), QString::from("info"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!(
            "notice-text",
            call!("get", QString::from("errorLabel"), QString::from("text"))
        );
        record!("notice-shown", call!("own", QString::from("visible")));
        record!(
            "tone-info-colour",
            call!("get", QString::from("errorLabel"), QString::from("color"))
        );

        // The attach tray: four choices, and the one that sends an app
        // says so and puts the tray away rather than leaving it over the
        // picker it opens. The app entry is the one that has to be asked
        // for -- webxdc is off until the settings page turns it on -- so
        // it is not there until the page says it is.
        record!(
            "tray-load",
            call!(
                "load",
                QString::from(common::component_url("AttachButton.qml"))
            )
        );
        record!("tray-watch", call!("watchTray"));
        call!("set", QString::from("open"), true);
        record!("tray-open", call!("own", QString::from("open")));
    });

    // A tick later, because a child of a tray still fading in reads as
    // invisible whatever its own binding says: an item's `visible` is the
    // parent's too, and the tray's follows a 150ms opacity animation.
    single_shot(Duration::from_secs(6), move || unsafe {
        record!(
            "tray-app-hidden",
            call!("get", QString::from("attachApp"), QString::from("visible"))
        );
        call!("set", QString::from("appsAvailable"), true);
        record!(
            "tray-app-shown",
            call!("get", QString::from("attachApp"), QString::from("visible"))
        );
        record!("tray-tap", call!("tap", QString::from("attachApp")));
        record!("tray-asked", call!("askedSoFar"));
        record!("tray-closed", call!("own", QString::from("open")));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
}

/// Each piece does the one thing it is there for.
#[allow(clippy::too_many_lines)]
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("bar-load"),
        "ok",
        "the reply bar did not load. {context}"
    );
    assert_eq!(
        value("bar-lines"),
        "3",
        "a long quote is not wrapped to three lines. {context}"
    );
    assert_eq!(
        value("bar-truncated"),
        "true",
        "a quote too long for three lines is not cut off. {context}"
    );
    assert!(
        value("bar-height").parse::<f64>().unwrap_or_default() > 0.0,
        "the bar has no height to wrap into. {context}"
    );

    assert_eq!(
        value("jump-load"),
        "ok",
        "the jump button did not load. {context}"
    );
    // Half, deliberately: the theme's own highlight brings transparency of
    // its own, and the two together are unreadable. Compared loosely
    // because a colour's alpha is stored in eight bits.
    let alpha: f64 = value("jump-opaque").parse().unwrap_or_default();
    assert!(
        (alpha - 0.5).abs() < 0.01,
        "the jump button's transparency is not the one it was given: {alpha}. {context}"
    );
    assert!(
        value("jump-chevron").parse::<f64>().unwrap_or_default() > 0.0,
        "the jump button has no chevron drawn on it: {}. {context}",
        value("jump-chevron")
    );
    // A themed icon here is itself a disc, which read as two circles. Asked
    // as "is there one", not "is there one called jumpIcon": the old form
    // held whatever was put back, as long as it was named something else.
    assert_eq!(
        value("jump-themed-icons"),
        "0",
        "the button is back to a themed icon, which brings a second circle. {context}"
    );
    // `Item.Left` is 3; `Item.TopLeft`, which draws it crooked, is 0.
    assert_eq!(
        value("chevron-origin"),
        "3",
        "the chevron's halves turn about their corner, which draws them \
         crooked. {context}"
    );
    assert_eq!(
        value("chevron-mirrored"),
        value("chevron-origin"),
        "the chevron's halves turn about different points. {context}"
    );
    // Lifted by half a stroke, so the line through the middle -- not the
    // top edge -- starts where the corner of the chevron is.
    let left_y: f64 = value("chevron-left-y").parse().unwrap_or_default();
    assert!(
        left_y < 0.0 && (left_y - value("chevron-right-y").parse().unwrap_or(0.0)).abs() < 0.01,
        "the chevron's halves do not sit on the same line: {} and {}. {context}",
        value("chevron-left-y"),
        value("chevron-right-y")
    );
    assert_eq!(
        value("badge-hidden"),
        "false",
        "the button wore a badge with nothing to count. {context}"
    );
    assert_eq!(
        value("badge-shown"),
        "true",
        "messages arrived and the button did not say so. {context}"
    );
    assert_eq!(value("badge-text"), "3", "the badge miscounts. {context}");

    assert_eq!(
        value("banner-load"),
        "ok",
        "the banner did not load. {context}"
    );
    assert_eq!(
        value("notice-text"),
        "Copied to clipboard",
        "a notice does not say what happened. {context}"
    );
    assert_eq!(
        value("notice-shown"),
        "true",
        "a notice with something to say stayed hidden. {context}"
    );
    // Two colours, not the property read back: `tone` exists to change how
    // the strip looks, and an assertion that it holds the value just
    // written to it would pass with the colour binding deleted.
    assert_ne!(
        value("tone-error-colour"),
        value("tone-info-colour"),
        "a confirmation is dressed as a failure: switching tone left the \
         colour where it was ({}). {context}",
        value("tone-info-colour")
    );

    assert_eq!(
        value("tray-load"),
        "ok",
        "the attach tray did not load. {context}"
    );
    assert_eq!(
        value("tray-open"),
        "true",
        "the tray did not open. {context}"
    );
    assert_eq!(
        value("tray-app-hidden"),
        "false",
        "the tray offers an app before webxdc has been asked for, so the \
         setting hides nothing. {context}"
    );
    assert_eq!(
        value("tray-app-shown"),
        "true",
        "the tray still hides the app entry with webxdc turned on, so \
         there is no way to send one. {context}"
    );
    assert_eq!(
        value("tray-tap"),
        "ok",
        "the tray offers no way to send an app. {context}"
    );
    assert_eq!(
        value("tray-asked"),
        "app;",
        "tapping the app entry asked for something else. {context}"
    );
    assert_eq!(
        value("tray-closed"),
        "false",
        "the tray stayed open over the picker it opened. {context}"
    );
}
