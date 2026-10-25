//! Emoji in names (chat list, contact rows, search hit titles, chat header, contact and group
//! page name, sender in bubble and quote, contact request bar) drawn as Twemoji, as bubbles
//! draw them, in name's own colour (darker parts away from it): Sailfish's font has few. Pinned per
//! component: name with emoji is escaped `StyledText` with `<img>` of shipped picture, search
//! marks kept, app's own mail mark left the font's; name without stays plain. Name field
//! (`TextField`, no pictures) drawn by a label over it while not edited. Every other name label: `qml_syntax.rs`
//! `every_name_draws_its_emoji_as_pictures`.

// Qt harness: see qml_pages.rs.
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
        function load(url, json) {
            var props = JSON.parse(json)
            props.width = 540
            loader.setSource(url, props)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed:' + url
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
        // Label as drawn: StyledText or not, and its text.
        function label(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return (item.textFormat === Text.StyledText ? 'styled:' : 'plain:') + item.text
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            return item ? '' + item[property] : 'missing:' + name
        }
        function focusField(name) {
            findIn(loader.item, name).focus = true
            return 'ok'
        }
    }
";

/// `<img>` of shipped picture for `size` px font, lit in `tint` (`#rrggbb`), as name drawn.
fn img(picture: &str, size: f64, tint: &str) -> String {
    let file = common::art_dir()
        .join(format!("emoji/{picture}.svg"))
        .canonicalize()
        .unwrap_or_else(|err| panic!("{picture}.svg is not shipped ({err})"));
    #[allow(clippy::cast_possible_truncation)]
    let edge = (size * 1.15).round() as i64;
    format!(
        "<img src=\"image://piirit/emoji?file={}&tint=%23{}&level=0.8\" width=\"{edge}\" \
         height=\"{edge}\" align=\"middle\">",
        encode(&format!("file://{}", file.display())),
        tint.trim_start_matches('#')
    )
}

/// `encodeURIComponent`, for paths: ASCII letters, digits, `-_.!~*'()` kept.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

/// Stub `Theme.primaryColor`: Label's default.
const PRIMARY: &str = "#ffffff";
/// Stub `Theme.secondaryHighlightColor`: empty name field's fallback.
const SECONDARY_HIGHLIGHT: &str = "#6090c0";
/// Stub `Theme.highlightColor`, as Qt writes a colour.
const HIGHLIGHT: &str = "#80c0ff";

