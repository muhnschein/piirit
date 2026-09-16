//! A message too long for the bubble it arrived in.
//!
//! A conversation is a column of remarks, and somebody's to-do document
//! is not one: drawn whole it fills the screen, pushes the rest of the
//! chat out of it, and makes a row nobody can scroll past. So the row
//! shows the first few lines and offers the rest on a page of its own.
//!
//! A page rather than an unfold in place, which would be a second way to
//! read the same words and the worse of the two: it makes exactly the row
//! nobody can scroll past, and folding it again has to put the reader back
//! by hand. The page shows the whole message and leaving it puts them
//! back.
//!
//! What this pins is where the offer appears and where it does not: a
//! short message must not grow two words of chrome it has no use for,
//! and a message the sending core cut must offer the page even when the
//! little it has fits, because the rest of it is not on this phone at
//! all.
//!
//! And that the offer stays inside the bubble. A bubble is as wide as
//! its widest line, so a long message of short lines makes a narrow one
//! -- narrower than the offer under it, until the bubble is sized from
//! the offer too.
//!
//! And what it is *never* about: an attachment. A picture or a document
//! with no caption has no body to read on a page, and a message the core
//! is still holding back has none of it yet -- neither was excluded at
//! first, and an RPM arriving in an open chat grew a View full message
//! it had no use for.
//!
//! Nor is it about a message that merely came from a mail client. The
//! core raises `hasHtml` on anything whose text is not the whole of what
//! arrived -- a plain part with an HTML one beside it, a mail written
//! only in HTML, a reply whose quoted history was stripped -- and taking
//! that for "cut" put the offer under nearly every message in a mailbox,
//! two-word replies included. A message the core really cut ends in its
//! `[...]`, and that is what is asked for here.
//!
//! And that the offer's own width cannot reach the width the body is
//! wrapped at. It could, once, and the circle -- the offer widens the
//! bubble, the wider bubble fits the body, the fitting body sends the
//! offer away -- left QML dropping a binding wherever it happened to
//! notice, so the same message came out one way and then another after
//! leaving the chat and coming back.
//!
//! The delegate and the list are loaded on their own, as the other QML
//! tests load them.

// Qt harness: see qml_reactions.rs.
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
        width: 540
        height: 900

        property string raised: ''

        // The delegate on its own.
        Loader { id: delegate }
        function loadDelegate(url) {
            delegate.setSource(url, { width: 540 })
            return delegate.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function set(property, value) {
            delegate.item[property] = value
            return 'ok'
        }
        function ask(property) { return '' + delegate.item[property] }
        /// Where the offer sits in the bubble: how far its near edge is
        /// from the bubble's, and how far the bubble's far edge is from
        /// its end. A negative either way is an offer hanging out of the
        /// bubble it belongs to.
        function offerInset() {
            var full = findIn(delegate.item, 'fullButton')
            var bubble = findIn(delegate.item, 'bubble')
            if (!full || !bubble) { return 'missing:offer' }
            var start = full.mapToItem(bubble, 0, 0).x
            return Math.round(start) + ':'
                   + Math.round(bubble.width - (start + full.width))
        }
        function bubbleWidth() {
            var bubble = findIn(delegate.item, 'bubble')
            return bubble ? '' + Math.round(bubble.width) : 'missing:bubble'
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
            var item = findIn(delegate.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }

        // The list, over a plain model with one long message in it.
        ListModel { id: rows }
        Loader { id: list; width: 540; height: 900 }
        function loadList(url, longText) {
            rows.append({
                message_id: 7, text: longText, styled_text: '',
                is_edited: false, is_outgoing: false, is_info: false,
                show_padlock: true, state: 16, timestamp: 1700000000,
                day_number: 19675, sender_name: 'Ada',
                sender_color: '#00875a', is_forwarded: false,
                quote_text: '', quote_author: '', file_path: '',
                file_name: '', file_mime: '', file_bytes: 0,
                view_type: 'Text', image_width: 0, image_height: 0,
                is_new: false, has_html: false,
                download_state: 'Done', vcard_name: '', vcard_addr: '',
                vcard_color: '', webxdc_name: '', webxdc_document: '',
                webxdc_summary: '', webxdc_icon: '', reactions: '',
                my_reaction: '', loaded: true
            })
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            list.item.fullTextRequested.connect(function(id, author) {
                raised = 'full:' + id + ':' + author
            })
            return 'ok'
        }
        function rowBody() {
            var row = findIn(list.item, 'messageRow')
            if (!row) { return null }
            return findIn(row, 'bodyActions')
        }
        function tapFull() {
            var actions = rowBody()
            if (!actions) { return 'missing:bodyActions' }
            var button = findIn(actions, 'fullButton')
            if (!button) { return 'missing:fullButton' }
            // The MouseArea inside it, which is what the tap lands on.
            button.children[0].clicked(null)
            return 'ok'
        }
        function raisedSignal() { return raised }
    }
";

/// The same length in short lines. A bubble is as wide as its widest
/// line, so this one comes out narrow -- narrower than the offer under
/// it, which is what the bubble has to be sized from as well.
fn a_narrow_message() -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for number in 1..=40 {
        let _ = writeln!(out, "hi {number}");
    }
    out
}

