//! "New messages" line: one row carries it, the row the model points at.
//!
//! Drawn inside row, not as section delegate, as day heading is (`ConversationList.qml`):
//! row it lands on must grow by it. Line drawn over message below is worse than none.

// Qt harness: see qml_conversation_list.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

/// Plain `ListModel` with real roles.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        width: 540
        height: 800

        ListModel { id: rows }
        Loader { id: loader; anchors.fill: parent }

        function load(url, count) {
            for (var i = 0; i < count; i++) {
                rows.append({
                    message_id: rows.count + 1,
                    text: 'message number ' + (rows.count + 1),
                    is_outgoing: false, is_info: false, show_padlock: true,
                    state: 16, timestamp: 1700000000 + rows.count,
                    day_number: 19675, sender_name: 'Ada',
                    sender_color: '#00875a', quote_text: '', quote_author: '',
                    file_path: '', file_name: '', view_type: 'Text',
                    image_width: 0, image_height: 0, loaded: true
                })
            }
            loader.setSource(url, { model: rows })
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function setMark(id) {
            loader.item.unreadFrom = id
            return '' + loader.item.unreadFrom
        }
        function allIn(node, name, found) {
            if (!node) { return found }
            if (node.objectName === name) { found.push(node) }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                allIn(kids[i], name, found)
            }
            if (node.contentItem && node.contentItem !== node) {
                allIn(node.contentItem, name, found)
            }
            return found
        }
        function findIn(node, name) {
            var found = allIn(node, name, [])
            return found.length > 0 ? found[0] : null
        }
        // The rows, once each and in the order they are drawn. A
        // ListItem is reached both as a child and through its own
        // contentItem, so the walk finds every row twice.
        function rowsInOrder() {
            var found = allIn(loader.item, 'messageRow', [])
            var once = []
            for (var i = 0; i < found.length; i++) {
                if (once.indexOf(found[i]) === -1) { once.push(found[i]) }
            }
            once.sort(function (one, other) { return one.y - other.y })
            return once
        }
        // Whether each row carries the line: '010' is the middle row of
        // three.
        function pattern() {
            var out = []
            var found = rowsInOrder()
            for (var i = 0; i < found.length; i++) {
                var line = findIn(found[i], 'unreadLine')
                if (!line) { return 'missing:unreadLine' }
                out.push(line.visible ? '1' : '0')
            }
            return out.join('')
        }
        // What the line says, and how much room it takes on the row that
        // has it against a row that does not.
        function said() {
            var label = findIn(loader.item, 'unreadLabel')
            return label ? label.text : 'missing:unreadLabel'
        }
        function heights() {
            var out = []
            var found = rowsInOrder()
            for (var i = 0; i < found.length; i++) {
                out.push(Math.round(found[i].contentHeight))
            }
            return out.join(',')
        }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn one_row_carries_the_line_and_makes_room_for_it() {
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

    let list = common::component_url("ConversationList.qml");
    plan.after(1, move || unsafe {
        record!("load", call!("load", QString::from(list.clone()), 3));
        // Nothing unread: no line, every row same height.
        record!("quiet", call!("pattern"));
        record!("quiet-heights", call!("heights"));
        record!("mark", call!("setMark", 2));
    });

    plan.after(2, move || unsafe {
        record!("marked", call!("pattern"));
        record!("marked-heights", call!("heights"));
        record!("said", call!("said"));
        // Line moves with mark.
        call!("setMark", 3);
    });

    plan.after(1, move || unsafe {
        record!("moved", call!("pattern"));
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
    assert_eq!(
        value("quiet"),
        "000",
        "a chat with nothing new in it has a line in it. {context}"
    );
    assert_eq!(
        value("marked"),
        "010",
        "the line is not on the row the model pointed at. {context}"
    );
    assert_eq!(
        value("moved"),
        "001",
        "the line did not follow the model. {context}"
    );
    assert_eq!(
        value("said"),
        "New messages",
        "the line says something else: {}. {context}",
        value("said")
    );

    // Carrying row taller by the line; others untouched.
    let heights = |label: &str| -> Vec<f64> {
        value(label)
            .split(',')
            .filter_map(|part| part.parse().ok())
            .collect()
    };
    let quiet = heights("quiet-heights");
    let marked = heights("marked-heights");
    assert_eq!(
        quiet.len(),
        3,
        "three rows were not measured: {quiet:?}. {context}"
    );
    assert_eq!(quiet.len(), marked.len(), "the rows changed. {context}");
    assert!(
        marked[1] > quiet[1],
        "the row carrying the line did not grow by it: {quiet:?} -> \
         {marked:?}. {context}"
    );
    for row in [0, 2] {
        assert!(
            (marked[row] - quiet[row]).abs() < 0.5,
            "a row without the line changed height: {quiet:?} -> \
             {marked:?}. {context}"
        );
    }
}
