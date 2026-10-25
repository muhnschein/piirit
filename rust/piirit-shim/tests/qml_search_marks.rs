//! Search marks: found text bold in highlight colour (Marks.js), as vuo, salama. Pinned: any
//! case, every occurrence, peer markup stays text, rows mark only where core matched, plain
//! (not `StyledText`) when nothing found.

// Qt harness: see qml_pages.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use qmetaobject::*;

mod common;

/// Stub `Theme.highlightColor`.
const HIGHLIGHT: &str = "#80c0ff";

/// Probe imports script by absolute URL: data has no dir to resolve.
fn probe_qml() -> String {
    let qml = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml");
    format!(
        r"
    import QtQuick 2.0
    import 'file://{qml}/js/Marks.js' as Marks
    Item {{
        width: 540
        height: 1200

        function marked(text, query) {{ return Marks.marked(text, query, '{HIGHLIGHT}') }}

        // SearchResults stand-in: rows, counts, query.
        ListModel {{
            id: fakeRows
            ListElement {{
                kind: 'chat'; title: 'Anna and Bo'; subtitle: 'ann: hi'
                chat_id: 1; contact_id: 0; message_id: 0; timestamp: 0
                color: '#ff0000'; avatar_path: ''
            }}
            ListElement {{
                kind: 'contact'; title: 'Joanna'; subtitle: 'jo.ann@example.org'
                chat_id: 0; contact_id: 2; message_id: 0; timestamp: 0
                color: '#00ff00'; avatar_path: ''
            }}
            ListElement {{
                kind: 'message'; title: 'Ann chat'; subtitle: 'see Ann later'
                chat_id: 1; contact_id: 0; message_id: 3; timestamp: 0
                color: '#0000ff'; avatar_path: ''
            }}
        }}
        QtObject {{
            id: fakeSearch
            property string query: 'ann'
            property var rows: fakeRows
            property int count: 3
            property int chat_count: 1
            property int contact_count: 1
            property int message_count: 1
            property int message_total: 1
            property bool loaded: true
        }}

        // Row properties per case; `width` added.
        readonly property var rowProps: ({{
            result: {{ title: 'Anna', subtitle: 'no hit here', titleMatch: 'ann',
                      subtitleMatch: 'ann' }},
            chat: {{ chatName: 'Anna', preview: 'ann', isEncrypted: false, match: 'ann' }},
            contact: {{ displayName: 'Joanna', address: 'jo.ann@example.org',
                       showAddress: true, match: 'ann' }},
            unsearched: {{ displayName: 'Joanna' }}
        }})

        Loader {{ id: loader }}
        function load(url, which) {{
            loader.setSource('', {{}})
            var props = {{ width: 540 }}
            var given = rowProps[which]
            for (var key in given) {{
                props[key] = given[key]
            }}
            loader.setSource(url, props)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function loadList(url) {{
            loader.setSource('', {{}})
            loader.setSource(url, {{ width: 540, height: 1200, search: fakeSearch }})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }}
        function findAll(node, name, out) {{
            if (!node) {{ return out }}
            // contentItem also among children: once each.
            if (node.objectName === name && out.indexOf(node) < 0) {{ out.push(node) }}
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                findAll(kids[i], name, out)
            }}
            if (node.contentItem && node.contentItem !== node) {{
                findAll(node.contentItem, name, out)
            }}
            return out
        }}
        function shown(item) {{
            return (item.textFormat === Text.StyledText ? 'styled:' : 'plain:') + item.text
        }}
        // Each label so named as drawn, top to bottom, `|` between.
        function labels(name) {{
            var found = findAll(loader.item, name, [])
            found.sort(function (a, b) {{
                return a.mapToItem(null, 0, 0).y - b.mapToItem(null, 0, 0).y
            }})
            var out = []
            for (var i = 0; i < found.length; i++) {{
                out.push(shown(found[i]))
            }}
            return found.length > 0 ? out.join('|') : 'missing:' + name
        }}
    }}
",
        qml = qml.display()
    )
}

/// `text` as mark draws it.
fn mark(text: &str) -> String {
    format!("<b><font color=\"{HIGHLIGHT}\">{text}</font></b>")
}

