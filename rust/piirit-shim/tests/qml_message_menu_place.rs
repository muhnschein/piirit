//! A long press on a message leaves the view where the reader put it.
//!
//! The conversation's list moves itself: it follows the newest message
//! while the reader is down there, and it holds the row they came back to
//! while the rows around it settle. Both watch the content's height, and
//! both were watching when a context menu unfolded -- which grows a row by
//! a menu's worth in exactly the way a picture decoding does.
//!
//! So the view was thrown at the newest message the moment a menu finished
//! opening, and a menu opened on a held row was put back under the bottom
//! of the screen with the reader looking at the half of it that fit. The
//! chat list has neither the follow nor the hold, and its menus were fine;
//! this is what the conversation had that it did not.
//!
//! Pinned here: nothing in the view moves it while a menu is open -- not
//! the follow, not a message arriving, not a hold armed over it -- and a
//! view that was following goes back to the newest message once the menu
//! is closed again.

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

/// The list over rows that are filled in, so each is its message's own
/// height -- and, in the stub, so each grows by a menu's height when one
/// of its menus is opened.
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
    // SAFETY: single-threaded test binary; set before Qt starts.
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
    // Down at the newest message, following it: the state the follow
    // moves the view from.
    single_shot(Duration::from_secs(2), move || unsafe {
        (*steps_ptr).push(("following", call!("settleAtEnd")));
    });
    single_shot(Duration::from_secs(3), move || unsafe {
        (*steps_ptr).push(("pressed", call!("topIndex")));
    });
    // The row the reader pressed, read back out of the steps: which row
    // is at the top of the view depends on how the rows measured, so it
    // is not known until the run is under way.
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
    // The grown row measured, and a whole event loop after that: the
    // follow runs off a zero-length timer, so if it is going to throw
    // the view at the newest message it has by now.
    single_shot(Duration::from_secs(5), move || unsafe {
        call!("settle");
    });
    single_shot(Duration::from_secs(6), move || unsafe {
        (*steps_ptr).push(("after-opening", call!("place", pressed!())));
        // And a message arrives while the menu is up, which is the other
        // way the follow moves the view.
        (*steps_ptr).push(("arrival", call!("append", 1)));
    });
    single_shot(Duration::from_secs(7), move || unsafe {
        (*steps_ptr).push(("after-arrival", call!("place", pressed!())));
        (*steps_ptr).push(("still-open", call!("closeMenu", pressed!())));
    });
    // Closed again: a view that was following the newest message is back
    // on it, arrival and all.
    single_shot(Duration::from_secs(8), move || unsafe {
        (*steps_ptr).push(("closed-at-end", call!("ended")));
        (*steps_ptr).push(("closed-following", call!("following")));
        // Now the other mover: a row held where it is, as coming back
        // from a picture leaves it, with a menu opened on it.
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
        // With that menu still up, the button back to the newest
        // message. It is over the list, so the tap never reaches the row
        // and Silica leaves the menu open.
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
        // And the row goes while its menu is open on it. Nothing will
        // ever say the menu closed, because there is nothing left to
        // say it.
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

    // The reader long-pressed a row and is looking at it. Whatever the
    // menu's unfold does to the content's height, that row does not move.
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

    // Closed, and the reader is back where following means: at the newest
    // message, with what arrived while the menu was up on screen.
    assert_eq!(
        (
            value("closed-at-end").as_str(),
            value("closed-following").as_str()
        ),
        ("true", "true"),
        "closing the menu left a reader who was following the newest \
         message short of it. {context}"
    );

    // The other mover. A held row is let go of when a menu opens on it,
    // exactly as it is when a drag takes the view over -- otherwise the
    // hold re-pins the row's top through the whole unfold and the menu
    // grows off the bottom of the screen.
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

    // The button back to the newest message is over the list, so a tap
    // on it never reaches the row a menu is open on: left waiting on a
    // menu Silica is never going to close, the jump did nothing at all.
    assert_eq!(
        (value("jumped-end").as_str(), value("jumped-menu").as_str()),
        ("true", "false"),
        "the button back to the newest message did nothing while a \
         context menu was open. {context}"
    );

    // And the way out of the state, for a menu that never gets to say it
    // closed: the row it was on is gone. A view that still believes a
    // menu is up never follows the newest message again.
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
