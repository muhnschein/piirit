//! The whole of a message, for the page that shows it.
//!
//! A long message does not arrive whole: the sending core cuts the body
//! and puts the rest in an HTML part, so the words the reader is missing
//! are behind `get_message_html` and nowhere else. That is pinned
//! against the real core in `deltachat-jsonrpc/tests/real_server.rs`;
//! what is pinned here is what the app does with it -- that the whole
//! text comes back as words rather than markup, that a message which was
//! never cut costs no second call, and that the page built on it shows
//! what came back.

// Qt harness: see chat_actions.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piiri_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Piiri 1.0
    Item {
        // What a message page reads.
        FullText { id: whole; account_id: 1 }
        function read(id) { whole.message_id = id; return 'ok' }
        function text() { return whole.text }
        function styled() { return whole.styled_text }
        function busy() { return '' + whole.loading }
        function done() { return '' + whole.loaded }

        // The page the reader actually sees it on.
        Loader { id: page; width: 540; height: 900 }
        function openPage(url, id) {
            page.setSource(url, {
                accountId: 1, messageId: id, senderName: 'Ada'
            })
            return page.status === Loader.Ready ? 'ok' : 'load-failed'
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
        function shown(name, property) {
            var item = findIn(page.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn the_rest_of_a_cut_message_comes_back_as_words() {
    let temp = std::env::temp_dir().join(format!("piiri-full-message-{}", std::process::id()));
    let journal = common::fresh_journal(&temp);
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRI_FAKE_JOURNAL", &journal);
        std::env::set_var("PIIRI_ACCOUNTS_DIR", temp.join("accounts"));
    }

    piiri_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*engine_ptr).load_data(QByteArray::from(PROBE_QML));
    });

    // The message the fake seeds as one the sending core cut.
    single_shot(Duration::from_secs(2), move || unsafe {
        record!("ask", call!("read", 11));
        record!("asking", call!("busy"));
    });

    single_shot(Duration::from_secs(4), move || unsafe {
        record!("cut", call!("text"));
        record!("cut-styled", call!("styled"));
        record!("cut-done", call!("done"));
        // A message that was never cut: its own text is the whole of it.
        record!("ask-short", call!("read", 1));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        record!("short", call!("text"));
    });

    let page = common::page_url("MessagePage.qml");
    single_shot(Duration::from_secs(8), move || unsafe {
        record!("page", call!("openPage", QString::from(page.clone()), 11));
    });

    single_shot(Duration::from_secs(10), move || unsafe {
        record!(
            "page-body",
            call!("shown", QString::from("bodyLabel"), QString::from("text"))
        );
        record!(
            "page-sender",
            call!("shown", QString::from("senderLabel"), QString::from("text"))
        );
        record!(
            "page-spinner",
            call!(
                "shown",
                QString::from("loadingIndicator"),
                QString::from("visible")
            )
        );
        record!(
            "page-empty",
            call!(
                "shown",
                QString::from("emptyLabel"),
                QString::from("visible")
            )
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
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("asking"),
        "true",
        "the model did not say it was fetching, so a page has nothing to \
         show a spinner for. {context}"
    );
    assert_eq!(
        value("cut"),
        "Groceries\nmilk\nbread\nand a & sign",
        "the rest of a cut message did not come back as words: its HTML \
         part is what the core holds, and markup is not what a Label may \
         be handed. {context}"
    );
    assert_eq!(
        value("cut-done"),
        "true",
        "the model never finished. {context}"
    );
    // Rendered for the Markdown setting, so the page draws it without
    // going back to the shim -- and with its line breaks as `<br>`,
    // which is the only thing `Text.StyledText` reads as one. The
    // message's four lines arrived as one paragraph until they were:
    // see `qml_message_lines.rs`.
    // No bold in it: the HTML part's heading came back as a plain line
    // of words, so there is no `#` left for the renderer to make one of
    // -- what it renders is the text, not the markup it came from.
    let styled = value("cut-styled");
    assert!(
        styled.contains("Groceries") && !styled.contains("&lt;"),
        "the whole text was not rendered for the Markdown setting: \
         {styled:?}. {context}"
    );
    assert_eq!(
        styled.matches("<br>").count(),
        3,
        "the four lines of the whole text are not four lines: \
         {styled:?}. {context}"
    );

    let short = value("short");
    assert!(
        !short.is_empty() && !short.contains('<'),
        "a message that was never cut did not come back as itself: \
         {short:?}. {context}"
    );

    assert_eq!(
        value("page"),
        "ok",
        "the full-message page did not load. {context}"
    );
    assert_eq!(
        value("page-body"),
        "Groceries\nmilk\nbread\nand a & sign",
        "the page did not show the whole message, which is the only \
         thing it is for. {context}"
    );
    assert_eq!(
        value("page-sender"),
        "Ada",
        "the page does not say who wrote what it is showing. {context}"
    );
    assert_eq!(
        value("page-spinner"),
        "false",
        "the page was still saying it was loading after it had loaded. \
         {context}"
    );
    assert_eq!(
        value("page-empty"),
        "false",
        "the page said the message had no text while showing its text. \
         {context}"
    );

    // A message the core did not cut has no HTML part, and asking for
    // one is a round trip for a string the core has already said is
    // empty. So `hasHtml` decides, and only the cut message is ever
    // asked about -- however many readers there are of it.
    let calls = common::calls(&journal);
    let asked: Vec<&serde_json::Value> = calls
        .iter()
        .filter(|(name, _)| name == "get_message_html")
        .map(|(_, params)| params)
        .collect();
    assert!(
        !asked.is_empty(),
        "the whole text was never asked for, so the page can only be \
         showing the cut version. {calls:?}"
    );
    assert!(
        asked
            .iter()
            .all(|params| params.get(1).and_then(serde_json::Value::as_u64) == Some(11)),
        "the whole text was asked for on a message the core never cut: \
         {asked:?}. {calls:?}"
    );
}