/// A message nobody would want drawn whole in a bubble.
fn a_long_message() -> String {
    let mut out = String::new();
    for number in 1..=40 {
        use std::fmt::Write as _;
        let _ = writeln!(out, "- [ ] the {number}th thing to do today");
    }
    out
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_long_body_is_cut_to_a_few_lines_with_the_rest_on_a_page() {
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

    let long = a_long_message();
    let long_for_list = long.clone();

    single_shot(Duration::from_secs(1), move || unsafe {
        record!(
            "load",
            call!(
                "loadDelegate",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
        // A remark, which is what nearly every message is.
        call!(
            "set",
            QString::from("messageText"),
            QString::from("Buy milk")
        );
    });

    single_shot(Duration::from_secs(2), move || unsafe {
        record!(
            "short-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!("short-height", call!("ask", QString::from("height")));
        call!(
            "set",
            QString::from("messageText"),
            QString::from(long.clone())
        );
    });

    single_shot(Duration::from_secs(3), move || unsafe {
        record!("long-inset", call!("offerInset"));
        record!("long-bubble", call!("bubbleWidth"));
        record!(
            "long-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!(
            "long-full",
            call!("get", QString::from("fullButton"), QString::from("text"))
        );
        record!(
            "long-lines",
            call!(
                "get",
                QString::from("messageLabel"),
                QString::from("lineCount")
            )
        );
        record!(
            "long-truncated",
            call!(
                "get",
                QString::from("messageLabel"),
                QString::from("truncated")
            )
        );
        record!("collapsed-height", call!("ask", QString::from("height")));
        // A message the core cut: what is here is short, and the rest is
        // not on this phone.
        call!(
            "set",
            QString::from("messageText"),
            QString::from("# Groceries\n[...]")
        );
        call!("set", QString::from("hasHtml"), true);
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!(
            "cut-full",
            call!("get", QString::from("fullButton"), QString::from("visible"))
        );
        record!("cut-height", call!("ask", QString::from("height")));
        // Forty short lines: the bubble is as wide as its widest line, so
        // this is the narrow case the offer must not hang out of.
        call!("set", QString::from("hasHtml"), false);
        call!(
            "set",
            QString::from("messageText"),
            QString::from(a_narrow_message())
        );
    });

    single_shot(Duration::from_secs(5), move || unsafe {
        record!("narrow-inset", call!("offerInset"));
        record!("narrow-bubble", call!("bubbleWidth"));
        record!(
            "list",
            call!(
                "loadList",
                QString::from(common::component_url("ConversationList.qml")),
                QString::from(long_for_list.clone())
            )
        );
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("row-full", call!("tapFull"));
        record!("row-raised", call!("raisedSignal"));

        // An attachment: a file, no caption, and the core saying it has
        // an HTML part -- which is the shape that grew the offer.
        call!("set", QString::from("messageText"), QString::from(""));
        call!("set", QString::from("hasHtml"), true);
        call!("set", QString::from("viewType"), QString::from("File"));
        call!(
            "set",
            QString::from("filePath"),
            QString::from("/tmp/postivene-long-message/postivene.rpm")
        );
        call!(
            "set",
            QString::from("fileName"),
            QString::from("postivene.rpm")
        );
    });

    single_shot(Duration::from_secs(7), move || unsafe {
        record!(
            "file-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        // And a message the core is holding back: a header and a
        // download offer, with none of the words here yet.
        call!("set", QString::from("filePath"), QString::from(""));
        call!("set", QString::from("viewType"), QString::from("Text"));
        call!(
            "set",
            QString::from("messageText"),
            QString::from("a message that has not been fetched")
        );
        call!(
            "set",
            QString::from("downloadState"),
            QString::from("Available")
        );
    });

    single_shot(Duration::from_secs(8), move || unsafe {
        record!(
            "held-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!(
            "held-download",
            call!(
                "get",
                QString::from("downloadButton"),
                QString::from("visible")
            )
        );
        // A two-word reply written in a mail client: its plain part has
        // an HTML one beside it, so the core raises `hasHtml` on a
        // message it never cut and whose every word is in the bubble.
        call!("set", QString::from("downloadState"), QString::from("Done"));
        call!(
            "set",
            QString::from("messageText"),
            QString::from("Will do")
        );
        call!("set", QString::from("hasHtml"), true);
    });

    single_shot(Duration::from_secs(9), move || unsafe {
        record!(
            "mail-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!("mail-cut", call!("ask", QString::from("wasCut")));
        record!(
            "mail-offer-width",
            call!("ask", QString::from("actionsWidth"))
        );
        // One paragraph, still from a mail client, still nothing cut:
        // wide enough to reach the bubble's limit, which is where the
        // offer's width used to be left behind after the offer went.
        call!(
            "set",
            QString::from("messageText"),
            QString::from(
                "A somewhat longer remark, but one paragraph, and nowhere \
                 near the dozen lines the bubble folds at."
            )
        );
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!(
            "paragraph-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!(
            "paragraph-offer-width",
            call!("ask", QString::from("actionsWidth"))
        );
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
    let number = |label: &str| value(label).parse::<f64>().unwrap_or(-1.0);
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the delegate did not load. {context}");

    assert_eq!(
        value("short-actions"),
        "false",
        "a message of three words offered a page of its own. {context}"
    );

    assert_eq!(
        value("long-actions"),
        "true",
        "a forty-line message offered nothing: the reader can see the \
         first lines of it and has no way to the rest. {context}"
    );
    assert_eq!(
        value("long-truncated"),
        "true",
        "the bubble drew the whole of a forty-line message. {context}"
    );
    assert_eq!(
        value("long-full"),
        "View full message",
        "the offer on a long message does not say what it does. {context}"
    );
    let collapsed_lines = number("long-lines");
    assert!(
        collapsed_lines > 1.0 && collapsed_lines <= 12.0,
        "the body is {collapsed_lines} lines, not the dozen the row caps \
         it at. {context}"
    );

    // Inside the bubble at both ends: a negative either way is the offer
    // hanging out of it.
    let inset = |label: &str| -> (f64, f64) {
        let text = value(label);
        let mut parts = text.split(':').map(|part| part.parse().unwrap_or(-1.0));
        (parts.next().unwrap_or(-1.0), parts.next().unwrap_or(-1.0))
    };
    for label in ["long-inset", "narrow-inset"] {
        let (start, end) = inset(label);
        assert!(
            start >= 0.0 && end >= 0.0,
            "the offer hangs out of the bubble on {label}: {start} in from \
             its start, {end} in from its end. The bubble is as wide as \
             its widest line, so it has to be sized from the offer as \
             well. {context}"
        );
    }
    assert!(
        number("narrow-bubble") > 0.0,
        "the narrow bubble was not measured. {context}"
    );

    assert_eq!(
        value("cut-full"),
        "true",
        "a message the sending core cut did not offer the page, so its \
         `[...]` is the end of it as far as the reader can tell. {context}"
    );

    // `hasHtml` is not "cut". The core raises it for any message whose
    // text is not the whole of what arrived -- a plain part with an HTML
    // one beside it, a mail written only in HTML, a stripped quote, a
    // forward -- which is very nearly everything written in a mail
    // client. Read as "cut", it put the offer under two-word replies,
    // each one opening a page with the words already in the bubble.
    assert_eq!(
        value("mail-cut"),
        "false",
        "a two-word reply with an HTML part beside its text was taken \
         for a message the core had cut. {context}"
    );
    assert_eq!(
        value("mail-actions"),
        "false",
        "a two-word reply offered a page of its own because it came \
         from a mail client. {context}"
    );
    assert_eq!(
        value("paragraph-actions"),
        "false",
        "a one-paragraph mail the bubble drew whole offered a page with \
         the same paragraph on it. {context}"
    );

    // What the offer asks of the bubble's width, once there is no offer.
    // The body used to be wrapped at a width the offer had widened, so
    // the offer's going changed the wrapping that decided it had to go:
    // a circle, which QML ends by dropping a binding where it stands.
    // What that left was a bubble held open by an offer that was not
    // there, and a row whose look came down to the order its properties
    // arrived in -- the same message drawn one way, and another way
    // after leaving the chat and coming back.
    // A range rather than an equality: `number` answers -1 for a step
    // that was never recorded, which a "not a pixel wide" test has to
    // fail rather than pass.
    for label in ["mail-offer-width", "paragraph-offer-width"] {
        let asked = number(label);
        assert!(
            (0.0..1.0).contains(&asked),
            "with no offer shown, {label} is {asked}, the width of one: \
             the bubble is sized from an offer it does not have. {context}"
        );
    }

    assert_eq!(value("list"), "ok", "the list did not load. {context}");
    assert_eq!(value("row-full"), "ok", "the row has no offer. {context}");
    assert_eq!(
        value("row-raised"),
        "full:7:Ada",
        "asking for the page did not name the message and who wrote it. \
         {context}"
    );

    assert_eq!(
        value("file-actions"),
        "false",
        "an attachment with no caption offered a page of its own -- there \
         is no body there to read on one. {context}"
    );
    assert_eq!(
        value("held-actions"),
        "false",
        "a message the core is still holding back offered the rest of \
         its words, which are not on this phone yet. {context}"
    );
    assert_eq!(
        value("held-download"),
        "true",
        "the one offer a held-back message should carry is gone. \
         {context}"
    );
}
