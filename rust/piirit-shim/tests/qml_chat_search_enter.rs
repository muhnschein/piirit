//! Enter in the chat list's search field closes the keyboard and keeps the
//! search.
//!
//! The field drops focus through `page.dismissKeyboard()`, and the query
//! and the results stay where they were: the page's `onActiveFocusChanged`
//! only closes the search when the field is left empty.
//!
//! The Enter key itself cannot be injected in this harness -- QML key
//! events are not delivered to items here -- so the wiring is checked
//! against the source instead: the `chatSearchField` block must hand
//! both `Keys.onReturnPressed` and `Keys.onEnterPressed` to
//! `page.dismissKeyboard()`. The function is exercised directly through
//! the probe.

// Qt harness: see qml_chat_list.rs.
#![allow(
    unsafe_code,
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]

use std::path::PathBuf;
use std::time::Duration;

use piirit_shim::DeltaChatCore;
use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    import QtQuick.Window 2.0
    // A shown window, so the page's field can hold active focus. Without
    // one, an item has focus but never active focus.
    Window {
        visible: true
        width: 480
        height: 800
        Loader { id: loader; anchors.fill: parent }
        function load(url, accountId, archived) {
            loader.setSource('', {})
            loader.setSource(url, { accountId: accountId, archived: archived })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        // `data` rather than `children`: the model is a plain QObject, so
        // it is not among an Item's visual children at all.
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            // A list's header and delegates hang off contentItem.
            if (node.contentItem && node.contentItem !== node) {
                return findIn(node.contentItem, name)
            }
            return null
        }
        function setText(name, value) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.text = value
            return 'ok'
        }
        function focus(name) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            item.forceActiveFocus()
            return 'ok'
        }
        function dismiss() {
            if (!loader.item) { return 'no-page' }
            loader.item.dismissKeyboard()
            return 'ok'
        }
        function page(property) {
            if (!loader.item) { return 'no-page' }
            return '' + loader.item[property]
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
    }
";

#[test]
fn enter_closes_the_keyboard_and_keeps_the_search() {
    let temp =
        std::env::temp_dir().join(format!("piirit-chat-search-enter-{}", std::process::id()));
    std::fs::create_dir_all(temp.join("accounts")).expect("create temp dirs");

    // SAFETY: single-threaded test binary; set before Qt starts.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        // The probe's window needs a scene graph; the software one needs no
        // GL context, which a CI runner without a display does not have.
        std::env::set_var("QT_QUICK_BACKEND", "software");
        std::env::set_var("PIIRIT_ACCOUNTS_DIR", temp.join("accounts"));
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

    single_shot(Duration::from_secs(1), move || unsafe {
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::page_url("ChatListPage.qml")),
                1,
                false
            ),
        ));
    });

    // Text in the field, and the field holds the keyboard.
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push((
            "typed",
            call!(
                "setText",
                QString::from("chatSearchField"),
                QString::from("chat 2")
            ),
        ));
        (*steps_ptr).push(("focused", call!("focus", QString::from("chatSearchField"))));
    });

    // Past the 250ms debounce: the query has reached the model. Enter
    // is then pressed, and the result is read straight after.
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push((
            "focus-before",
            call!(
                "get",
                QString::from("chatSearchField"),
                QString::from("activeFocus")
            ),
        ));
        (*steps_ptr).push(("dismiss", call!("dismiss")));
        (*steps_ptr).push((
            "focus-after",
            call!(
                "get",
                QString::from("chatSearchField"),
                QString::from("activeFocus")
            ),
        ));
        (*steps_ptr).push((
            "text-after",
            call!(
                "get",
                QString::from("chatSearchField"),
                QString::from("text")
            ),
        ));
        (*steps_ptr).push((
            "search-open-after",
            call!("page", QString::from("searchOpen")),
        ));
        (*steps_ptr).push((
            "query-after",
            call!("get", QString::from("search"), QString::from("query")),
        ));
    });

    single_shot(Duration::from_secs(6), move || unsafe {
        (*engine_ptr).quit();
    });

    engine.exec();

    assert_outcome(&steps);
    assert_source_wires_enter();
}

/// Enter closes the keyboard, and the search stays open with its query.
fn assert_outcome(steps: &[(&str, String)]) {
    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the chat list page did not load. {context}"
    );
    assert_eq!(
        value("typed"),
        "ok",
        "the search field was not found on the page. {context}"
    );
    assert_eq!(
        value("focused"),
        "ok",
        "the search field could not take focus. {context}"
    );
    assert_eq!(
        value("focus-before"),
        "true",
        "the search field did not hold focus before Enter, so the test proves \
         nothing about closing the keyboard. {context}"
    );
    assert_eq!(
        value("dismiss"),
        "ok",
        "dismissKeyboard could not be called on the page. {context}"
    );
    assert_eq!(
        value("focus-after"),
        "false",
        "Enter did not close the keyboard: the search field still has focus. {context}"
    );
    assert_eq!(
        value("text-after"),
        "chat 2",
        "closing the keyboard cleared the query. {context}"
    );
    assert_eq!(
        value("search-open-after"),
        "true",
        "closing the keyboard closed the search. {context}"
    );
    assert_eq!(
        value("query-after"),
        "chat 2",
        "closing the keyboard dropped the query the results are for. {context}"
    );
}

/// The field's block in the page hands both Enter keys to
/// `page.dismissKeyboard()`, and the page defines it.
fn assert_source_wires_enter() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/pages/ChatListPage.qml");
    let source = std::fs::read_to_string(&path).expect("read ChatListPage.qml");

    assert!(
        source.contains("function dismissKeyboard() {"),
        "ChatListPage.qml does not define dismissKeyboard()"
    );

    let marker = source
        .find("objectName: \"chatSearchField\"")
        .expect("chatSearchField is not in ChatListPage.qml");
    let open = source[..marker]
        .rfind("SearchField {")
        .expect("no SearchField { opens the chatSearchField block");
    let body_start = open + source[open..].find('{').expect("brace");
    let mut depth = 0usize;
    let mut end = None;
    for (offset, byte) in source[body_start..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(body_start + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let block = &source[open..end.expect("chatSearchField block closes")];

    for handler in [
        "Keys.onReturnPressed: page.dismissKeyboard()",
        "Keys.onEnterPressed: page.dismissKeyboard()",
    ] {
        assert!(
            block.contains(handler),
            "the chatSearchField block lacks `{handler}`, so Enter leaves the keyboard up"
        );
    }
}
