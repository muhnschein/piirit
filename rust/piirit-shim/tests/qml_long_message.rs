//! Message too long for bubble: row shows first few lines, offers rest on own page. Drawn whole it
//! fills screen, a row nobody can scroll past. Page, not unfold in place: unfold makes that same
//! row, and re-folding must put reader back by hand; leaving page puts them back.
//!
//! Pinned where offer appears: not on a short message; always on one the sending core cut (text
//! ends `[...]`), even if what's here fits, since the rest isn't on the phone. Never on an
//! attachment with no caption, nor a message core still holds back (an RPM arriving in an open chat
//! once grew one). Core's `hasHtml` is not "cut": raised for any text not the whole of what arrived
//! (HTML beside plain, HTML-only mail, stripped quote); read as cut, it offered under two-word
//! replies.
//!
//! Offer stays inside bubble: bubble is as wide as widest line, so short lines make a narrow one --
//! sized from offer too. Offer width must not reach body wrap width: that circle (offer widens
//! bubble, body fits, offer goes) made QML drop a binding wherever, so a message drew differently
//! after leaving chat and coming back. Delegate and list loaded alone, as other QML tests.

// Qt harness: see qml_reactions.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

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

/// Same length in short lines: narrow bubble, sized from offer too.
fn a_narrow_message() -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for number in 1..=40 {
        let _ = writeln!(out, "hi {number}");
    }
    out
}

/// Too long to draw whole.
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
    // SAFETY: single thread; pre-Qt.
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "load",
            call!(
                "loadDelegate",
                QString::from(common::component_url("MessageDelegate.qml"))
            )
        );
        // Remark: nearly every message.
        call!(
            "set",
            QString::from("messageText"),
            QString::from("Buy milk")
        );
    });

    plan.after(1, move || unsafe {
        record!(
            "short-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        record!("short-height", call!("ask", QString::from("height")));
        call!("set", QString::from("messageText"), QString::from(long));
    });

    plan.after(1, move || unsafe {
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
        // Core-cut: short here, rest not on phone.
        call!(
            "set",
            QString::from("messageText"),
            QString::from("# Groceries\n[...]")
        );
        call!("set", QString::from("hasHtml"), true);
    });

    plan.after(1, move || unsafe {
        record!(
            "cut-full",
            call!("get", QString::from("fullButton"), QString::from("visible"))
        );
        record!("cut-height", call!("ask", QString::from("height")));
        // Forty short lines: narrow case.
        call!("set", QString::from("hasHtml"), false);
        call!(
            "set",
            QString::from("messageText"),
            QString::from(a_narrow_message())
        );
    });

    plan.after(1, move || unsafe {
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

    plan.after(1, move || unsafe {
        record!("row-full", call!("tapFull"));
        record!("row-raised", call!("raisedSignal"));

        // Attachment, no caption, HTML part: shape that grew offer.
        call!("set", QString::from("messageText"), QString::from(""));
        call!("set", QString::from("hasHtml"), true);
        call!("set", QString::from("viewType"), QString::from("File"));
        call!(
            "set",
            QString::from("filePath"),
            QString::from("/tmp/piirit-long-message/piirit.rpm")
        );
        call!(
            "set",
            QString::from("fileName"),
            QString::from("piirit.rpm")
        );
    });

    plan.after(1, move || unsafe {
        record!(
            "file-actions",
            call!(
                "get",
                QString::from("bodyActions"),
                QString::from("visible")
            )
        );
        // Core holding back: no words yet.
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

    plan.after(1, move || unsafe {
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
        // Two-word mail reply: HTML part beside plain; `hasHtml` on uncut message.
        call!("set", QString::from("downloadState"), QString::from("Done"));
        call!(
            "set",
            QString::from("messageText"),
            QString::from("Will do")
        );
        call!("set", QString::from("hasHtml"), true);
    });

    plan.after(1, move || unsafe {
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
        // One paragraph, mail, nothing cut: reaches bubble limit, where offer's width used to be
        // left behind after offer went.
        call!(
            "set",
            QString::from("messageText"),
            QString::from(
                "A somewhat longer remark, but one paragraph, and nowhere \
                 near the dozen lines the bubble folds at."
            )
        );
    });

    plan.after(1, move || unsafe {
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

    plan.run();
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

    // Inside bubble both ends: negative = offer hanging out.
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

    // `hasHtml` is not "cut": any text not whole (HTML-only mail, forward). Read as cut:
    // offers under two-word replies.
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

    // What offer asks of bubble width once there's no offer. Body used to wrap at width offer had
    // widened, so offer going changed the wrap that decided it should go: a circle QML ends by
    // dropping a binding, leaving look down to property order. Range, not equality: `number` is
    // -1 for an unrecorded step, which must fail.
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
