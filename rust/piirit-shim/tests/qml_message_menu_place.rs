//! Long press keeps view where reader put it. Conversation follows newest and holds returned-to
//! row; both watch content height, which an unfolding menu grows like a decoding picture. So view
//! was thrown to newest once a menu opened, and a menu on a held row was pushed under the screen's
//! bottom. Chat list has neither follow nor hold; its menus were fine. Pinned: nothing moves view
//! while menu open (not follow, not arrival, not a hold armed over it); a following view returns
//! to newest after close.

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

/// Rows filled in: each its message's height; in stub, grows by a menu's height when one opens.
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
                var text = 'message number ' + rows.count
                for (var line = 0; line < rows.count % 4; line++) {
                    text += ', and another line of it to make this row '
                          + 'a different height from the ones around it'
                }
                rows.append({
                    message_id: rows.count + 1, loaded: true,
                    text: text, styled_text: '', is_edited: false,
                    is_outgoing: rows.count % 2 === 0, is_info: false,
                    show_padlock: true, state: 16,
                    timestamp: 1700000000 + rows.count, day_number: 19675,
                    sender_name: 'Ada', sender_color: '#00875a',
                    is_forwarded: false, quote_text: '', quote_author: '',
                    file_path: '', file_name: '', file_mime: '',
                    file_bytes: 0, view_type: 'Text',
                    image_width: 0, image_height: 0,
                    is_new: false, has_html: false, download_state: 'Done',
                    vcard_name: '', vcard_addr: '', vcard_color: '',
                    webxdc_name: '', webxdc_document: '',
                    webxdc_summary: '', webxdc_icon: '', reactions: ''
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
        /// At the newest message and following it, the way a reader who
        /// has just scrolled down to the end is.
        function settleAtEnd() {
            var view = loader.item
            view.positionViewAtEnd()
            view.movementStarted()
            view.movementEnded()
            return '' + view.following
        }
        /// The first row from the top of the view.
        function topIndex() {
            var view = loader.item
            view.forceLayout()
            for (var y = 0; y < view.height; y += 8) {
                var index = view.indexAt(view.width / 2, view.contentY + y)
                if (index >= 0) { return '' + index }
            }
            return '-1'
        }
        /// Where row `index` sits, its top from the top of the view.
        /// This is what the reader sees move.
        function place(index) {
            var view = loader.item
            view.forceLayout()
            var item = view.itemOf(index)
            if (!item) { return 'off-screen' }
            return '' + Math.round(item.y - view.contentY)
        }
        /// The long press, on the row the reader pressed.
        function openMenu(index) {
            var view = loader.item
            view.forceLayout()
            var item = view.itemOf(index)
            if (!item) { return 'off-screen' }
            item.openMenu()
            return '' + item.menuOpen
        }
        function closeMenu(index) {
            var view = loader.item
            view.forceLayout()
            var item = view.itemOf(index)
            if (!item) { return 'off-screen' }
            item.closeMenu()
            return '' + item.menuOpen
        }
        /// A row held where it is, which is what coming back to the page
        /// from a picture leaves behind.
        function hold(index, offset) {
            loader.item.holdPlace(index, offset)
            return '' + loader.item.pendingRow
        }
        /// One layout pass. A row grows when its menu opens, and the
        /// view does not notice until it has measured it -- so without
        /// this the reader is asked what they can see a frame before the
        /// view has decided to move them.
        function settle() {
            loader.item.forceLayout()
            return 'ok'
        }
        /// The row a menu was opened on, taken out from under it: the
        /// message deleted, or a reload that drops it.
        function removeRow(index) {
            rows.remove(index, 1)
            return '' + rows.count
        }
        function menuUp() { return '' + loader.item.menuOpen }
        /// The button over the list, which never reaches the row a menu
        /// is open on.
        function jump() { loader.item.jumpToNewest(); return 'ok' }
        function holding() { return '' + loader.item.pendingRow }
        function following() { return '' + loader.item.following }
        function ended() { return '' + loader.item.atYEnd }
    }
";

#[test]
#[allow(clippy::too_many_lines)]
fn a_context_menu_does_not_move_the_view_under_the_reader() {
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
        (*steps_ptr).push(("rows", call!("append", 60)));
        (*steps_ptr).push((
            "load",
            call!(
                "load",
                QString::from(common::component_url("ConversationList.qml"))
            ),
        ));
    });
    // Down at newest, following: start state.
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("following", call!("settleAtEnd")));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("pressed", call!("topIndex")));
    });
    // Pressed row read from steps: top row known only mid-run.
    macro_rules! pressed {
        () => {
            (*steps_ptr)
                .iter()
                .find(|(name, _)| *name == "pressed")
                .and_then(|(_, value)| value.parse::<f64>().ok())
                .unwrap_or(-1.0)
        };
    }
    single_shot(Duration::from_secs(4), move || unsafe {
        let index = pressed!();
        (*steps_ptr).push(("before", call!("place", index)));
        (*steps_ptr).push(("opened", call!("openMenu", index)));
    });
    // Grown row measured, then a whole event loop: follow runs off a zero-length timer, so if it
    // would throw view to newest, it has by now.
    single_shot(Duration::from_secs(5), move || unsafe {
        call!("settle");
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("after-opening", call!("place", pressed!())));
        // Message arrives while menu up: other follow mover.
        (*steps_ptr).push(("arrival", call!("append", 1)));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("after-arrival", call!("place", pressed!())));
        (*steps_ptr).push(("still-open", call!("closeMenu", pressed!())));
    });
    // Closed: following view back on newest, arrivals included.
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("closed-at-end", call!("ended")));
        (*steps_ptr).push(("closed-following", call!("following")));
        // Other mover: row held in place, as coming back from a picture leaves it, menu opened on
        // it.
        (*steps_ptr).push(("held", call!("hold", 12.0, 40.0)));
    });
    single_shot(Duration::from_secs(9), move || unsafe {
        (*steps_ptr).push(("held-place", call!("place", 12.0)));
        (*steps_ptr).push(("held-opened", call!("openMenu", 12.0)));
    });
    single_shot(Duration::from_secs(10), move || unsafe {
        call!("settle");
    });
    single_shot(Duration::from_secs(11), move || unsafe {
        (*steps_ptr).push(("hold-let-go", call!("holding")));
        (*steps_ptr).push(("held-after", call!("place", 12.0)));
        // Menu still up, tap newest button: it's over list, so tap never reaches row and Silica
        // leaves menu open.
        (*steps_ptr).push(("jumped", call!("jump")));
    });
    single_shot(Duration::from_secs(12), move || unsafe {
        call!("settle");
    });
    single_shot(Duration::from_secs(13), move || unsafe {
        (*steps_ptr).push(("jumped-end", call!("ended")));
        (*steps_ptr).push(("jumped-menu", call!("menuUp")));
        (*steps_ptr).push(("pressed-again", call!("topIndex")));
    });
    single_shot(Duration::from_secs(14), move || unsafe {
        let index = (*steps_ptr)
            .iter()
            .find(|(name, _)| *name == "pressed-again")
            .and_then(|(_, value)| value.parse::<f64>().ok())
            .unwrap_or(-1.0);
        (*steps_ptr).push(("opened-again", call!("openMenu", index)));
        // Row goes while menu open: nothing says menu closed.
        (*steps_ptr).push(("row-gone", call!("removeRow", index)));
    });
    single_shot(Duration::from_secs(15), move || unsafe {
        call!("settle");
    });
    single_shot(Duration::from_secs(16), move || unsafe {
        (*steps_ptr).push(("gone-menu", call!("menuUp")));
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
    assert_eq!(
        value("following"),
        "true",
        "the view is not following the newest message, so this run says \
         nothing about the follow moving it. {context}"
    );
    let pressed: i64 = value("pressed").parse().unwrap_or(-1);
    assert!(
        pressed > 0,
        "no row at the top of the view to press on. {context}"
    );
    assert_eq!(
        value("opened"),
        "true",
        "the row's menu did not open, so this run says nothing about what \
         opening one does to the view. {context}"
    );

    // Long-pressed row stays put while menu unfolds.
    assert_eq!(
        value("after-opening"),
        value("before"),
        "opening a context menu moved the row it was opened on: the view \
         chased the end as the menu unfolded. {context}"
    );
    assert_eq!(
        value("after-arrival"),
        value("before"),
        "a message arriving while a context menu was open moved the row \
         the menu was opened on. {context}"
    );
    assert_eq!(
        value("still-open"),
        "false",
        "the row's menu did not close. {context}"
    );

    // Closed: back at newest, with arrivals while menu up.
    assert_eq!(
        (
            value("closed-at-end").as_str(),
            value("closed-following").as_str()
        ),
        ("true", "true"),
        "closing the menu left a reader who was following the newest \
         message short of it. {context}"
    );

    // Held row released when menu opens, like a drag; else hold re-pins top, menu grows off
    // bottom.
    assert_ne!(
        value("held"),
        "-1",
        "the row was not held, so this run says nothing about a hold over \
         an open menu. {context}"
    );
    assert_eq!(
        value("held-opened"),
        "true",
        "the held row's menu did not open. {context}"
    );
    assert_eq!(
        value("hold-let-go"),
        "-1",
        "opening a context menu did not let go of the held row: the hold \
         puts the view back through the menu's unfold and takes the menu \
         off the bottom of the screen with it. {context}"
    );
    assert_eq!(
        value("held-after"),
        value("held-place"),
        "opening a context menu on a held row moved it. {context}"
    );

    // Newest button over list: tap never reaches menu'd row. Jump used to wait on a menu Silica
    // never closes, doing nothing at all.
    assert_eq!(
        (value("jumped-end").as_str(), value("jumped-menu").as_str()),
        ("true", "false"),
        "the button back to the newest message did nothing while a \
         context menu was open. {context}"
    );

    // Way out for a menu that never says it closed: its row is gone. View still believing a menu
    // is up would never follow newest again.
    assert_ne!(
        value("row-gone"),
        "",
        "the row was not taken away, so this run says nothing about a \
         menu whose row is gone. {context}"
    );
    assert_eq!(
        value("opened-again"),
        "true",
        "the row's menu did not open, so this run says nothing about a \
         menu whose row is gone. {context}"
    );
    assert_eq!(
        value("gone-menu"),
        "false",
        "a row destroyed with its menu open left the view believing a \
         menu was still up, which stops it ever moving itself again. \
         {context}"
    );
}