#[test]
#[allow(clippy::too_many_lines)]
fn names_draw_their_emoji_as_twemoji() {
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
    macro_rules! load {
        ($file:expr, $url:expr, { $($key:literal : $value:expr),* }) => {
            record!(
                format!("{}-load", $file),
                call!(
                    "load",
                    QString::from($url),
                    QString::from(serde_json::json!({ $($key: $value),* }).to_string().as_str())
                )
            )
        };
    }
    macro_rules! label {
        ($step:expr, $name:expr) => {
            record!($step, call!("label", QString::from($name)))
        };
    }
    macro_rules! set {
        ($property:expr, $value:expr) => {
            call!("set", QString::from($property), $value)
        };
    }

    single_shot(Duration::from_secs(1), move || unsafe {
        let component = |name: &str| common::component_url(name);

        // Unencrypted chat, searched: mark, marks, picture, sender's markup text.
        load!("chat", component("ChatListDelegate.qml"), {
            "chatName": "Climbers 🧗 <crew>", "isEncrypted": false, "match": "crew"
        });
        label!("chat-name", "nameLabel");
        set!("chatName", QString::from("Plain crew"));
        set!("match", QString::from(""));
        label!("chat-plain", "nameLabel");

        load!("contact", component("ContactRow.qml"), {
            "displayName": "Ada 🚀", "isKeyContact": false
        });
        label!("contact-name", "contactName");

        load!("result", component("SearchResultRow.qml"), {
            "title": "Ada 🚀 Lovelace", "titleMatch": "ada"
        });
        label!("result-title", "resultTitle");

        load!("header", component("ConversationHeader.qml"), { "title": "Family 🏡" });
        label!("header-title", "headerTitle");
        set!("title", QString::from("Family"));
        label!("header-plain", "headerTitle");

        // Name field: label over it, field's text see-through; edited, field's own.
        load!("field", component("EditableName.qml"), {
            "text": "Team 🌻", "fieldObjectName": "nameField"
        });
        label!("field-shown", "emojiName");
        record!(
            "field-shown-visible",
            call!("get", QString::from("emojiName"), QString::from("visible"))
        );
        record!(
            "field-text-colour",
            call!("get", QString::from("nameField"), QString::from("color"))
        );
        call!("focusField", QString::from("nameField"));
        record!(
            "field-editing-visible",
            call!("get", QString::from("emojiName"), QString::from("visible"))
        );
        record!(
            "field-editing-colour",
            call!("get", QString::from("nameField"), QString::from("color"))
        );
        // Empty: fallback (their own name) drawn so.
        load!("fallback", component("EditableName.qml"), {
            "fallbackText": "Ada 🚀", "fieldObjectName": "nameField"
        });
        label!("fallback-shown", "emojiName");
        record!(
            "fallback-placeholder-colour",
            call!(
                "get",
                QString::from("nameField"),
                QString::from("placeholderColor")
            )
        );

        load!("bubble", component("MessageDelegate.qml"), {
            "messageText": "hi", "senderName": "Bo 🐝", "showSender": true,
            "senderColor": "#d07030", "quoteText": "earlier", "quoteAuthor": "Cy 🌵"
        });
        label!("bubble-sender", "senderLabel");
        label!("bubble-quote-author", "quoteAuthor");

        load!("request", component("RequestBar.qml"), {
            "shown": true, "chatName": "Dee 🦊"
        });
        label!("request-text", "requestText");
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");
    for which in [
        "chat", "contact", "result", "header", "field", "fallback", "bubble", "request",
    ] {
        assert_eq!(
            value(&format!("{which}-load")),
            "ok",
            "the {which} did not load. {context}"
        );
    }
    let mark = |text: &str| format!("<b><font color=\"{HIGHLIGHT}\">{text}</font></b>");
    assert_eq!(
        value("chat-name"),
        format!(
            "styled:✉ Climbers {} &lt;{}&gt;",
            img("1f9d7", 18.0, PRIMARY),
            mark("crew")
        ),
        "a chat name's emoji is not a picture, its search mark or mail mark got lost, or its \
         text became markup. {context}"
    );
    assert_eq!(
        value("chat-plain"),
        "plain:✉ Plain crew",
        "a chat name without emoji is no longer plain text. {context}"
    );
    assert_eq!(
        value("contact-name"),
        format!("styled:✉ Ada {}", img("1f680", 18.0, PRIMARY)),
        "a contact row's emoji is not a picture. {context}"
    );
    assert_eq!(
        value("result-title"),
        format!(
            "styled:{} {} Lovelace",
            mark("Ada"),
            img("1f680", 18.0, PRIMARY)
        ),
        "a search hit's title loses its marks or its pictures. {context}"
    );
    assert_eq!(
        value("header-title"),
        format!("styled:Family {}", img("1f3e1", 24.0, HIGHLIGHT)),
        "the chat header's emoji is not a picture. {context}"
    );
    assert_eq!(
        value("header-plain"),
        "plain:Family",
        "a chat header without emoji is no longer plain text. {context}"
    );
    assert_eq!(
        value("field-shown"),
        format!("styled:Team {}", img("1f33b", 24.0, HIGHLIGHT)),
        "a name field's emoji is not drawn as a picture over it. {context}"
    );
    assert_eq!(
        (
            value("field-shown-visible"),
            value("field-text-colour").to_lowercase()
        ),
        ("true".to_string(), "#00000000".to_string()),
        "the name field's own text shows through under its pictures. {context}"
    );
    assert_eq!(
        (
            value("field-editing-visible"),
            value("field-editing-colour").to_lowercase()
        ),
        ("false".to_string(), HIGHLIGHT.to_string()),
        "a name being edited is hidden under its pictures. {context}"
    );
    assert_eq!(
        value("fallback-shown"),
        format!("styled:Ada {}", img("1f680", 24.0, SECONDARY_HIGHLIGHT)),
        "an empty name field's fallback name loses its pictures. {context}"
    );
    assert_eq!(
        value("fallback-placeholder-colour").to_lowercase(),
        "#00000000",
        "an empty name field's own fallback shows through under its pictures. {context}"
    );
    assert_eq!(
        value("bubble-sender"),
        format!("styled:Bo {}", img("1f41d", 10.0, "#d07030")),
        "a sender's name in a bubble loses its pictures. {context}"
    );
    assert_eq!(
        value("bubble-quote-author"),
        format!("styled:Cy {}", img("1f335", 10.0, HIGHLIGHT)),
        "a quote's author loses its pictures. {context}"
    );
    assert!(
        value("request-text")
            .starts_with(&format!("styled:Dee {} isn", img("1f98a", 12.0, HIGHLIGHT))),
        "the contact request bar's name loses its pictures. {context}"
    );
}
