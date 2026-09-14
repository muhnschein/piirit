//! The chat list, as a QML-instantiable type.
//!
//! A chat list reorders constantly: any message moves its chat to the top.
//! Rebuilding the model for that loses the scroll position and redraws every
//! row, so this one reconciles instead -- it moves the row that moved and
//! refetches only the chats whose contents changed.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;
use serde_json::json;

use crate::chat::chat_is_group;
use crate::core::connection;
use crate::json;
use crate::models::{ChatListItem, ChatListModel};

/// One account's chats, most recent first.
///
/// ```qml
/// ChatList { id: chats; account_id: page.accountId }
/// SilicaListView { model: chats.rows }
/// ```
#[derive(QObject, Default)]
pub struct ChatList {
    base: qt_base_class!(trait QObject),

    /// Whose chats these are. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Emitted when the account changes.
    pub account_changed: qt_signal!(),

    /// Only show chats matching this. Empty shows everything.
    ///
    /// The core does the matching, so a search finds chats this model has
    /// never loaded rather than filtering the rows already on screen.
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Emitted when the query changes.
    pub query_changed: qt_signal!(),

    /// Show the archived chats instead of the ordinary ones.
    pub archived: qt_property!(bool; WRITE set_archived NOTIFY archived_changed),
    /// Emitted when the archived flag changes.
    pub archived_changed: qt_signal!(),

    /// List only chats a message can be forwarded into.
    ///
    /// The core leaves out the ones that would fail or make no sense --
    /// the device chat among them -- so a picker built on this cannot
    /// offer a destination the forward would then be refused by.
    pub for_forwarding: qt_property!(bool; WRITE set_for_forwarding NOTIFY for_forwarding_changed),
    /// Emitted when the forwarding flag changes.
    pub for_forwarding_changed: qt_signal!(),

    /// Whether a message in a muted group that answers one of the
    /// account's own is announced all the same. The reader's setting:
    /// what the reference clients call mention notifications, where a
    /// mention is a reply to something the reader wrote. Only a group --
    /// a one-to-one chat the reader muted was muted with that person in
    /// mind, and everything in it is addressed to the reader anyway.
    pub notify_mentions: qt_property!(bool; NOTIFY notify_mentions_changed),
    /// Emitted when the setting changes.
    pub notify_mentions_changed: qt_signal!(),

    /// The rows, for a `SilicaListView`'s `model`.
    pub rows: qt_property!(RefCell<ChatListModel>; CONST),

