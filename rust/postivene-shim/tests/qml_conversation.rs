//! The message delegate, loaded on its own. `ConversationPage` cannot be
//! loaded headlessly -- Silica's `EnterKey` attached property has no stub --
//! so the part worth testing lives in a component that can be.
//!
//! Reads happen a tick after the write that caused them: a positioner sizes
//! itself in a polish pass, which only runs while the event loop does.

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
            loader.setSource(url, { width: 540 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            loader.item[property] = value
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
        /// Whether something is on the screen. A part the row has not
        /// built at all -- the attachment preview of a message with no
        /// file -- is not shown, the same as one built and hidden.
        function shown(name) {
            var item = findIn(loader.item, name)
            return item ? '' + item.visible : 'false'
        }
        function height() { return '' + loader.item.height }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_message_shows_its_sender_time_quote_and_attachment() {
    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        // Fixes what `Qt.formatTime` renders.
        std::env::set_var("TZ", "UTC");
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
    macro_rules! set {
        ($property:expr, $value:expr) => {
            call!("set", QString::from($property), $value)
        };
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

    // An incoming message in a group, at 1970-01-02 03:04 UTC.
    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
        set!("messageText", QString::from("hello there"));
        set!("senderName", QString::from("Ada Lovelace"));
        set!("senderColor", QString::from("#00875a"));
        set!("showSender", true);
        set!("sentAt", 97_440.0);
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!("sender-shown", get!("senderLabel", "visible"));
        record!("sender-name", get!("senderLabel", "text"));
        record!("footer", get!("footerLabel", "text"));
        record!("quote-hidden", get!("quoteRow", "visible"));
        record!(
            "file-hidden",
            call!("shown", QString::from("attachmentLabel"))
        );
        record!("short-height", call!("height"));

        // A one-to-one chat says nothing about the sender: there is one.
        set!("showSender", false);
        // Wrapping text has to make the row taller, or messages overlap.
        set!(
            "messageText",
            QString::from(
                "a much longer message that has to wrap across several lines \
                 of the delegate, which is the case a fixed row height gets wrong"
            )
        );
        // Outgoing: the delivery mark rides along with the time.
        set!("isOutgoing", true);
        set!("deliveryState", 26);
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("sender-hidden", get!("senderLabel", "visible"));
        record!("delivered-footer", get!("footerLabel", "text"));
        record!("long-height", call!("height"));

        set!("quoteText", QString::from("earlier"));
        set!("quoteAuthor", QString::from("Grace Hopper"));
        set!("filePath", QString::from("/tmp/postivene/note.txt"));
        set!("fileName", QString::from("note.txt"));
        set!("viewType", QString::from("File"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("quote-shown", get!("quoteRow", "visible"));
        record!("quote-text", get!("quoteLabel", "text"));
        record!("file-shown", get!("attachmentLabel", "visible"));
        record!("file-name", get!("attachmentLabel", "text"));

        // An image is shown rather than named.
        set!("viewType", QString::from("Image"));
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("image-shown", get!("attachmentImage", "visible"));
        record!("image-not-named", get!("attachmentLabel", "visible"));

        // A core notice is not a message: no bubble.
        set!("isInfo", true);
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("info-shown", get!("infoLabel", "visible"));
        record!("info-has-no-bubble", get!("bubble", "visible"));
        // Back to an ordinary message, and mark it forwarded. Its own
        // tick: `set` persists, and the info flag hides the whole bubble.
        set!("isInfo", false);
        set!("isForwarded", true);
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!("forwarded-shown", get!("forwardedLabel", "visible"));
        record!("forwarded-text", get!("forwardedLabel", "text"));
        // It has to sit above the quote, not overlap it.
        record!("forwarded-y", get!("forwardedLabel", "y"));
        record!("quote-y", get!("quoteRow", "y"));
        set!("isForwarded", false);
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!("plain-not-forwarded", get!("forwardedLabel", "visible"));
        // Markdown, as the setting wants it: the shim's rendering drawn
        // as StyledText, or the message as written. Anything but 0 is
        // as written -- the 2 an older build stored included.
        set!("messageText", QString::from("**bold** words"));
        set!("styledText", QString::from("<b>bold</b> words"));
        set!("markdownMode", 0);
        record!("drawn-text", get!("messageLabel", "text"));
        record!("drawn-format", get!("messageLabel", "textFormat"));
        set!("markdownMode", 1);
        record!("written-text", get!("messageLabel", "text"));
        record!("written-format", get!("messageLabel", "textFormat"));
        set!("markdownMode", 2);
        record!("stale-text", get!("messageLabel", "text"));
        record!("stale-format", get!("messageLabel", "textFormat"));
        // A message whose text the sender changed afterwards says so in
        // its footer, and the bubble widens to hold the word.
        record!("footer-plain", get!("footerLabel", "text"));
        set!("isEdited", true);
        record!("footer-edited", get!("footerLabel", "text"));
        record!("footer-width", get!("footerLabel", "width"));
        record!("footer-wanted", get!("footerLabel", "implicitWidth"));
        set!("isEdited", false);
        // Drawn, but with nothing rendered to draw: never the raw text as
        // StyledText, which is the case the plain-text pinning exists for.
        set!("markdownMode", 0);
        set!("styledText", QString::from(""));
        record!("unrendered-text", get!("messageLabel", "text"));
        record!("unrendered-format", get!("messageLabel", "textFormat"));
        // A message the download limit held back offers the rest.
        record!("download-hidden", get!("downloadButton", "visible"));
        set!("downloadState", QString::from("Available"));
        record!("download-shown", get!("downloadButton", "visible"));
        record!("download-text", get!("downloadButton", "text"));
        set!("downloadState", QString::from("Done"));
        record!("download-done", get!("downloadButton", "visible"));
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
    assert_forwarded(&steps);
    assert_markdown_and_download(&steps);
}

/// The body follows the Markdown setting, is `StyledText` only when the
/// shim rendered it, and a held-back message offers its rest.
fn assert_markdown_and_download(steps: &[(&str, String)]) {
    let context = format!("steps: {steps:?}");
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    // Text.PlainText is 0 and Text.StyledText is 4.
    assert_eq!(
        (value("drawn-text").as_str(), value("drawn-format").as_str()),
        ("<b>bold</b> words", "4"),
        "with Markdown drawn, the body is not the rendering as StyledText. {context}"
    );
    assert_eq!(
        (
            value("written-text").as_str(),
            value("written-format").as_str()
        ),
        ("**bold** words", "0"),
        "with Markdown off, the body is not the message as written. {context}"
    );
    assert_eq!(
        (value("stale-text").as_str(), value("stale-format").as_str()),
        ("**bold** words", "0"),
        "the value an older build stored for 'as written' no longer reads as \
         written. {context}"
    );
    assert!(
        !value("footer-plain").contains("Edited"),
        "a message nobody changed says it was edited. {context}"
    );
    assert!(
        value("footer-edited").starts_with("Edited"),
        "an edited message does not say so in its footer, got {:?}. {context}",
        value("footer-edited")
    );
    let footer_width: f64 = value("footer-width").parse().unwrap_or(0.0);
    let footer_wanted: f64 = value("footer-wanted").parse().unwrap_or(f64::MAX);
    assert!(
        footer_width >= footer_wanted,
        "the bubble did not widen for the footer, which now hangs out of it: \
         {footer_width} wide for {footer_wanted}. {context}"
    );
    assert_eq!(
        (
            value("unrendered-text").as_str(),
            value("unrendered-format").as_str()
        ),
        ("**bold** words", "0"),
        "a row with nothing rendered drew its raw text as StyledText, which \
         is exactly what the plain-text pinning is for. {context}"
    );
    assert_eq!(
        value("download-hidden"),
        "false",
        "a message with nothing to fetch offers a download. {context}"
    );
    assert_eq!(
        value("download-shown"),
        "true",
        "a message the limit held back offers no way to fetch the rest. {context}"
    );
    assert!(
        value("download-text").contains("Download"),
        "the download offer does not say what it does. {context}"
    );
    assert_eq!(
        value("download-done"),
        "false",
        "the offer stays once the message is fetched. {context}"
    );
}

/// Everything the conversation view has to get right about one message.
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

    assert_eq!(value("load"), "ok", "the delegate did not load. {context}");
    assert_eq!(
        value("sender-shown"),
        "true",
        "a group message did not say who sent it. {context}"
    );
    assert_eq!(
        value("sender-name"),
        "Ada Lovelace",
        "the sender's name is not the one the core gave. {context}"
    );
    assert_eq!(
        value("footer"),
        "03:04",
        "the message carries no time. {context}"
    );
    assert_eq!(
        value("quote-hidden"),
        "false",
        "a message with nothing quoted showed a quote. {context}"
    );
    assert_eq!(
        value("file-hidden"),
        "false",
        "a message with no file showed an attachment. {context}"
    );
    assert_eq!(
        value("sender-hidden"),
        "false",
        "a one-to-one message named its sender anyway. {context}"
    );
    assert_eq!(
        value("delivered-footer"),
        "03:04 ✓",
        "a delivered message of ours does not show it. {context}"
    );

    let short: f64 = value("short-height").parse().unwrap_or_default();
    let long: f64 = value("long-height").parse().unwrap_or_default();
    assert!(
        short > 0.0 && long > short,
        "the row does not grow with its text. {context}"
    );

    assert_eq!(
        value("quote-shown"),
        "true",
        "a quoted message was not shown. {context}"
    );
    assert_eq!(
        value("quote-text"),
        "earlier",
        "the quote is empty. {context}"
    );
    assert_eq!(
        value("file-shown"),
        "true",
        "an attached file was not offered. {context}"
    );
    assert!(
        value("file-name").contains("note.txt"),
        "the attachment is not named. {context}"
    );
    assert_eq!(
        value("image-shown"),
        "true",
        "an image attachment was not shown as one. {context}"
    );
    assert_eq!(
        value("image-not-named"),
        "false",
        "an image was named instead of shown. {context}"
    );
    assert_eq!(
        value("info-shown"),
        "true",
        "a core notice was rendered as a message. {context}"
    );
    assert_eq!(
        value("info-has-no-bubble"),
        "false",
        "a core notice kept its bubble. {context}"
    );
}

/// A forwarded message says so in the sender's own client too.
fn assert_forwarded(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("forwarded-shown"),
        "true",
        "a forwarded message is not marked, so it reads as one written here. {context}"
    );
    assert_eq!(
        value("forwarded-text"),
        "Forwarded",
        "the forwarded marker does not say what it means. {context}"
    );
    assert_eq!(
        value("plain-not-forwarded"),
        "false",
        "an ordinary message claims to be forwarded, so the mark means nothing. {context}"
    );
    let forwarded_y: f64 = value("forwarded-y").parse().unwrap_or(-1.0);
    let quote_y: f64 = value("quote-y").parse().unwrap_or(-1.0);
    assert!(
        forwarded_y >= 0.0 && quote_y > forwarded_y,
        "the marker does not sit above the quote (marker {forwarded_y}, quote {quote_y}). {context}"
    );
}
