use qmetaobject::listmodel::SimpleListModel;
use qmetaobject::QString;

/// Chat-list row from `get_chatlist_items_by_entries`; UI fields only (upstream ~20 more).
// Flags are QML roles: cannot fold.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default, Clone, PartialEq, Eq, qmetaobject::SimpleListItem)]
pub struct ChatListItem {
    /// Core chat id.
    pub chat_id: u32,
    /// Chat name.
    pub name: QString,
    /// Preview (`summaryText2`).
    pub preview: QString,
    /// Last sender (`summaryText1`), for group preview.
    pub preview_sender: QString,
    /// Unseen count, for badge.
    pub unread_count: u32,
    /// Unix seconds; core sends ms.
    pub last_updated: i64,
    /// `DC_STATE_*` of last message.
    pub summary_state: u32,
    /// `isEncrypted`. Unencrypted: mail icon.
    pub is_encrypted: bool,
    /// Pinned to top.
    pub is_pinned: bool,
    /// Muted; badge too.
    pub is_muted: bool,
    /// From unaccepted contact.
    pub is_contact_request: bool,
    /// "Saved messages". Not a person: cover skips it.
    pub is_self_talk: bool,
    /// Core's device chat. Not a person.
    pub is_device_talk: bool,
    /// Chat colour, `#rrggbb`.
    pub color: QString,
    /// Picture path; empty if none.
    pub avatar_path: QString,
}

/// Chat-list model.
pub type ChatListModel = SimpleListModel<ChatListItem>;

/// One message row, from `get_message_list_items` plus `get_message`.
// Flags are QML roles: cannot fold.
#[allow(clippy::struct_excessive_bools)]
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct MessageListItem {
    /// Core message id.
    pub message_id: u32,
    /// Content fetched? Row per message (first stays row 0); unviewed rows are placeholders.
    pub loaded: bool,
    /// Body text.
    pub text: QString,
    /// Sent by this account.
    pub is_outgoing: bool,
    /// Unix seconds.
    pub timestamp: i64,
    /// Days since epoch, viewer's timezone. Section key.
    pub day_number: i64,
    /// `showPadlock`: encrypted and signed. False: mail icon.
    pub show_padlock: bool,
    /// `DC_STATE_*`: out 20 pending, 24 failed, 26 delivered, 28 read; in 10 fresh, 13 noticed,
    /// 16 seen.
    pub state: u32,
    /// `overrideSenderName`, else contact name.
    pub sender_name: QString,
    /// Contact colour, `#rrggbb`.
    pub sender_color: QString,
    /// Core notice, not typed by anyone.
    pub is_info: bool,
    /// `isForwarded`; set on sender's copy too.
    pub is_forwarded: bool,
    /// Quoted text; empty if none.
    pub quote_text: QString,
    /// Quoted author.
    pub quote_author: QString,
    /// Quoted message (`WithMessage` quote); 0 if core has none (deleted, never here).
    pub quote_message_id: u32,
    /// Absolute attachment path in blob dir.
    pub file_path: QString,
    /// Attachment name.
    pub file_name: QString,
    /// `viewType`, core's pick: Text, Image, Gif, Sticker, Audio, Voice, Video, File, Call,
    /// Webxdc, Vcard.
    pub view_type: QString,
    /// Pixel size; 0 if not image. GIF read from header.
    pub image_width: i32,
    /// Pixel size; 0 if not image.
    pub image_height: i32,
    /// Incoming, unread when built; GIF autoplay.
    pub is_new: bool,
    /// MIME type; empty if none.
    pub file_mime: QString,
    /// Bytes; 0 if unknown.
    pub file_bytes: f64,
    /// Length ms, 0 unknown; sender-written, core never measures.
    pub duration_ms: i32,
    /// Shared contact name; empty if not vCard.
    pub vcard_name: QString,
    /// Shared contact address.
    pub vcard_addr: QString,
    /// Shared contact colour, `#rrggbb`.
    pub vcard_color: QString,
    /// `downloadState`: Done, Available, `InProgress`, Failure, Undecipherable. Not Done:
    /// header only, held by download limit.
    pub download_state: QString,
    /// `hasHtml`: `get_message_html` has more than `text`. Also HTML alternative or only,
    /// stripped quote, forward: not just "cut" (`wasCut` in `MessageDelegate.qml`).
    pub has_html: bool,
    /// Markdown as `Text.StyledText`, rendered once.
    pub styled_text: QString,
    /// `isEdited`: changed after send; footer "Edited".
    pub is_edited: bool,
    /// JSON array of `{"emoji", "count", "self"}`, most frequent first; empty if none. One
    /// string: only chips read it.
    pub reactions: QString,
    /// Own reaction; empty if none. Tapping again removes it.
    pub my_reaction: QString,
    /// Webxdc app name from manifest (`webxdc.rs`); else empty.
    pub webxdc_name: QString,
    /// Document a webxdc app edits; empty if none.
    pub webxdc_document: QString,
    /// Webxdc summary ("3 votes"); changes as chat plays.
    pub webxdc_summary: QString,
    /// App icon path, cached; empty if none.
    pub webxdc_icon: QString,
    /// `call_info` state: `Alerting` (ringing), `Active`, `Completed`, `Missed`, `Declined`,
    /// `Canceled`; empty for non-calls. Row drawn from it, not English `text`.
    pub call_state: QString,
    /// Video call.
    pub call_has_video: bool,
    /// Call length, seconds; 0 if never connected.
    pub call_duration: i32,
    /// `hasLocation`: carries a point (sent once, or streamed beside it).
    pub has_location: bool,
    /// Core's notice that sender shares location live; tap shows their newest.
    pub starts_location: bool,
    /// Sender's contact id (`fromId`); 1 = reader.
    pub sender_id: u32,
}

