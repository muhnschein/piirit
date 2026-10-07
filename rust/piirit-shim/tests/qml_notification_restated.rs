//! A notification follows its chat after it is up: an edited message is
//! the edited text on it, a deleted one comes off it, and once nothing
//! there is unread the notification comes down. All of it in place, with
//! no banner, since nothing arrived.

// Qt harness: see qml_chat_row.rs.
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
            loader.setSource(url, {})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
            return 'ok'
        }
        function arrived(chatId, name, sender, preview) {
            loader.item.arrived(chatId, name, sender, preview)
            return 'ok'
        }
        function restated(chatId, name, sender, preview, unread) {
            loader.item.restated(chatId, name, sender, preview, unread)
            return 'ok'
        }
        // What one chat's notification is saying, how many it counts,
        // what pops up over the top, and how often it was published.
        function state(chatId) {
            var note = loader.item.notes[chatId]
            if (!note) { return 'none' }
            return note.summary + '/' + note.body + '/' + note.itemCount
                + '/' + note.previewBody + '/' + note.publishCount
                + '/' + note.isPublished
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_notification_follows_edits_and_deletions_in_its_chat() {
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
    macro_rules! arrived {
        ($chat:expr, $preview:expr) => {
            call!(
                "arrived",
                $chat,
                QString::from("Ada"),
                QString::from(""),
                QString::from($preview)
            )
        };
    }
    macro_rules! restated {
        ($chat:expr, $preview:expr, $unread:expr) => {
            call!(
                "restated",
                $chat,
                QString::from("Ada"),
                QString::from(""),
                QString::from($preview),
                $unread
            )
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!("load", QString::from(common::component_url("Notifier.qml")))
        );
        call!("set", QString::from("appActive"), false);
        call!("set", QString::from("detail"), 0);

        // Edited: the new text, in place, no banner.
        arrived!(7, "see you there");
        record!("arrived", call!("state", 7));
        restated!(7, "see you tomorrow", 1);
        record!("edited", call!("state", 7));
        // Restated again with nothing changed: published no further.
        restated!(7, "see you tomorrow", 1);
        record!("unchanged", call!("state", 7));

        // Two up, one deleted: the one before is what it says, counting
        // one; then the other deleted too, and it comes down.
        arrived!(9, "first");
        arrived!(9, "second");
        restated!(9, "first", 1);
        record!("one-deleted", call!("state", 9));
        restated!(9, "older message", 0);
        record!("all-deleted", call!("state", 9));

        // A chat with nothing up says nothing.
        restated!(11, "edited elsewhere", 1);
        record!("never-up", call!("state", 11));
        // Nor does one whose notification the reader took down.
        restated!(9, "edited later", 2);
        record!("after-down", call!("state", 9));
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps.iter().find(|(step, _)| *step == label).map_or_else(
            || panic!("no step {label}. {context}"),
            |(_, value)| value.clone(),
        )
    };
    assert_eq!(value("load"), "ok", "{context}");
    assert_eq!(
        value("arrived"),
        "Ada/see you there/1/see you there/1/true",
        "{context}"
    );
    assert_eq!(
        value("edited"),
        "Ada/see you tomorrow/1//2/true",
        "an edit did not change the notification in place, without a banner. {context}"
    );
    assert_eq!(
        value("unchanged"),
        "Ada/see you tomorrow/1//2/true",
        "a restatement that changes nothing published again. {context}"
    );
    assert_eq!(
        value("one-deleted"),
        "Ada/first/1//3/true",
        "a deleted message is still counted, or still said. {context}"
    );
    assert_eq!(
        value("all-deleted"),
        "Ada/first/1//3/false",
        "the notification stayed up with nothing unread under it. {context}"
    );
    assert_eq!(
        value("never-up"),
        "none",
        "a restatement raised a notification. {context}"
    );
    assert_eq!(
        value("after-down"),
        "Ada/first/1//3/false",
        "a notification taken down came back up on a restatement. {context}"
    );
}
