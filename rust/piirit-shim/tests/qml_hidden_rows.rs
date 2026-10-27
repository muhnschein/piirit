//! Hidden page keeps rows. Platform hides covered page; `visible` reads effective
//! visibility (false). `visible ? implicitHeight : 0` collapses covered rows, list
//! rebuilds ~100 rows, stalls few hundred ms on swipe. Same for pictures, sound.
//! Pinned: hiding changes neither content measure nor row count.

// Qt harness: see qml_conversation_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

/// Rows filled in (`loaded`), so height is message's; with quote and sender name so every part
/// that can be there is.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: holder
        width: 540
        height: 400

        ListModel { id: rows }

        Loader {
            id: loader
            anchors.fill: parent
        }

        function append(count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    message_id: rows.count + 1, loaded: true,
                    text: 'message number ' + rows.count + ', long enough '
                          + 'to take a line or two of its own in the list',
                    is_outgoing: rows.count % 2 === 0, is_info: false,
                    show_padlock: true, state: 16,
                    timestamp: 1700000000 + rows.count, day_number: 19675,
                    sender_name: 'Ada', sender_color: '#00875a',
                    quote_text: 'earlier', quote_author: 'Grace',
                    file_path: '', file_name: '', view_type: 'Text',
                    image_width: 0, image_height: 0
                })
            }
            return '' + rows.count
        }
        function load(url) {
            loader.setSource(url, { model: rows, showSender: true })
            if (loader.status !== Loader.Ready) { return 'load-failed' }
            loader.item.messageCount = Qt.binding(function() { return rows.count })
            return 'ok'
        }
        function toEnd() { loader.item.positionViewAtEnd(); return 'ok' }
        /// What the platform does to a page with another over it.
        function hide() { holder.visible = false; return 'ok' }
        function show() { holder.visible = true; return 'ok' }
        function measure() {
            var view = loader.item
            view.forceLayout()
            return Math.round(view.contentHeight) + '/'
                   + view.contentItem.children.length + '/'
                   + Math.round(view.contentY)
        }
    }
";

#[test]
fn hiding_the_list_leaves_its_rows_as_they_were() {
    // SAFETY: single-threaded; set pre-Qt.
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

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        (*steps_ptr).push(("rows", call!("append", 60)));
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml"))
            ),
        ));
    });
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("end", call!("toEnd")));
    });
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("before", call!("measure")));
        (*steps_ptr).push(("hide", call!("hide")));
    });
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("hidden", call!("measure")));
        (*steps_ptr).push(("show", call!("show")));
    });
    plan.after(1, move || unsafe {
        (*steps_ptr).push(("shown", call!("measure")));
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
    let before = value("before");
    assert!(
        before.split('/').next().and_then(|n| n.parse::<i64>().ok()) > Some(1000),
        "the rows did not measure anything before being hidden, so this \
         run says nothing about hiding them. {context}"
    );

    // Measures and rows same, hidden or not.
    assert_eq!(
        value("hidden"),
        before,
        "hiding the list changed what its content measures or how many \
         rows it holds (content height/rows/position): the rows collapsed \
         under the page and the list rebuilt itself in the dark. {context}"
    );
    assert_eq!(
        value("shown"),
        before,
        "showing the list again moved its rows or its content, which is \
         the rebuild a reader feels as the swipe back sticking. {context}"
    );
}