/// Conversation model.
pub type MessageListModel = SimpleListModel<MessageListItem>;

/// One account from `get_all_accounts`.
#[derive(Default, Clone, PartialEq, Eq, qmetaobject::SimpleListItem)]
pub struct AccountItem {
    /// Core account id.
    pub account_id: u32,
    /// Profile name; empty if unset.
    pub display_name: QString,
    /// `configured_addr`; empty if unconfigured or unknown.
    pub addr: QString,
    /// Has a usable transport.
    pub is_configured: bool,
    /// Picture path; empty if none.
    pub avatar_path: QString,
    /// Account colour, `#rrggbb`.
    pub color: QString,
    /// Unread: `get_fresh_msgs`, muted chats excluded.
    pub unread_count: u32,
}

/// Account model.
pub type AccountListModel = SimpleListModel<AccountItem>;

/// Relay from `list_transports`, with core's connectivity report.
#[derive(Default, Clone, PartialEq, qmetaobject::SimpleListItem)]
pub struct TransportItem {
    /// Per-address id, stable across reloads, for `PendingRemoval.qml` (keys on numbers).
    /// Positive, fits QML `int`.
    pub id: u32,
    /// Address on relay; handed to core to remove it.
    pub addr: QString,
    /// Relay domain (after `@`).
    pub domain: QString,
    /// `configured_addr`: one per profile, first row. Not necessarily send relay (core picks
    /// per connect, unsaid).
    pub is_primary: bool,
    /// Dot colour: `green`, `yellow`, `red`, `grey`; empty before reported.
    pub dot: QString,
    /// Core's localized connection words; empty before reported.
    pub status: QString,
    /// Profile not connected (IO stopped: no network, or offered to second device); report then
    /// omits relays. Dot red, page's own words.
    pub offline: bool,
    /// Quota reported; fields below meaningless if false.
    pub has_quota: bool,
    /// Mailbox fullness, percent.
    pub quota_percent: u32,
    /// Core's mailbox sentence, e.g. `1.3 MiB of 2 GiB used`.
    pub quota_text: QString,
    /// Bytes behind `quota_text`; 0 if unknown. f64: QML lacks 64-bit int.
    pub quota_used_bytes: f64,
    /// See `quota_used_bytes`.
    pub quota_limit_bytes: f64,
}

/// Transport model.
pub type TransportListModel = SimpleListModel<TransportItem>;

/// One contact from `get_contacts`.
#[derive(Default, Clone, PartialEq, Eq, qmetaobject::SimpleListItem)]
pub struct ContactItem {
    /// Core contact id.
    pub contact_id: u32,
    /// Display name: given, else contact's, else address.
    pub display_name: QString,
    /// Name given on this device; empty if none.
    pub name: QString,
    /// Name the contact chose; fallback for blank `name`.
    pub auth_name: QString,
    /// Email address.
    pub address: QString,
    /// Encryption-capable (address contacts aren't).
    pub is_key_contact: bool,
    /// This account's own contact.
    pub is_self: bool,
    /// Status line; empty if none.
    pub status: QString,
    /// Contact colour, `#rrggbb`.
    pub color: QString,
    /// Picture path; empty if none.
    pub avatar_path: QString,
    /// Not seen long or ever (core's "Old"; key contacts).
    pub seen_long_ago: bool,
}

/// Contact model.
pub type ContactListModel = SimpleListModel<ContactItem>;

/// Search result, any kind: one flat model, as QML `section` needs for counted headings.
#[derive(Default, Clone, qmetaobject::SimpleListItem)]
pub struct SearchItem {
    /// `chat`, `contact` or `message`; QML sections on it.
    pub kind: QString,
    /// Chat to open; message rows carry their chat.
    pub chat_id: u32,
    /// Contact rows only; chat with them may not exist yet.
    pub contact_id: u32,
    /// Message rows only.
    pub message_id: u32,
    /// Chat or contact name, or chat a message is in.
    pub title: QString,
    /// Last message, address, or matching text.
    pub subtitle: QString,
    /// Unix seconds; 0 if none.
    pub timestamp: i64,
    /// Colour, `#rrggbb`.
    pub color: QString,
    /// Picture path; empty if none.
    pub avatar_path: QString,
}

/// Search model.
pub type SearchListModel = SimpleListModel<SearchItem>;
