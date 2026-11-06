//! Avatar initial is first whole char: emoji whole as picture, letter upper-cased. First UTF-16
//! unit was half a surrogate pair, drawn "?" (#246).

// Qt harness: see qml_chat_row.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { width: 80 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
        // Letter and emoji size at `width`: 'letter|emoji'.
        function sizes(width) {
            loader.item.width = width
            loader.item.initial = 'ada'
            var letter = findIn(loader.item, 'avatarInitial').font.pixelSize
            loader.item.initial = '🎉'
            return letter + '|' + findIn(loader.item, 'avatarEmoji').size
        }
        // What stands in for `initial`: 'emoji:X', 'letter:X', or both/neither.
        function drawn(initial) {
            loader.item.initial = initial
            var glyph = findIn(loader.item, 'avatarEmoji')
            var letter = findIn(loader.item, 'avatarInitial')
            var out = []
            if (glyph.visible) { out.push('emoji:' + glyph.emoji) }
            if (letter.visible) { out.push('letter:' + letter.text) }
            return out.join(',')
        }
    }
";

/// Name, and what avatar must draw for it.
const CASES: &[(&str, &str)] = &[
    ("🎉 Party", "emoji:🎉"),
    ("🍕🍺", "emoji:🍕"),
    ("👨‍👩‍👧 Family", "emoji:👨‍👩‍👧"),
    ("👍🏽 ok", "emoji:👍🏽"),
    ("☀️ Sun", "emoji:☀️"),
    ("🇫🇮 Suomi", "emoji:🇫🇮"),
    ("ada", "letter:A"),
    ("Weekend ☀️ Trip", "letter:W"),
    ("مرحبا", "letter:م"),
    ("𝔄lgebra", "letter:𝔄"),
    ("1 on 1", "letter:1"),
];

#[test]
fn an_avatar_draws_the_first_whole_character_of_a_name() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let call = |engine: &mut QmlEngine, name: &str, arg: &str| {
        let result = engine.invoke_method(name.into(), &[QVariant::from(QString::from(arg))]);
        QString::from_qvariant(result)
            .map(|value| value.to_string())
            .unwrap_or_default()
    };

    assert_eq!(
        call(&mut engine, "load", &common::component_url("Avatar.qml")),
        "ok",
        "the avatar did not load"
    );
    let wrong: Vec<String> = CASES
        .iter()
        .filter_map(|(name, want)| {
            let got = call(&mut engine, "drawn", name);
            (got != *want).then(|| format!("{name:?}: drew {got:?}, want {want:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "avatar initials wrong: {wrong:#?}");
}

/// Big page avatars (contact, group, profile) kept list-size letter: tiny in big disc.
#[test]
fn a_big_avatar_draws_a_big_initial() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let mut call = |name: &str, arg: QVariant| {
        let result = engine.invoke_method(name.into(), &[arg]);
        QString::from_qvariant(result)
            .map(|value| value.to_string())
            .unwrap_or_default()
    };

    assert_eq!(
        call(
            "load",
            QString::from(common::component_url("Avatar.qml")).into()
        ),
        "ok",
        "the avatar did not load"
    );
    // Stub Theme: itemSizeSmall 60, fontSizeLarge 24, itemSizeExtraLarge 180.
    for (width, want) in [(60, "24|24"), (40, "24|24"), (360, "144|144")] {
        assert_eq!(
            call("sizes", width.into()),
            want,
            "initial at width {width}: 'letter|emoji' pixel size"
        );
    }
}
