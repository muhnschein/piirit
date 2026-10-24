//! "New" page: chat, group, channel, each with its line; tap puts picked page in this one's
//! place (chat opened there then sits on chat list). Channel is group page in channel mode.

// Qt harness: see qml_new_group.rs.
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

// `pageStack` recorded, not performed; see qml_new_group.rs. No core: page asks it nothing.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        property QtObject pageStack: QtObject {
            /// `replace:Foo.qml{...}|...`
            property string log: ''
            function note(verb, page, properties) {
                var name = ('' + page).split('/').pop()
                log = log + verb + ':' + name + JSON.stringify(properties) + '|'
            }
            function push(page, properties) { note('push', page, properties) }
            function replace(page, properties) { note('replace', page, properties) }
        }
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, { accountId: 3 })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
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
        /// Tile's words and line, `|`-joined.
        function words(tile) {
            var item = findIn(loader.item, tile)
            if (!item) { return 'missing:' + tile }
            return findIn(item, 'tileLabel').text + '|' + findIn(item, 'tileHint').text
        }
        function tap(tile) {
            var item = findIn(loader.item, tile)
            if (!item) { return 'missing:' + tile }
            item.clicked()
            return pageStack.log
        }
        function title() { return findIn(loader.item, 'header').title }
    }
";

#[test]
fn the_new_page_offers_chat_group_and_channel() {
    // SAFETY: single-threaded; set pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }
    piirit_shim::register_qml_types();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
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

    let steps: common::Steps = common::Steps::default();
    let taken = steps.clone();
    // Loads, builds tiles, taps: all synchronous once loop runs.
    single_shot(Duration::from_secs(1), move || unsafe {
        let load = call!("load", QString::from(common::page_url("NewPage.qml")));
        common::record(&taken, "load", load.into());
        common::record(&taken, "title", call!("title").into());
        for tile in ["newChatTile", "newGroupTile", "newChannelTile"] {
            common::record(&taken, tile, call!("words", QString::from(tile)).into());
        }
        for tile in ["newChatTile", "newGroupTile", "newChannelTile"] {
            let label = format!("tap-{tile}");
            common::record(&taken, &label, call!("tap", QString::from(tile)).into());
        }
        (*engine_ptr).quit();
    });

    engine.exec();

    let steps = steps.borrow().clone();
    let context = format!("steps: {steps:?}");
    let value = |label: &str| common::value_of(&steps, label).to_string();
    assert_eq!(value("load"), "ok", "the page did not load. {context}");
    assert_eq!(
        value("title"),
        "New",
        "the page is not headed New. {context}"
    );
    assert_eq!(
        value("newChatTile"),
        "New chat|You write to one person.",
        "the chat choice lacks its name or its line. {context}"
    );
    assert_eq!(
        value("newGroupTile"),
        "New group|Several people write to each other.",
        "the group choice lacks its name or its line. {context}"
    );
    assert_eq!(
        value("newChannelTile"),
        "New channel|Channels are a one-to-many tool for broadcasting your messages.",
        "the channel choice lacks its name or Delta Chat's line for it. {context}"
    );
    // Log grows per tap: each reading ends with that tap's move.
    assert!(
        value("tap-newChatTile").ends_with(r#"replace:NewChatPage.qml{"accountId":3}|"#),
        "the chat choice does not open the new chat page in this page's place. {context}"
    );
    assert!(
        value("tap-newGroupTile").ends_with(r#"replace:NewGroupPage.qml{"accountId":3}|"#),
        "the group choice does not open the new group page in this page's place. {context}"
    );
    assert!(
        value("tap-newChannelTile")
            .ends_with(r#"replace:NewGroupPage.qml{"accountId":3,"channel":true}|"#),
        "the channel choice does not open the group page in channel mode in this page's \
         place. {context}"
    );
    assert!(
        !value("tap-newChannelTile").contains("push:"),
        "a choice was pushed on top of this page rather than put in its place. {context}"
    );
}