    /// How many rows there are.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    // Stored rather than counted out of the model on demand. A section
    // heading binds to these, so QML reads them from inside the model
    // reset that sets the rows -- and a reader that borrowed the row list
    // there would find it already mutably borrowed and take the process
    // down with it.
    /// Chats kept at the top, for deciding whether to head the two
    /// groups at all: one heading over the whole list says nothing.
    pub pinned_count: qt_property!(u32; NOTIFY rows_changed),
    /// The rest, for the same decision from the other side.
    pub unpinned_count: qt_property!(u32; NOTIFY rows_changed),
    /// Unread messages across every chat, for the cover.
    ///
    /// Muted chats are counted. Muting silences the announcement, not the
    /// arithmetic -- the badge on a muted chat behaves the same way.
    pub unread_total: qt_property!(u32; READ unread_total NOTIFY rows_changed),
    /// The people behind the chats, for the cover to draw: every row but
    /// the chat with oneself and the device chat, as a JSON array of
    /// `{"chat_id", "name", "color", "avatar_path", "unread_count"}` in
    /// the list's order. One string rather than the model: the cover
    /// lays its grid out in one pass and repeats the few to fill it,
    /// which a view over the rows cannot.
    pub cover_people: qt_property!(QString; READ cover_people NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// A message just arrived in this chat, and the row for it now holds
    /// the sender and text a notification wants.
    ///
    /// Only for `IncomingMsg`, and only once the refetch it triggered has
    /// landed -- announcing on the event itself would carry a preview from
    /// before the message. A muted chat is never announced: it still
    /// counts towards the badge, quietly, which is the whole point of
    /// muting. `sender` is who wrote it as the row's summary names them:
    /// a name in a group, and empty in a one-to-one chat, where the chat
    /// is already named after them.
    pub message_arrived: qt_signal!(chat_id: u32, chat_name: QString, sender: QString, preview: QString),

    /// Loading failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Reload the whole list.
    pub reload: qt_method!(fn(&mut self)),

    /// Feed a `core_event` in. Events for other accounts are ignored.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Mark everything in a chat read, without opening it.
    pub mark_read: qt_method!(fn(&mut self, chat_id: u32)),
    /// Put one unread message back on a chat, so it stands out in the
    /// list again until it is opened.
    pub mark_unread: qt_method!(fn(&mut self, chat_id: u32)),
    /// Keep a chat at the top of the list, or let it sort by time again.
    pub set_pinned: qt_method!(fn(&mut self, chat_id: u32, pinned: bool)),
    /// Silence a chat, or let it speak again.
    pub set_muted: qt_method!(fn(&mut self, chat_id: u32, muted: bool)),
    /// Move a chat out of the list.
    pub archive: qt_method!(fn(&mut self, chat_id: u32)),
    /// Accept a contact request; QML calls this.
    pub accept_chat: qt_method!(fn(&mut self, chat_id: u32)),
    /// Block a contact request's sender; QML calls this.
    pub block_chat: qt_method!(fn(&mut self, chat_id: u32)),
    /// Move a chat back into the ordinary list.
    pub unarchive: qt_method!(fn(&mut self, chat_id: u32)),
    /// Delete a chat and its messages on this device.
    pub delete_chat: qt_method!(fn(&mut self, chat_id: u32)),

    /// Chats with an arrival not yet announced: the message landed, and
    /// the row that will carry its preview has not been read back yet.
    ///
    /// Kept apart from the refresh that was started for it, because that
    /// refresh is rarely the one that lands: the core follows every
    /// `IncomingMsg` with a `ChatlistItemChanged` and a `ChatlistChanged`
    /// within the same millisecond, each starting a newer refresh and
    /// making the older answer stale (see `generation`). Whichever answer
    /// is current announces what is waiting.
    pending_announcements: HashSet<u32>,

    /// Muted chats whose latest arrival was for the reader, waiting to be
    /// announced past the mute: see `check_mention`.
    mentioned: HashSet<u32>,

    /// Counts refreshes, so a slow answer to an older question cannot land
    /// on top of a newer one.
    ///
    /// Pushing the archived page sets `account_id` and `archived` in
    /// whatever order QML chooses, and each starts its own fetch -- one
    /// for the ordinary list, one for the archived. Without this the
    /// slower answer wins, and the archived page shows ordinary chats
    /// until it is opened a second time. Typing in the search field starts
    /// one per keystroke for the same reason.
    generation: u64,
}

impl ChatList {
    /// How many rows there are.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Unread messages across every chat.
    pub fn unread_total(&self) -> u32 {
        self.rows
            .borrow()
            .iter()
            .fold(0u32, |total, row| total.saturating_add(row.unread_count))
    }

