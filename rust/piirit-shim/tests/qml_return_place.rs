//! Coming back to a conversation puts reader exactly where they were.
//!
//! Place is remembered as a row when page goes under another, and held while away:
//! a list torn down under a pushed page comes back at top of what it loaded. Holding
//! row to view *centre* is not enough: reader stopped anywhere (half a row above
//! centre, or a line below), so every return moved rows that far when swipe back ended.
//!
//! Pinned: view unmoved while away does not move on return; one that lost place is
//! put back to the pixel, not nearest row's centre.

// Qt harness: see qml_conversation_list.rs.
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

/// List over filled-in rows: each its message's own height, no two alike.
const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        id: holder
        width: 540
        height: 400

        ListModel { id: rows }
        /// Rows' heights cycle over this many line counts.
        property int spread: 4

        Loader {
            id: loader
            anchors.fill: parent
        }

        function setSpread(lines) { holder.spread = lines; return 'ok' }
        function append(count) {
            for (var i = 0; i < count; i++) {
                var text = 'message number ' + rows.count
                for (var line = 0; line < rows.count % holder.spread; line++) {
                    text += ', and another line of it to make this row '
                          + 'a different height from the ones around it'
                }
                rows.append({
                    message_id: rows.count + 1, loaded: true,
                    text: text,
                    is_outgoing: rows.count % 2 === 0, is_info: false,
                    show_padlock: true, state: 16,
                    timestamp: 1700000000 + rows.count, day_number: 19675,
                    sender_name: 'Ada', sender_color: '#00875a',
                    quote_text: '', quote_author: '',
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
        /// Stopped part way through the history, a little past a row's
        /// top: where a reader's thumb leaves a list, never a row's edge.
        function settleMidway() {
            var view = loader.item
            view.positionViewAtIndex(30, ListView.Beginning)
            view.forceLayout()
            view.contentY = view.contentY + 37
            view.movementStarted()
            view.movementEnded()
            return 'ok'
        }
        /// What the reader sees: the first row from the top of the view
        /// and where its top is, from the top of the view -- and the
        /// view's own position, which only means anything while the list
        /// has not re-estimated where its content begins.
        function position() {
            var view = loader.item
            view.forceLayout()
            for (var y = 0; y < view.height; y += 16) {
                var index = view.indexAt(view.width / 2, view.contentY + y)
                if (index >= 0) {
                    var item = view.itemAt(view.width / 2, view.contentY + y)
                    return Math.round(view.contentY) + ' ' + index + '@'
                           + Math.round(item.y - view.contentY)
                }
            }
            return Math.round(view.contentY) + ' none'
        }
        /// What the page does as another goes over it, and what the
        /// platform then does to the page.
        function leave() {
            loader.item.rememberPlace()
            holder.visible = false
            return 'ok'
        }
        /// Rows the view has built on screen, as it stands: no relayout
        /// asked, so a list left blank reads 0.
        function built() {
            var view = loader.item
            var rows = 0
            for (var y = 0; y < view.height; y += 16) {
                if (view.indexAt(view.width / 2, view.contentY + y) >= 0) { rows++ }
            }
            return '' + rows
        }
        /// The list losing its place under the other page.
        function lose() {
            var view = loader.item
            view.contentY = view.originY
            return 'ok'
        }
        /// The page shown and active again.
        function comeBack() {
            holder.visible = true
            loader.item.restorePlace()
            return 'ok'
        }
        /// Back to the newest message by the button, which lets go of
        /// any held row.
        function jump() { loader.item.jumpToNewest(); return 'ok' }
        /// Stopped a little short of the end, the way a scroll to it
        /// does: following, but not at the very end.
        function stopJustShort(gap) {
            var view = loader.item
            view.positionViewAtEnd()
            view.contentY = view.contentY - gap
            view.movementStarted()
            view.movementEnded()
            return '' + view.stickToBottom
        }
        function ended() { return '' + loader.item.atYEnd }
    }
";

#[test]
fn coming_back_puts_the_view_where_it_was_to_the_pixel() {
    coming_back(4, true);
}

/// Other row heights: lost place under a hidden list came back on its own for some heights
/// (luck of view's estimate), so one mix can pass a broken restore. Lost place only: how many
/// stops short it takes to stay short depends on row heights.
#[test]
fn coming_back_puts_the_view_back_with_other_row_heights() {
    coming_back(3, false);
}

/// Scenario, rows' heights cycling over `spread` line counts; `follow`: also following parts.
#[allow(clippy::too_many_lines)]
fn coming_back(spread: i32, follow: bool) {
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

    single_shot(Duration::from_secs(1), move || unsafe {
        call!("setSpread", spread);
        (*steps_ptr).push(("rows", call!("append", 60)));
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml"))
            ),
        ));
    });
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("settle", call!("settleMidway")));
    });
    // Away and back, nothing moved: plain swipe back.
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("before", call!("position")));
        (*steps_ptr).push(("leave", call!("leave")));
    });
    single_shot(Duration::from_secs(4), move || unsafe {
        (*steps_ptr).push(("back", call!("comeBack")));
    });
    single_shot(Duration::from_secs(5), move || unsafe {
        (*steps_ptr).push(("after", call!("position")));
        // Away again; list loses place while under the other page.
        (*steps_ptr).push(("leave-again", call!("leave")));
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("lose", call!("lose")));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("built-away", call!("built")));
        (*steps_ptr).push(("while-away", call!("position")));
        (*steps_ptr).push(("back-again", call!("comeBack")));
    });
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("after-losing", call!("position")));
        if !follow {
            (*engine_ptr).quit();
        }
        // Down to newest message: lets go of held row.
        call!("jump");
    });
    // Twice, a moment apart. First stop short brings one more row into view's buffer above,
    // changing content measure; following view is taken back to end for that. Second finds
    // those rows there and stays.
    single_shot(Duration::from_secs(9), move || unsafe {
        call!("stopJustShort", 12.0);
    });
    single_shot(Duration::from_secs(10), move || unsafe {
        (*steps_ptr).push(("following", call!("stopJustShort", 12.0)));
    });
    single_shot(Duration::from_secs(11), move || unsafe {
        (*steps_ptr).push(("short-ended", call!("ended")));
        (*steps_ptr).push(("short-before", call!("position")));
        (*steps_ptr).push(("leave-short", call!("leave")));
    });
    single_shot(Duration::from_secs(12), move || unsafe {
        (*steps_ptr).push(("back-short", call!("comeBack")));
    });
    single_shot(Duration::from_secs(13), move || unsafe {
        (*steps_ptr).push(("short-after", call!("position")));
        // Away once more; a message arrives meanwhile.
        (*steps_ptr).push(("leave-following", call!("leave")));
        (*steps_ptr).push(("arrival", call!("append", 1)));
    });
    single_shot(Duration::from_secs(14), move || unsafe {
        (*steps_ptr).push(("back-following", call!("comeBack")));
    });
    single_shot(Duration::from_secs(15), move || unsafe {
        (*steps_ptr).push(("at-end", call!("ended")));
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

    assert_eq!(value("load"), "ok", "the list did not load. {context}");
    let before = value("before");
    let (before_y, before_row) = before.split_once(' ').unwrap_or_default();
    assert!(
        before_y.parse::<i64>().ok() > Some(100) && before_row.contains('@'),
        "the view did not settle part way through the history, so this \
         run says nothing about coming back to it. {context}"
    );

    // Nothing moved while away, so nothing moves on return: not view, not row under eye.
    assert_eq!(
        value("after"),
        before,
        "coming back to a view that had not moved moved it: the rows jump \
         the moment the swipe back finishes. {context}"
    );

    // Put back in list's own scroll handler, rows were dropped and none built: blank list.
    assert_ne!(
        value("built-away"),
        "0",
        "the list lost its place under the other page and was left blank. {context}"
    );

    // Lost place while away, put back before return; coming back changed nothing more. Judged by
    // top row and its top, not view position: a list jumped to beginning and back
    // re-estimates content start, reads different position for same rows.
    let row_at_top = |label: &str| {
        value(label)
            .split_once(' ')
            .map(|(_, row)| row.to_string())
            .unwrap_or_default()
    };
    assert_eq!(
        row_at_top("while-away"),
        before_row,
        "the list lost its place under the other page and was not put \
         back while away. {context}"
    );
    assert_eq!(
        row_at_top("after-losing"),
        before_row,
        "coming back after the list lost its place did not put the reader \
         back where they were, to the pixel. {context}"
    );

    if !follow {
        return;
    }
    // Reader a line short of end, following: must not be sent to end on return (same jump,
    // always downwards).
    assert_eq!(
        (value("following").as_str(), value("short-ended").as_str()),
        ("true", "false"),
        "a reader a line short of the end does not count as following, or \
         was not still short of it a moment later, so this run says \
         nothing about coming back while following. {context}"
    );
    assert_eq!(
        value("short-after"),
        value("short-before"),
        "coming back while following, a line short of the end, moved the \
         view. {context}"
    );
    // Following still means following: what arrives while away is on screen on return.
    assert_eq!(
        value("at-end"),
        "true",
        "a message that arrived while the reader was away, following, is \
         not on screen when they are back. {context}"
    );
}
