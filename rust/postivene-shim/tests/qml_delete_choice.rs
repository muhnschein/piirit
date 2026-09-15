//! Which kind of delete, asked before anything goes.
//!
//! Deleting a message took it off this account's devices alone; the core
//! will also ask the other ends to delete their copies
//! (`delete_messages_for_all`). The two are different enough -- one of
//! them cannot be taken back by anybody -- that the row menu asks rather
//! than offering both as entries, and this is the dialog it asks in.
//!
//! What it pins is the shape of that question: nothing picked when it
//! opens, one of the two at a time, the second offered only where the
//! core would take it, no forward swipe until something is picked, and
//! an answer that says which was. What it never does is delete
//! anything: accepting starts the wait (`qml_delete_run`), and a dialog
//! left by the back edge says nothing at all.

// Qt harness: see qml_conversation.rs.
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
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        /// What the dialog answered with, if anything.
        property string answer: ''

        function load(url, canForAll) {
            answer = ''
            loader.setSource('', {})
            loader.setSource(url, { canDeleteForEveryone: canForAll })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.picked.connect(function(forEveryone) {
                answer = 'picked:' + forEveryone
            })
            return 'ok'
        }
        function answered() { return answer }
        /// The forward swipe, which Silica only allows while `canAccept`.
        function swipeForward() {
            loader.item.accept()
            return 'ok'
        }
        function canAccept() { return '' + loader.item.canAccept }

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
        /// Whether the dialog draws this at all.
        function has(name) { return '' + (findIn(loader.item, name) !== null) }
        function tap(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
        /// Both switches at once, as `for-me/for-everyone`.
        function switches() {
            return get('forMeSwitch', 'checked') + '/'
                   + get('forEveryoneSwitch', 'checked')
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_dialog_picks_one_of_the_two_and_accepts_only_then() {
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
    let dialog = common::page_url("DeleteMessageDialog.qml");
    let (own, theirs) = (dialog.clone(), dialog.clone());

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
    macro_rules! get {
        ($name:expr, $property:expr) => {
            call!("get", QString::from($name), QString::from($property))
        };
    }

    // This account's own encrypted message: both ways are open.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!("load-own", call!("load", QString::from(own.clone()), true));
        record!("picked-on-open", call!("switches"));
        record!("accept-on-open", call!("canAccept"));
        // Neither the message nor a Cancel button: the chat is one swipe
        // back, and the reader has just come from the message itself.
        record!(
            "shows-message",
            call!("has", QString::from("deletePreview"))
        );
        record!("shows-cancel", call!("has", QString::from("deleteCancel")));
        // Silica's own switch flips itself unless told not to, which
        // would fight the binding that makes these two one answer.
        record!("me-manual", get!("forMeSwitch", "automaticCheck"));
        record!(
            "everyone-manual",
            get!("forEveryoneSwitch", "automaticCheck")
        );
        record!("everyone-offered", get!("forEveryoneSwitch", "enabled"));
        record!("why-not-shown", get!("deleteWhyNot", "visible"));

        record!("tap-me", call!("tap", QString::from("forMeSwitch")));
        record!("picked-me", call!("switches"));
        record!("accept-after-me", call!("canAccept"));
        // The other way instead: one answer, not two settings.
        call!("tap", QString::from("forEveryoneSwitch"));
        record!("picked-everyone", call!("switches"));
        // And off again, which puts it back where it opened.
        call!("tap", QString::from("forEveryoneSwitch"));
        record!("picked-none", call!("switches"));
        record!("accept-after-none", call!("canAccept"));
        record!("answer-so-far", call!("answered"));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        call!("tap", QString::from("forEveryoneSwitch"));
        record!("swipe", call!("swipeForward"));
        record!("answer-everyone", call!("answered"));
    });

    // Somebody else's message, or an unencrypted one: the core deletes
    // it here and nowhere else, so only one way is open.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "load-theirs",
            call!("load", QString::from(theirs.clone()), false)
        );
        record!("theirs-everyone", get!("forEveryoneSwitch", "enabled"));
        record!("theirs-why-not", get!("deleteWhyNot", "visible"));
        call!("tap", QString::from("forMeSwitch"));
        record!("theirs-accept", call!("canAccept"));
        call!("swipeForward");
        record!("answer-me", call!("answered"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
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

    for label in ["load-own", "load-theirs", "tap-me", "swipe"] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    for (label, expected, complaint) in [
        (
            "picked-on-open",
            "false/false",
            "the dialog opens with one of the two ways already picked, so \
             a reader who swipes forward without reading deletes the way \
             this dialog chose for them",
        ),
        (
            "accept-on-open",
            "false",
            "the dialog can be accepted before either way is picked",
        ),
        (
            "shows-message",
            "false",
            "the dialog still shows the message; the reader has just come \
             from it",
        ),
        (
            "shows-cancel",
            "false",
            "the dialog still draws a Cancel button, which the back edge \
             already is",
        ),
        (
            "me-manual",
            "false",
            "the first switch flips itself, which fights the binding that \
             makes the two of them one answer",
        ),
        (
            "everyone-manual",
            "false",
            "the second switch flips itself, which fights the binding that \
             makes the two of them one answer",
        ),
        (
            "everyone-offered",
            "true",
            "deleting one's own encrypted message for everyone is not \
             offered, which is exactly what the core takes",
        ),
        (
            "why-not-shown",
            "false",
            "the dialog says why the second way is greyed while offering it",
        ),
        (
            "picked-me",
            "true/false",
            "picking the first way did not take",
        ),
        (
            "accept-after-me",
            "true",
            "there is no way to accept a way that has been picked",
        ),
        (
            "picked-everyone",
            "false/true",
            "picking the second way left the first one picked too, so what \
             is accepted is whichever the dialog happens to read",
        ),
        (
            "picked-none",
            "false/false",
            "a second tap on the picked way did not put it back",
        ),
        (
            "accept-after-none",
            "false",
            "the dialog can still be accepted after the reader took their \
             answer back",
        ),
        (
            "answer-so-far",
            "",
            "the dialog answered before it was accepted, so picking a way \
             deletes the message by itself",
        ),
        (
            "answer-everyone",
            "picked:true",
            "accepting the second way did not say so, and a message the \
             reader asked to take off everybody's phone would go off their \
             own alone",
        ),
        (
            "theirs-everyone",
            "false",
            "the dialog offers to delete somebody else's message for \
             everyone, which the core refuses",
        ),
        (
            "theirs-why-not",
            "true",
            "the greyed way is greyed with no reason given",
        ),
        (
            "theirs-accept",
            "true",
            "the one way that is open cannot be accepted",
        ),
        (
            "answer-me",
            "picked:false",
            "accepting the first way did not say so",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
}