    /// The people behind the chats, as JSON; see the property.
    pub fn cover_people(&self) -> QString {
        let people: Vec<serde_json::Value> = self
            .rows
            .borrow()
            .iter()
            .filter(|row| !row.is_self_talk && !row.is_device_talk)
            .map(|row| {
                json!({
                    "chat_id": row.chat_id,
                    "name": row.name.to_string(),
                    "color": row.color.to_string(),
                    "avatar_path": row.avatar_path.to_string(),
                    "unread_count": row.unread_count,
                })
            })
            .collect();
        serde_json::Value::Array(people).to_string().into()
    }

    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.reload();
        }
    }

    /// Reload every row.
    pub fn reload(&mut self) {
        self.refresh(Refresh::All);
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        let kind = kind.to_string();
        if !matches!(
            kind.as_str(),
            "IncomingMsg"
                | "MsgsChanged"
                | "MsgsNoticed"
                | "MsgDelivered"
                | "MsgRead"
                | "MsgFailed"
                // Pinning, muting, archiving and deleting land here.
                | "ChatModified"
                | "ChatDeleted"
                | "ChatlistChanged"
                | "ChatlistItemChanged"
                // The core dropped events it could not queue. Whatever it
                // was, this model did not see it, so nothing it holds can
                // be trusted.
                | "EventChannelOverflow"
        ) {
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        // A chat id names the one row that changed. Without one -- absent,
        // null, or 0 -- the core is saying it could not work out which
        // rows are affected and every visible one has to be re-read.
        // Upstream's own words for `ChatlistItemChanged`: "If chat_id is
        // set to None, then all currently visible chats need to be
        // rerendered".
        // An overflow carries no chat id and could have hidden anything.
        if kind == "EventChannelOverflow" {
            self.refresh(Refresh::All);
            return;
        }
        let chat_id = json::u32_opt(&payload, "chatId").filter(|id| *id != 0);
        let scope = chat_id.map_or(Refresh::All, Refresh::One);
        // Worth telling anyone listening about, but only a genuinely new
        // message: MsgsChanged and friends fire for messages we sent, for
        // read receipts, and for a chat being pinned.
        let announce = if kind == "IncomingMsg" { chat_id } else { None };
        // A muted chat is refreshed like any other and announced only if
        // the message was for the reader, which takes asking the core.
        if let Some(chat_id) = announce {
            if self.notify_mentions && self.is_muted(chat_id) {
                if let Some(message_id) = json::u32_opt(&payload, "msgId").filter(|id| *id != 0) {
                    self.check_mention(chat_id, message_id);
                }
            }
        }
        self.refresh_announcing(scope, announce);
    }

    /// Whether the row for this chat is muted. A chat not in the list is
    /// a new one, which nobody has had the chance to mute.
    fn is_muted(&self, chat_id: u32) -> bool {
        self.rows
            .borrow()
            .iter()
            .any(|row| row.chat_id == chat_id && row.is_muted)
    }

    /// Find out whether a message that landed in a muted chat answers one
    /// of the reader's own, and announce it if so.
    ///
    /// The answer arrives after the refresh the event started has
    /// usually landed and left the chat unannounced for being muted, so
    /// a mention starts a refresh of its own, with the chat marked to be
    /// let through the mute once: the announcement then carries the
    /// row's preview like every other, rather than one read here.
    fn check_mention(&mut self, chat_id: u32, message_id: u32) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |mentioned: bool| {
            let Some(this) = ptr.as_pinned() else { return };
            if !mentioned {
                return;
            }
            this.borrow_mut().mentioned.insert(chat_id);
            this.borrow_mut()
                .refresh_announcing(Refresh::One(chat_id), Some(chat_id));
        });
        runtime.spawn(async move {
            done(is_mention(&rpc, account_id, chat_id, message_id).await);
        });
    }

    /// Set the query and reload if it changed.
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.refresh(Refresh::All);
        }
    }

    /// Show the archived chats, or the ordinary ones.
    pub fn set_archived(&mut self, archived: bool) {
        if self.archived != archived {
            self.archived = archived;
            self.archived_changed();
            self.refresh(Refresh::All);
        }
    }

    /// List only chats a message can be forwarded into.
    pub fn set_for_forwarding(&mut self, for_forwarding: bool) {
        if self.for_forwarding != for_forwarding {
            self.for_forwarding = for_forwarding;
            self.for_forwarding_changed();
            self.refresh(Refresh::All);
        }
    }

    /// Accept a contact request, so its chat becomes an ordinary one.
    pub fn accept_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "accept_chat", serde_json::Value::Null);
    }

    /// Block the sender of a contact request.
    pub fn block_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "block_chat", serde_json::Value::Null);
    }

    /// Mark everything in a chat read.
    pub fn mark_read(&mut self, chat_id: u32) {
        self.act(chat_id, "marknoticed_chat", serde_json::Value::Null);
    }

    /// Mark a chat unread: the core's `markfresh_chat`, which puts the
    /// chat's last incoming message back to fresh, so the chat counts one
    /// unread until it is opened. A chat with no incoming message has
    /// nothing to put back, and the core leaves it as it is.
    pub fn mark_unread(&mut self, chat_id: u32) {
        self.act(chat_id, "markfresh_chat", serde_json::Value::Null);
    }

    /// Pin a chat, or unpin it.
    pub fn set_pinned(&mut self, chat_id: u32, pinned: bool) {
        let visibility = if pinned { "Pinned" } else { "Normal" };
        self.act(chat_id, "set_chat_visibility", json!([visibility]));
    }

    /// Mute a chat, or unmute it.
    pub fn set_muted(&mut self, chat_id: u32, muted: bool) {
        let kind = if muted { "Forever" } else { "NotMuted" };
        self.act(chat_id, "set_chat_mute_duration", json!([{"kind": kind}]));
    }

    /// Move a chat out of the list.
    pub fn archive(&mut self, chat_id: u32) {
        self.act(chat_id, "set_chat_visibility", json!(["Archived"]));
    }

    /// Move a chat back into the ordinary list.
    pub fn unarchive(&mut self, chat_id: u32) {
        self.act(chat_id, "set_chat_visibility", json!(["Normal"]));
    }

    /// Delete a chat and its messages on this device.
    pub fn delete_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "delete_chat", serde_json::Value::Null);
    }

    /// Call `method` with `(account, chat)` plus `extra`, then refresh the
    /// row it acted on: the core does not announce every one of these.
    fn act(&mut self, chat_id: u32, method: &'static str, extra: serde_json::Value) {
        let account_id = self.account_id;
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let mut params = vec![json!(account_id), json!(chat_id)];
        if let serde_json::Value::Array(rest) = extra {
            params.extend(rest);
        }

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().refresh(Refresh::One(chat_id)),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(method, params)
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// What is waiting to be announced now that the rows are current:
    /// each chat's name and preview, for the chats a mute does not
    /// silence. Everything waiting, not only what the refresh that just
    /// landed was started for: see `pending_announcements`. A chat's
    /// leave to pass a mute is spent here whether or not its row is
    /// still muted.
    fn settled_announcements(&mut self) -> Vec<(u32, QString, QString, QString)> {
        let waiting: Vec<u32> = self.pending_announcements.drain().collect();
        let rows = self.rows.borrow();
        let mut announcements = Vec::new();
        for chat_id in waiting {
            let mentioned = self.mentioned.remove(&chat_id);
            let row = rows
                .iter()
                .find(|row| row.chat_id == chat_id && (!row.is_muted || mentioned));
            if let Some(row) = row {
                announcements.push((
                    chat_id,
                    row.name.clone(),
                    row.preview_sender.clone(),
                    row.preview.clone(),
                ));
            }
        }
        announcements
    }

    /// Bring the model in line with the core.
    ///
    /// [`Refresh::One`] refetches the chat it names along with any chat not
    /// in the model yet, and reuses every other row -- so a message
    /// arriving in one chat costs one entry listing and one item fetch, not
    /// a rebuild. [`Refresh::All`] refetches the lot, which is what the
    /// core asks for when it reports a change it cannot attribute.
    fn refresh(&mut self, scope: Refresh) {
        self.refresh_announcing(scope, None);
    }

    /// [`Self::refresh`], and afterwards say that a message landed in
    /// `announce` -- once the row for it holds the new preview.
    fn refresh_announcing(&mut self, scope: Refresh, announce: Option<u32>) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let query = self.query.to_string();
        let archived = self.archived;
        let for_forwarding = self.for_forwarding;
        let cached: Vec<ChatListItem> = self.rows.borrow().iter().cloned().collect();
        // A set, not a list: this is asked once per entry, and a long chat
        // list would otherwise make the scan quadratic.
        let known: HashSet<u32> = cached.iter().map(|row| row.chat_id).collect();
        if let Some(chat_id) = announce {
            self.pending_announcements.insert(chat_id);
        }

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ChatListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answered after something newer was asked: this is the answer
            // to a question that no longer describes what is on screen.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(target) => {
                    {
                        // Counted before the rows are set, from the list
                        // about to become them: setting the rows is what
                        // makes QML read the counts back.
                        let pinned = target.iter().filter(|row| row.is_pinned).count();
                        let mut this_mut = this.borrow_mut();
                        this_mut.pinned_count = u32::try_from(pinned).unwrap_or(u32::MAX);
                        this_mut.unpinned_count =
                            u32::try_from(target.len() - pinned).unwrap_or(u32::MAX);
                    }
                    {
                        let this_ref = this.borrow();
                        let mut rows = this_ref.rows.borrow_mut();
                        reconcile(&mut rows, target);
                    }
                    this.borrow().rows_changed();
                    let announcements = this.borrow_mut().settled_announcements();
                    for (chat_id, name, sender, preview) in announcements {
                        this.borrow()
                            .message_arrived(chat_id, name, sender, preview);
                    }
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let entries =
                    chat_entries(&rpc, account_id, &query, archived, for_forwarding).await?;
                let wanted: Vec<u32> = entries
                    .iter()
                    .copied()
                    .filter(|id| match scope {
                        Refresh::All => true,
                        Refresh::One(chat) => *id == chat || !known.contains(id),
                    })
                    .collect();
                let fresh = if wanted.is_empty() {
                    HashMap::new()
                } else {
                    chat_items(&rpc, account_id, &wanted).await?
                };
                // Target order from the core, contents from the fetch where
                // we have them and from the model where we do not.
                Ok::<_, String>(
                    entries
                        .into_iter()
                        .filter_map(|chat_id| {
                            fresh.get(&chat_id).cloned().or_else(|| {
                                cached.iter().find(|row| row.chat_id == chat_id).cloned()
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            }
            .await;
            done(result);
        });
    }
}

/// Whether a message that landed in a chat answers one of the account's
/// own messages there, in a group: the shape deltachat-android counts as
/// a mention. Two reads of the core after the kind of chat, since the
/// quote names its message and says nothing else about it; a quote that
/// carries text alone could be quoting anyone, and is not one.
async fn is_mention(rpc: &RpcClient, account_id: u32, chat_id: u32, message_id: u32) -> bool {
    if !chat_is_group(rpc, account_id, chat_id).await {
        return false;
    }
    let Ok(message) = rpc
        .call::<_, serde_json::Value>("get_message", (account_id, message_id))
        .await
    else {
        return false;
    };
    let Some(quoted) = json::u32_opt(&message, "/quote/messageId").filter(|id| *id != 0) else {
        return false;
    };
    let Ok(original) = rpc
        .call::<_, serde_json::Value>("get_message", (account_id, quoted))
        .await
    else {
        return false;
    };
    // Contact id 1 is the well-known DC_CONTACT_ID_SELF.
    json::u32_opt(&original, "fromId") == Some(1)
}

/// How much of the list a refresh has to re-read from the core.
#[derive(Clone, Copy)]
enum Refresh {
    /// Every visible row. What the core means by a change it reports
    /// without naming a chat, and what `reload` has to do to be worth
    /// calling at all.
    All,
    /// This chat, plus any chat not in the model yet.
    One(u32),
}

/// Move, insert and remove rows until the model matches `target`.
///
/// The common case -- one chat moving to the top -- is one remove and one
/// insert, so every other row keeps its identity and the view keeps its
/// place.
fn reconcile(rows: &mut ChatListModel, target: Vec<ChatListItem>) {
    // The ids as they stand, kept in step with the model rather than read
    // back out of it each time round: rebuilding this per row, and reaching
    // into the model with `nth`, would cost a no-op refresh a scan of the
    // whole list for every chat in it.
    let mut current: Vec<u32> = rows.iter().map(|row| row.chat_id).collect();
    let keep: HashSet<u32> = target.iter().map(|row| row.chat_id).collect();

    // Gone from the core: dropped first, so what follows only ever moves
    // rows that are staying.
    let mut index = 0;
    while index < current.len() {
        if keep.contains(&current[index]) {
            index += 1;
        } else {
            rows.remove(index);
            current.remove(index);
        }
    }

    for (index, wanted) in target.iter().enumerate() {
        // Everything before `index` is already where the core wants it, so
        // only the tail is worth looking through.
        let found = current
            .iter()
            .skip(index)
            .position(|id| *id == wanted.chat_id)
            .map(|offset| index + offset);
        match found {
            Some(at) if at == index => {
                if rows[index] != *wanted {
                    rows.change_line(index, wanted.clone());
                }
            }
            Some(at) => {
                rows.remove(at);
                rows.insert(index, wanted.clone());
                let id = current.remove(at);
                current.insert(index, id);
            }
            None => {
                rows.insert(index, wanted.clone());
                current.insert(index, wanted.chat_id);
            }
        }
    }
}

/// The account's chat ids, in the order the core wants them shown.
/// `DC_GCL_ARCHIVED_ONLY`: the archived chats rather than the ordinary
/// ones. The two lists are disjoint, which is why this is a mode and not a
/// filter over what is already loaded.
const ARCHIVED_ONLY: u32 = 0x01;

/// `DC_GCL_FOR_FORWARDING`: only chats a message can be forwarded into.
const FOR_FORWARDING: u32 = 0x08;

async fn chat_entries(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
    archived: bool,
    for_forwarding: bool,
) -> Result<Vec<u32>, String> {
    // The core does the matching. Filtering the loaded rows instead would
    // only ever find chats that happened to be on screen already.
    //
    // A query of nothing but spaces is rejected by the core outright, so
    // it is trimmed here and treated as no query at all rather than
    // swapping the list for an error banner.
    let query = query.trim();
    let mut flags = 0;
    if for_forwarding {
        flags |= FOR_FORWARDING;
    }

    if archived && !query.is_empty() {
        // Two calls, because the core has no single one that means
        // "archived chats matching this". Verified against the pinned
        // binary: with ARCHIVED_ONLY set it never looks at the query --
        // asking for archived chats matching "Beta" returns the archived
        // "Alpha group" all the same -- while a plain query searches every
        // chat and *does* include archived ones. So the archived list
        // intersected with the hits is exactly what this page is asking
        // for.
        let archived_only: Vec<u32> = rpc
            .call(
                "get_chatlist_entries",
                (
                    account_id,
                    Some(flags | ARCHIVED_ONLY),
                    Option::<String>::None,
                    Option::<u32>::None,
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
        let matching: Vec<u32> = rpc
            .call(
                "get_chatlist_entries",
                (
                    account_id,
                    if flags == 0 { None } else { Some(flags) },
                    Some(query.to_string()),
                    Option::<u32>::None,
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
        // The archived list's order, kept: it is the one the reader sees
        // when nothing is typed, and the search should not reshuffle it.
        let hits: HashSet<u32> = matching.into_iter().collect();
        return Ok(archived_only
            .into_iter()
            .filter(|chat_id| hits.contains(chat_id))
            .collect());
    }

    if archived {
        flags |= ARCHIVED_ONLY;
    }
    let flags = if flags == 0 { None } else { Some(flags) };
    let query = if query.is_empty() {
        None
    } else {
        Some(query.to_string())
    };
    rpc.call(
        "get_chatlist_entries",
        (account_id, flags, query, Option::<u32>::None),
    )
    .await
    .map_err(|err| err.to_string())
}

/// Rows for the given chats, in one call.
pub(crate) async fn chat_items(
    rpc: &RpcClient,
    account_id: u32,
    ids: &[u32],
) -> Result<HashMap<u32, ChatListItem>, String> {
    let items: HashMap<u32, serde_json::Value> = rpc
        .call("get_chatlist_items_by_entries", (account_id, ids))
        .await
        .map_err(|err| err.to_string())?;
    Ok(items
        .into_iter()
        .filter(|(_, item)| json::str_at(item, "kind") == "ChatListItem")
        .map(|(chat_id, item)| {
            (
                chat_id,
                ChatListItem {
                    chat_id,
                    name: json::text(&item, "name"),
                    preview: json::text(&item, "summaryText2"),
                    preview_sender: json::text(&item, "summaryText1"),
                    unread_count: json::u32_at(&item, "freshMessageCounter"),
                    // The core counts in milliseconds here and in seconds
                    // on a message; the UI wants one unit.
                    last_updated: json::i64_at(&item, "lastUpdated") / 1000,
                    summary_state: json::u32_at(&item, "summaryStatus"),
                    is_encrypted: json::flag(&item, "isEncrypted"),
                    is_pinned: json::flag(&item, "isPinned"),
                    is_muted: json::flag(&item, "isMuted"),
                    is_contact_request: json::flag(&item, "isContactRequest"),
                    is_self_talk: json::flag(&item, "isSelfTalk"),
                    is_device_talk: json::flag(&item, "isDeviceTalk"),
                    color: json::text(&item, "color"),
                    avatar_path: json::text(&item, "avatarPath"),
                },
            )
        })
        .collect())
}
