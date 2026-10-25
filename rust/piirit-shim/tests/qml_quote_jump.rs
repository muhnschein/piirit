//! Tap on a reply's quote goes to quoted message, as reference clients: lit in place when in
//! this chat, older rows filled on the way; another chat's opens at it over this one (private
//! reply to group message); gone (deleted, or core keeps only text) says so, opens nothing.
//!
//! Page loaded from QML tree copy with `EnterKey` taken out; see
//! `common::qml_tree_without_enter_key`. Steps wait on what they need, not on time.

// Qt harness: see qml_chat_info_attached.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import Piirit 1.0
    Item {
        // Silica's `pageStack`, recorded not performed: which page pushed, at what. In QML, not a
        // Rust QObject (CodeQL flags each added `derive(QObject)`): Loader's page reads it here.
        property QtObject pageStack: QtObject {
            property string log: ''
            function push(page, properties) {
                log += 'push:' + ('' + page).split('/').pop()
                       + '(account=' + properties.accountId + ',chat=' + properties.chatId
                       + ',at=' + properties.findMessageId + ',name=' + properties.chatName
                       + ')|'
            }
            // Info page: not this test's; nothing attached.
            function pushAttached(page, properties) { return null }
            function nextPage(page) { return null }
        }
        Loader { id: loader }
        function load(url) {
            loader.setSource(url, {
                accountId: 1,
                chatId: 1,
                chatName: 'chat 1',
                status: PageStatus.Activating
            })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.status = PageStatus.Active
            return 'ok'
        }

        // Group chat 2 gets a message (id after the long chat's): the one chat 1's 119 quotes.
        ChatMessages { id: elsewhere; account_id: 1 }
        property bool sentElsewhere: false
        Connections {
            target: core
            onStatus_changed: if (core.status === 'ready') elsewhere.chat_id = 2
        }
        Connections {
            target: elsewhere
            onLoaded_changed: if (elsewhere.loaded) elsewhere.send('over there')
            onSent: sentElsewhere = true
        }
        // Core's answers on another chat's message, page's or not.
        property int answeredElsewhere: 0
        Connections {
            target: loader.item ? findIn(loader.item, 'messages') : null
            onQuote_elsewhere: answeredElsewhere += 1
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
        // Built message showing `text`; null while unfetched or off screen.
        function rowWith(node, text) {
            if (!node) { return null }
            if (node.objectName === 'messageDelegate' && node.messageText === text) {
                return node
            }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = rowWith(kids[i], text)
                if (hit) { return hit }
            }
            if (node.contentItem && node.contentItem !== node) {
                return rowWith(node.contentItem, text)
            }
            return null
        }
        function tapQuote(text) {
            var row = rowWith(loader.item, text)
            var area = row ? findIn(row, 'quoteArea') : null
            if (area) { area.clicked(null) }
            return area !== null
        }
        function notice() {
            var label = findIn(loader.item, 'noticeLabel')
            return label ? label.text : 'missing:noticeLabel'
        }
        function clearNotice() { findIn(loader.item, 'notice').text = '' }

        property string phase: 'start'
        property var steps: ({})
        function note(label, value) { steps[label] = '' + value }
        function isFinished() { return '' + (phase === 'done') }
        function stepsJson() { steps.phase = phase; return JSON.stringify(steps) }

        Timer {
            interval: 100; running: true; repeat: true
            onTriggered: {
                var page = loader.item
                var messages = page ? findIn(page, 'messages') : null
                if (!messages || !messages.loaded || !sentElsewhere) { return }
                var list = findIn(page, 'messageList')
                if (phase === 'start') {
                    // Newest page fetched; oldest only a placeholder row.
                    note('oldest-row', messages.row_of(1))
                    note('oldest-built-at-open', rowWith(page, 'message 1') !== null)
                    if (tapQuote('message 118')) { phase = 'gone' }
                } else if (phase === 'gone') {
                    if (notice().length > 0) {
                        note('gone', notice())
                        note('gone-pushed', pageStack.log)
                        clearNotice()
                        // Reader leaves before core answers: nothing pushed over what they went to.
                        if (tapQuote('message 119')) {
                            page.status = PageStatus.Inactive
                            phase = 'left'
                        }
                    }
                } else if (phase === 'left') {
                    if (answeredElsewhere === 1) {
                        note('left-pushed', pageStack.log)
                        page.status = PageStatus.Active
                        if (tapQuote('message 119')) { phase = 'elsewhere' }
                    }
                } else if (phase === 'elsewhere') {
                    if (pageStack.log.length > 0) {
                        note('elsewhere', pageStack.log)
                        note('elsewhere-notice', notice())
                        if (tapQuote('message 120')) { phase = 'older' }
                    }
                } else if (phase === 'older') {
                    var oldest = rowWith(page, 'message 1')
                    if (oldest && oldest.isFound) {
                        note('older-lit', true)
                        note('older-following', list.stickToBottom)
                        note('older-notice', notice())
                        if (tapQuote('message 1')) { phase = 'just-text' }
                    }
                } else if (phase === 'just-text') {
                    if (notice().length > 0) {
                        note('just-text', notice())
                        note('pushes', pageStack.log)
                        // Quote takes own press; long press still row menu.
                        var row = rowWith(page, 'message 1')
                        findIn(row, 'quoteArea').pressAndHold(null)
                        note('hold-menu', findIn(page, 'messageRow') !== null
                                          && list.menuOpen)
                        phase = 'done'
                    }
                }
            }
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_tap_on_a_quote_goes_to_the_quoted_message() {
    let temp = std::env::temp_dir().join(format!("piirit-quote-jump-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");
    let tree = common::qml_tree_without_enter_key();

    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
        // Past first page: oldest is a placeholder until gone to.
        std::env::set_var("PIIRIT_FAKE_LONG_CHAT", "120");
        // 120 quotes oldest; 119 quotes chat 2's message (sent by probe, id 121); 118 one core
        // no longer has. Seeded 1 quotes text only (`JustText`, original deleted).
        std::env::set_var("PIIRIT_FAKE_QUOTES", "120:1,119:121,118:999");
    }

    piirit_shim::register_qml_types();

    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(PROBE_QML));

    core_box
        .pinned()
        .borrow_mut()
        .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut loaded = String::new();
    let loaded_ptr: *mut String = std::ptr::addr_of_mut!(loaded);
    let mut steps = String::new();
    let steps_ptr: *mut String = std::ptr::addr_of_mut!(steps);

    let url = common::page_url_in(&tree, "ConversationPage.qml");
    single_shot(Duration::from_secs(1), move || unsafe {
        *loaded_ptr = QString::from_qvariant(
            (*engine_ptr)
                .invoke_method("load".into(), &[QVariant::from(QString::from(url.clone()))]),
        )
        .map(|value| value.to_string())
        .unwrap_or_default();
        wait_for_steps(engine_ptr, steps_ptr, 60);
    });

    engine.exec();

    assert_eq!(loaded, "ok", "the conversation page did not load");
    let steps: serde_json::Value = serde_json::from_str(&steps).unwrap_or_default();
    let step = |label: &str| {
        steps
            .get(label)
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<step did not run>")
            .to_string()
    };
    let context = format!("steps: {steps}");
    let missing = "Original message not found";

    assert_eq!(step("phase"), "done", "the probe never finished. {context}");
    assert_eq!(
        (
            step("oldest-row").as_str(),
            step("oldest-built-at-open").as_str()
        ),
        ("0", "false"),
        "the oldest message was not a placeholder far up the chat, so going to it proves no \
         older rows are loaded. {context}"
    );
    for (label, expected, complaint) in [
        (
            "gone",
            missing,
            "a quote whose original the core no longer has did not say so",
        ),
        ("gone-pushed", "", "a gone original opened a page"),
        (
            "left-pushed",
            "",
            "another chat was pushed over whatever the reader went to before the core answered",
        ),
        (
            "elsewhere",
            "push:ConversationPage.qml(account=1,chat=2,at=121,name=chat 2)|",
            "a quote of another chat's message did not open that chat, named, at that message",
        ),
        (
            "elsewhere-notice",
            "",
            "a quote found in another chat was also called not found",
        ),
        (
            "older-lit",
            "true",
            "the quoted message, far up the chat, was not fetched and lit",
        ),
        (
            "older-following",
            "false",
            "the view still follows newest, so the next arrival undoes the jump",
        ),
        (
            "older-notice",
            "",
            "a quote found here was also called not found",
        ),
        (
            "just-text",
            missing,
            "a quote kept only as text (original deleted) did not say the original is gone",
        ),
        (
            "pushes",
            "push:ConversationPage.qml(account=1,chat=2,at=121,name=chat 2)|",
            "a quote found here, or gone, opened a page",
        ),
        (
            "hold-menu",
            "true",
            "a long press on a quote did not open the row's menu",
        ),
    ] {
        assert_eq!(step(label), expected, "{complaint}. {context}");
    }
}

/// Each second: probe done, or patience spent? Then take steps and quit. Whole seconds: vendored
/// `single_shot` drops fraction.
fn wait_for_steps(engine: *mut QmlEngine, steps: *mut String, patience: u32) {
    single_shot(Duration::from_secs(1), move || unsafe {
        let ask = |method: &str| {
            QString::from_qvariant((*engine).invoke_method(method.into(), &[]))
                .map(|value| value.to_string())
                .unwrap_or_default()
        };
        if ask("isFinished") == "true" || patience == 0 {
            *steps = ask("stepsJson");
            (*engine).quit();
        } else {
            wait_for_steps(engine, steps, patience - 1);
        }
    });
}
