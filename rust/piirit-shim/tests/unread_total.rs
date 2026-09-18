//! What the cover counts, and whom it draws.
//!
//! The total has to include muted chats: muting silences the
//! announcement, not the arithmetic, and the per-chat badge behaves the
//! same way. It also has to survive a count that would overflow rather
//! than wrapping to a small, plausible-looking number. The people are
//! everyone but the two chats that are nobody: the one with oneself and
//! the core's own.

use piirit_shim::{ChatList, ChatListItem};
use qmetaobject::QString;

fn chat(chat_id: u32, unread: u32, muted: bool) -> ChatListItem {
    ChatListItem {
        chat_id,
        unread_count: unread,
        is_muted: muted,
        ..ChatListItem::default()
    }
}

#[test]
fn the_total_counts_every_chat_including_muted_ones() {
    let list = ChatList::default();
    assert_eq!(list.unread_total(), 0, "an empty list is not silent");

    {
        let mut rows = list.rows.borrow_mut();
        rows.push(chat(1, 3, false));
        rows.push(chat(2, 0, false));
        // Muted, and still waiting to be read.
        rows.push(chat(3, 4, true));
    }

    assert_eq!(
        list.unread_total(),
        7,
        "the cover total dropped a muted chat's unread messages"
    );
}

#[test]
fn the_people_leave_out_the_chats_that_are_nobody() {
    let list = ChatList::default();
    {
        let mut rows = list.rows.borrow_mut();
        rows.push(ChatListItem {
            chat_id: 10,
            name: QString::from("Saved messages"),
            is_self_talk: true,
            ..ChatListItem::default()
        });
        rows.push(ChatListItem {
            chat_id: 11,
            name: QString::from("Ada"),
            unread_count: 2,
            color: QString::from("#c50000"),
            avatar_path: QString::from("/blobs/ada.png"),
            ..ChatListItem::default()
        });
        rows.push(ChatListItem {
            chat_id: 12,
            name: QString::from("Device Messages"),
            unread_count: 1,
            is_device_talk: true,
            ..ChatListItem::default()
        });
        rows.push(ChatListItem {
            chat_id: 13,
            name: QString::from("Hikers"),
            ..ChatListItem::default()
        });
    }

    let people: serde_json::Value =
        serde_json::from_str(&list.cover_people().to_string()).expect("the people are JSON");
    assert_eq!(
        people,
        serde_json::json!([
            {"chat_id": 11, "name": "Ada", "color": "#c50000",
             "avatar_path": "/blobs/ada.png", "unread_count": 2},
            {"chat_id": 13, "name": "Hikers", "color": "",
             "avatar_path": "", "unread_count": 0},
        ]),
        "the cover's people are not everyone but oneself and the device"
    );
    assert_eq!(
        list.unread_total(),
        3,
        "the total left out a chat the grid leaves out; the number is every message"
    );
}

#[test]
fn an_absurd_count_saturates_rather_than_wrapping() {
    let list = ChatList::default();
    {
        let mut rows = list.rows.borrow_mut();
        rows.push(chat(1, u32::MAX, false));
        rows.push(chat(2, 5, false));
    }
    assert_eq!(
        list.unread_total(),
        u32::MAX,
        "a total past u32 wrapped, so a huge backlog would show as a small \
         number on the cover"
    );
}
