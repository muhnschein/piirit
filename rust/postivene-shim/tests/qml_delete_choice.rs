//! Which kind of delete, asked on a page of its own.
//!
//! Deleting a message used to be one entry on a row's menu that took the
//! message off this phone alone. The core will also ask the other ends
//! to delete their copies (`delete_messages_for_all`), and the two are
//! different enough -- one of them is irreversible for everybody in the
//! chat -- that the choice is a page rather than a second menu entry
//! next to the first.
//!
//! So this pins what that page does: it shows the message about to go,
//! it offers both ways, it offers the second only where the core would
//! take it, and it answers with which was picked. What it never does is
//! delete anything: the page it was pushed from starts the wait on the
//! answer (`qml_delete_run`), and leaving without picking says nothing
//! at all.

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

/// The page stack is a QML object handed over as a context property,
/// for the reason `qml_auto_delete_flow` gives: an object a method hands
/// to QML is QML's to delete.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    Item {
        Loader { id: loader }
        /// What the page answered with, and how many times it took
        /// itself off the stack.
        property string answer: ''
        property int pops: 0

        QtObject {
            id: stack
            function pop() { pops += 1 }
        }
        function stackObject() { return stack }

        function load(url, body, fileName, author, canForAll) {
            answer = ''
            pops = 0
            loader.setSource('', {})
            loader.setSource(url, {
                senderName: author,
                body: body,
                fileName: fileName,
                canDeleteForEveryone: canForAll
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.picked.connect(function(forEveryone) {
                answer = 'picked:' + forEveryone
            })
            return 'ok'
        }
        /// What was answered, and whether the page left with it.
        function answered() { return answer + '/' + pops }

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
        function click(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.clicked()
            return 'ok'
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_page_shows_the_message_and_answers_with_which_delete_was_picked() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));
    // Named for the page before it is loaded, as Silica names its own.
    let stack = engine.invoke_method("stackObject".into(), &[]);
    engine.set_property("pageStack".into(), stack);

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);
    let page = common::page_url("DeleteMessagePage.qml");
    let (with_file, empty, own) = (page.clone(), page.clone(), page.clone());

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

    // Somebody else's message: the core deletes it here and nowhere
    // else, so only one of the two ways is open.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load-theirs",
            call!(
                "load",
                QString::from(page.clone()),
                QString::from("see you there"),
                QString::from(""),
                QString::from("Ada Lovelace"),
                false
            )
        );
        record!("sender", get!("deleteSender", "text"));
        record!("preview", get!("deletePreview", "text"));
        record!("preview-plain", get!("deletePreview", "textFormat"));
        record!("empty-shown", get!("deleteEmpty", "visible"));
        record!("for-me-offered", get!("forMeTile", "enabled"));
        record!("for-everyone-offered", get!("forEveryoneTile", "enabled"));
        record!("why-not-shown", get!("deleteWhyNot", "visible"));
        record!("pick-for-me", call!("click", QString::from("forMeTile")));
        record!("answer-for-me", call!("answered"));
    });

    // An attachment with no caption is shown by what it carries: a page
    // asking whether to delete "" is a page asking about nothing.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "load-file",
            call!(
                "load",
                QString::from(with_file.clone()),
                QString::from(""),
                QString::from("notes.pdf"),
                QString::from("Ada Lovelace"),
                false
            )
        );
        record!("file-preview", get!("deletePreview", "text"));
        record!("cancel", call!("click", QString::from("deleteCancel")));
        record!("answer-after-cancel", call!("answered"));
    });

    // A message with neither words nor a name to show says so, rather
    // than leaving the reader a page with a gap in it.
    single_shot(Duration::from_secs(3), move || unsafe {
        record!(
            "load-empty",
            call!(
                "load",
                QString::from(empty.clone()),
                QString::from(""),
                QString::from(""),
                QString::from(""),
                false
            )
        );
        record!("empty-preview-shown", get!("deletePreview", "visible"));
        record!("empty-said", get!("deleteEmpty", "visible"));
        record!("sender-shown", get!("deleteSender", "visible"));
    });

    // This account's own encrypted message: both ways are open, and the
    // second is the one the core has a call of its own for.
    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "load-own",
            call!(
                "load",
                QString::from(own.clone()),
                QString::from("on my way"),
                QString::from(""),
                QString::from("Me"),
                true
            )
        );
        record!("own-for-everyone", get!("forEveryoneTile", "enabled"));
        record!("own-why-not", get!("deleteWhyNot", "visible"));
        record!(
            "pick-for-everyone",
            call!("click", QString::from("forEveryoneTile"))
        );
        record!("answer-for-everyone", call!("answered"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
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

    for label in [
        "load-theirs",
        "load-file",
        "load-empty",
        "load-own",
        "pick-for-me",
        "pick-for-everyone",
        "cancel",
    ] {
        assert_eq!(value(label), "ok", "step {label} failed. {context}");
    }

    for (label, expected, complaint) in [
        (
            "sender",
            "Ada Lovelace",
            "the page does not say whose message is about to go",
        ),
        (
            "preview",
            "see you there",
            "the page does not show the message it is asking about",
        ),
        (
            "preview-plain",
            "0",
            "the message is not drawn as plain text, so Qt decides for \
             itself whether what somebody sent is markup",
        ),
        (
            "empty-shown",
            "false",
            "a message with words in it is called empty",
        ),
        ("for-me-offered", "true", "there is no way to delete at all"),
        (
            "for-everyone-offered",
            "false",
            "the page offers to delete somebody else's message for \
             everyone, which the core refuses",
        ),
        (
            "why-not-shown",
            "true",
            "the greyed way is greyed with no reason given",
        ),
        (
            "answer-for-me",
            "picked:false/1",
            "picking the first way did not answer with it, or did not \
             take the page back off the stack",
        ),
        (
            "file-preview",
            "notes.pdf",
            "a message with no words is shown as nothing at all, rather \
             than by what it carries",
        ),
        (
            "answer-after-cancel",
            "/1",
            "Cancel deleted something, or did not leave the page",
        ),
        (
            "empty-preview-shown",
            "false",
            "a message with nothing to show draws an empty line anyway",
        ),
        (
            "empty-said",
            "true",
            "a message with nothing to show leaves the page with a gap in \
             it instead of saying so",
        ),
        (
            "sender-shown",
            "false",
            "a message with no sender draws an empty line for one",
        ),
        (
            "own-for-everyone",
            "true",
            "the page does not offer to delete this account's own \
             encrypted message for everyone, which is exactly what the \
             core takes",
        ),
        (
            "own-why-not",
            "false",
            "the page says why the second way is greyed while offering it",
        ),
        (
            "answer-for-everyone",
            "picked:true/1",
            "picking the second way did not answer with it, so a message \
             the reader asked to take off everybody's phone would go off \
             their own alone",
        ),
    ] {
        assert_eq!(value(label), expected, "{complaint}. {context}");
    }
}
