//! Unfetched row builds no message.
//!
//! List is chat-long; row stands one line until its message arrives. Once a row held a
//! hidden whole message, and fast flicks through unfetched history built and discarded a
//! screenful per frame. Pinned: placeholder holds nothing, keeps height, takes tap silently,
//! holds its message once message arrives.

// Qt harness: see qml_delete_run.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 900

        ListModel { id: rows }
        Loader { id: list; width: 540; height: 900 }

        /// Anything the list asked the page to do.
        property string raised: ''

        // Six messages of one day, the first three fetched.
        function load(url) {
            for (var i = 0; i < 6; i++) {
                rows.append({
                    message_id: i + 1,
                    text: 'message number ' + (i + 1),
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + i,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '',
                    quote_author: '', file_path: '', file_name: '',
                    view_type: 'Text', image_width: 0, image_height: 0,
                    loaded: i < 3
                })
            }
            list.setSource(url, { model: rows })
            if (list.status !== Loader.Ready) { return 'load-failed' }
            var view = list.item
            view.openRequested.connect(function() { raised += 'open;' })
            view.fullTextRequested.connect(function() { raised += 'full;' })
            view.downloadRequested.connect(function() { raised += 'download;' })
            view.appRequested.connect(function() { raised += 'app;' })
            view.callRequested.connect(function() { raised += 'call;' })
            return 'ok'
        }
        function findAll(node, name, out) {
            if (!node) { return out }
            if (node.objectName === name) { out.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                findAll(kids[i], name, out)
            }
            return out
        }
        // The rows top to bottom, each as its height and how many
        // messages it holds: 'height:messages', one per row.
        function rowsNow() {
            var found = findAll(list.item, 'messageRow', [])
            found.sort(function(a, b) { return a.y - b.y })
            var parts = []
            for (var i = 0; i < found.length; i++) {
                parts.push(Math.round(found[i].height) + ':'
                           + findAll(found[i], 'messageDelegate', []).length)
            }
            return parts.join(' ')
        }
        // A tap on the last row, which has nothing in it to open.
        function tapLast() {
            var found = findAll(list.item, 'messageRow', [])
            found.sort(function(a, b) { return a.y - b.y })
            found[found.length - 1].clicked()
            return raised.length > 0 ? raised : 'nothing'
        }
        function arrive(index) {
            rows.setProperty(index, 'loaded', true)
            return 'ok'
        }
    }
";

#[test]
fn a_placeholder_row_holds_no_message_until_it_has_one() {
    // SAFETY: single-threaded; set before Qt starts.
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml"))
            )
        );
    });
    plan.after(1, move || unsafe {
        record!("before", call!("rowsNow"));
        record!("tap", call!("tapLast"));
        // Fourth message arrives.
        record!("arrive", call!("arrive", 3));
    });
    plan.after(1, move || unsafe {
        record!("after", call!("rowsNow"));
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
    let context = format!("steps: {steps:?}");

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    let before: Vec<String> = value("before").split(' ').map(str::to_string).collect();
    let after: Vec<String> = value("after").split(' ').map(str::to_string).collect();
    assert_eq!(
        before.len(),
        6,
        "the list has not got its six rows. {context}"
    );
    assert_eq!(after.len(), 6, "the list lost rows. {context}");

    // `Theme.itemSizeExtraSmall` in stubs: placeholder line height.
    for (index, row) in before.iter().enumerate() {
        let wanted = if index < 3 { ":1" } else { "40:0" };
        assert!(
            row.ends_with(wanted),
            "row {index} is {row:?}, not ...{wanted:?}: a fetched message \
             holds its message, and a placeholder holds nothing and stands \
             one line high. {context}"
        );
    }
    assert_eq!(
        value("tap"),
        "nothing",
        "a tap on a placeholder asked the page to do something. {context}"
    );
    assert_eq!(
        after.get(3),
        before.get(2),
        "the message that arrived is not drawn as the one before it, which \
         says the same in as many words. {context}"
    );
    assert_eq!(
        after.get(4).map(String::as_str),
        Some("40:0"),
        "a placeholder took a message that has not arrived. {context}"
    );
}
