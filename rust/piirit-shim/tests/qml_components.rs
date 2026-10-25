//! Small pieces of conversation page, loaded alone (page can't load: no `EnterKey` stub).

// Qt harness: `unsafe` env::set_var pre-Qt (`unused_unsafe`: unsafe only from edition 2024),
// `borrow_as_ptr` for engine pointer.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

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

/// Long enough to overflow 3 lines even at stub's small font; on a phone far less.
const LONG_BODY: &str = "a quoted message long enough to need three lines of its own in \
     the bar above the field, and then a good deal more after that, so that what does \
     not fit has to be cut off rather than run off the side of the screen, which is what \
     it did before -- the label had no wrapping at all, so a single long line simply ran \
     past the edge and out of sight, taking the rest of the quote with it, and none of \
     that is any use to someone trying to see which message they are about to answer";

// Timed steps in order.
#[allow(clippy::too_many_lines)]
#[test]
fn the_reply_bar_wraps_the_jump_button_is_opaque_and_a_notice_is_quiet() {
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "bar-load",
            call!("load", QString::from(common::component_url("ReplyBar.qml")))
        );
        call!("set", QString::from("author"), QString::from("Ada"));
        call!("set", QString::from("body"), QString::from(LONG_BODY));
    });

    plan.after(1, move || unsafe {
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

    plan.after(1, move || unsafe {
        record!(
            "jump-opaque",
            call!("alphaOf", QString::from("jumpDisc"), QString::from("color"))
        );
        record!(
            "jump-chevron",
            call!("get", QString::from("jumpChevron"), QString::from("width"))
        );
        record!("jump-themed-icons", call!("themedIconCount"));
        // `Item.Left`, not `Item.TopLeft`: about the corner, stroke's top edge follows the line
        // and the halves come out crooked.
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

    plan.after(1, move || unsafe {
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
        // Default tone read first: assertion compares two colours.
        record!(
            "tone-error-colour",
            call!("get", QString::from("errorLabel"), QString::from("color"))
        );
        call!("set", QString::from("tone"), QString::from("info"));
    });

    plan.after(1, move || unsafe {
        record!(
            "notice-text",
            call!("get", QString::from("errorLabel"), QString::from("text"))
        );
        record!("notice-shown", call!("own", QString::from("visible")));
        record!(
            "tone-info-colour",
            call!("get", QString::from("errorLabel"), QString::from("color"))
        );

        // Attach tray: four choices; app entry says so and puts tray away, not left over picker it
        // opens. App entry only when asked for (webxdc off until settings page turns it on).
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

    // Tick later: child of tray still fading in reads invisible (`visible` includes parent's; tray
    // follows 150ms opacity animation).
    plan.after(1, move || unsafe {
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

    plan.run();
    engine.exec();

    assert_outcome(&steps);
}

/// Each piece does its one job.
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
    // Half, deliberately: theme highlight brings own transparency; both together unreadable.
    // Loose compare: alpha stored in 8 bits.
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
    // Themed icon is itself a disc: read as two circles. Asked "is there one", not by name: old
    // form passed whatever was put back under another name.
    assert_eq!(
        value("jump-themed-icons"),
        "0",
        "the button is back to a themed icon, which brings a second circle. {context}"
    );
    // `Item.Left` is 3; `Item.TopLeft` is 0.
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
    // Lifted half a stroke: line through middle, not top edge, starts at chevron corner.
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
    // Two colours, not property read back: else passes with binding deleted.
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