#[test]
#[allow(clippy::too_many_lines)]
fn search_results_mark_what_was_found() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(probe_qml().as_str()));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(String, String)> = Vec::new();
    let steps_ptr: *mut Vec<(String, String)> = std::ptr::addr_of_mut!(steps);

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
            (*steps_ptr).push(($label.to_string(), $value))
        };
    }

    // (label, text, query)
    let cases: Vec<(&'static str, &'static str, &'static str)> = vec![
        ("start", "Anna Berg", "ann"),
        ("every", "banana", "AN"),
        ("trimmed", "Joanna", "  ann "),
        ("none", "Bo", "ann"),
        ("no-query", "Anna", ""),
        ("blank-query", "Anna", "   "),
        ("no-text", "", "ann"),
        (
            "peer-markup",
            "<img src=\"https://t/p.gif\"> ann & co",
            "ann",
        ),
        ("markup-query", "if a<b then", "a<b"),
        ("line-break", "hi\nann  there", "ann"),
        ("folded", "HYVÄ ÄÄNI", "ää"),
        ("cyrillic", "Привет, Анна", "анна"),
        // Lowercase two units long: positions don't map back.
        ("unmappable", "\u{130}stanbul", "stan"),
    ];
    let cases_ptr = std::ptr::addr_of!(cases);

    single_shot(Duration::from_secs(0), move || unsafe {
        for (label, text, query) in &*cases_ptr {
            record!(
                *label,
                call!("marked", QString::from(*text), QString::from(*query))
            );
        }
    });

    single_shot(Duration::from_secs(0), move || unsafe {
        // Search result row: title and subtitle each own query.
        record!(
            "result-load",
            call!(
                "load",
                QString::from(common::component_url("SearchResultRow.qml")),
                QString::from("result")
            )
        );
        record!(
            "result-title",
            call!("labels", QString::from("resultTitle"))
        );
        record!(
            "result-subtitle",
            call!("labels", QString::from("resultSubtitle"))
        );

        record!(
            "chat-load",
            call!(
                "load",
                QString::from(common::component_url("ChatListDelegate.qml")),
                QString::from("chat")
            )
        );
        record!("chat-name", call!("labels", QString::from("nameLabel")));
        record!(
            "chat-preview",
            call!("labels", QString::from("previewLabel"))
        );

        record!(
            "contact-load",
            call!(
                "load",
                QString::from(common::component_url("ContactRow.qml")),
                QString::from("contact")
            )
        );
        record!(
            "contact-name",
            call!("labels", QString::from("contactName"))
        );
        record!(
            "contact-address",
            call!("labels", QString::from("contactAddress"))
        );

        record!(
            "unsearched-load",
            call!(
                "load",
                QString::from(common::component_url("ContactRow.qml")),
                QString::from("unsearched")
            )
        );
        record!(
            "unsearched-name",
            call!("labels", QString::from("contactName"))
        );

        record!(
            "list-load",
            call!(
                "loadList",
                QString::from(common::component_url("SearchResultsList.qml"))
            )
        );
        record!("list-titles", call!("labels", QString::from("resultTitle")));
        record!(
            "list-subtitles",
            call!("labels", QString::from("resultSubtitle"))
        );
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    for (label, expected) in [
        ("start", format!("{}a Berg", mark("Ann"))),
        ("every", format!("b{}{}a", mark("an"), mark("an"))),
        ("trimmed", format!("Jo{}a", mark("ann"))),
        ("none", String::new()),
        ("no-query", String::new()),
        ("blank-query", String::new()),
        ("no-text", String::new()),
        // Sender's markup stays text: a picture tag would fetch on draw.
        (
            "peer-markup",
            format!(
                "&lt;img src=\"https://t/p.gif\"&gt; {} &amp; co",
                mark("ann")
            ),
        ),
        ("markup-query", format!("if {} then", mark("a&lt;b"))),
        // StyledText folds line breaks, space runs: kept as Emoji.js keeps them.
        ("line-break", format!("hi<br>{} &nbsp;there", mark("ann"))),
        ("folded", format!("HYVÄ {}NI", mark("ÄÄ"))),
        ("cyrillic", format!("Привет, {}", mark("Анна"))),
        ("unmappable", String::new()),
    ] {
        assert_eq!(value(label), expected, "{label}: marked wrong. {context}");
    }

    for load in [
        "result-load",
        "chat-load",
        "contact-load",
        "unsearched-load",
        "list-load",
    ] {
        assert_eq!(value(load), "ok", "{load} failed. {context}");
    }

    assert_eq!(
        value("result-title"),
        format!("styled:{}a", mark("Ann")),
        "result title not marked. {context}"
    );
    // Nothing found: plain, as before marks.
    assert_eq!(
        value("result-subtitle"),
        "plain:no hit here",
        "unmarked subtitle not drawn plain. {context}"
    );

    // Core matches chat name only; preview is not where it found the chat.
    assert_eq!(
        value("chat-name"),
        format!("styled:✉ {}a", mark("Ann")),
        "chat name not marked. {context}"
    );
    assert_eq!(
        value("chat-preview"),
        "plain:ann",
        "preview marked, though core never searched it. {context}"
    );

    assert_eq!(
        value("contact-name"),
        format!("styled:Jo{}a", mark("ann")),
        "contact name not marked. {context}"
    );
    assert_eq!(
        value("contact-address"),
        format!("styled:jo.{}@example.org", mark("ann")),
        "contact address not marked. {context}"
    );
    assert_eq!(
        value("unsearched-name"),
        "plain:Joanna",
        "row outside search not drawn plain. {context}"
    );

    // Where core matched: chat name; contact name, address; message text, not its chat's name.
    assert_eq!(
        value("list-titles"),
        format!(
            "styled:{}a and Bo|styled:Jo{}a|plain:Ann chat",
            mark("Ann"),
            mark("ann")
        ),
        "result titles marked where core didn't match, or not where it did. {context}"
    );
    assert_eq!(
        value("list-subtitles"),
        format!(
            "plain:ann: hi|styled:jo.{}@example.org|styled:see {} later",
            mark("ann"),
            mark("Ann")
        ),
        "result subtitles marked where core didn't match, or not where it did. {context}"
    );
}
