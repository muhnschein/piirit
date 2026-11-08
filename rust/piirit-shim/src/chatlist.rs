//! Chat list QML type.
//!
//! Any message moves its chat to top. Rebuild loses scroll and redraws all, so this
//! reconciles: moves moved row, refetches changed chats only.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;
use serde_json::json;

use crate::chat::chat_is_group;
use crate::core::connection;
use crate::json;
use crate::models::{ChatListItem, ChatListModel};
use crate::transcripts::{self, Change, Previews};

/// One account's chats, most recent first.
///
/// ```qml
/// ChatList { id: chats; account_id: page.accountId }
/// SilicaListView { model: chats.rows }
/// ```
#[derive(QObject, Default)]
// Bools are QML properties; folding them hides them from QML.
#[allow(clippy::struct_excessive_bools)]
pub struct ChatList {
    base: qt_base_class!(trait QObject),

    /// Account. Setting reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Account changed.
    pub account_changed: qt_signal!(),

    /// Search filter; empty: all.
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Query changed.
    pub query_changed: qt_signal!(),

    /// Show archived chats.
    pub archived: qt_property!(bool; WRITE set_archived NOTIFY archived_changed),
    /// Archived flag changed.
    pub archived_changed: qt_signal!(),

    /// Chats this contact is in, archived too; 0: all.
    /// Core ignores `query`, `archived` and `for_forwarding` then.
    pub contact_id: qt_property!(u32; WRITE set_contact_id NOTIFY contact_changed),
    /// Contact changed.
    pub contact_changed: qt_signal!(),

    /// Forwardable chats only. Core drops ones forward would refuse (device chat).
    pub for_forwarding: qt_property!(bool; WRITE set_for_forwarding NOTIFY for_forwarding_changed),
    /// Forwarding flag changed.
    pub for_forwarding_changed: qt_signal!(),

    /// Unread only, filtered here (`get_chatlist_entries` has no flag). Read while on stays
    /// (`kept`): no vanishing under thumb.
    pub unread_only: qt_property!(bool; WRITE set_unread_only NOTIFY unread_only_changed),
    /// Unread filter changed.
    pub unread_only_changed: qt_signal!(),
    /// Release chats read since filter on (app resume).
    pub forget_read: qt_method!(fn(&mut self)),

    /// Chats only, no channels. Turning on turns `channels_only` off.
    pub chats_only: qt_property!(bool; WRITE set_chats_only NOTIFY kind_filter_changed),
    /// Channels only. Turning on turns `chats_only` off.
    pub channels_only: qt_property!(bool; WRITE set_channels_only NOTIFY kind_filter_changed),
    /// Either kind filter changed.
    pub kind_filter_changed: qt_signal!(),
    /// List holds a channel: kind filters offered. Last one gone turns them off.
    pub has_channels: qt_property!(bool; NOTIFY rows_changed),

    /// Reader setting: announce muted-group reply to own message (reference clients' mentions).
    /// Groups only: muted 1:1 was muted for that person.
    pub notify_mentions: qt_property!(bool; NOTIFY notify_mentions_changed),
    /// Setting changed.
    pub notify_mentions_changed: qt_signal!(),

    /// Events only noted, no core reads; one refresh once false. Cover off screen.
    pub paused: qt_property!(bool; WRITE set_paused NOTIFY paused_changed),
    /// Paused changed.
    pub paused_changed: qt_signal!(),

    /// Rows for `SilicaListView`.
    pub rows: qt_property!(RefCell<ChatListModel>; CONST),

    /// Row count.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    // Stored, not counted: headings read these inside model reset, where row list is
    // mutably borrowed (panic).
    /// Pinned count: whether to head the two groups.
    pub pinned_count: qt_property!(u32; NOTIFY rows_changed),
    /// Unpinned count, same decision.
    pub unpinned_count: qt_property!(u32; NOTIFY rows_changed),
    /// Unread across all chats, muted too, for cover.
    pub unread_total: qt_property!(u32; READ unread_total NOTIFY rows_changed),
    /// Cover people: JSON array of `{"chat_id", "name", "color", "avatar_path", "unread_count"}`
    /// in list order, no self or device chat. String: cover lays grid in one pass, repeating few.
    pub cover_people: qt_property!(QString; READ cover_people NOTIFY rows_changed),
    /// Emitted after `rows` change.
    pub rows_changed: qt_signal!(),

    /// Message arrived. `IncomingMsg` only, once refetch landed (event's preview is older).
    /// Muted: not announced. `sender`: name in group, empty in 1:1.
    pub message_arrived: qt_signal!(chat_id: u32, chat_name: QString, sender: QString, preview: QString),

    /// Messages changed without arrival (edit, delete), row read back; listener knows if a
    /// notification is up.
    pub message_restated: qt_signal!(
        chat_id: u32,
        chat_name: QString,
        sender: QString,
        preview: QString,
        unread: u32
    ),

    /// Load failed; core's message.
    pub error: qt_signal!(message: QString),

    /// Reload whole list.
    pub reload: qt_method!(fn(&mut self)),

    /// Feed `core_event`; other accounts ignored.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Mark chat read, unopened.
    pub mark_read: qt_method!(fn(&mut self, chat_id: u32)),
    /// Put one unread back until opened.
    pub mark_unread: qt_method!(fn(&mut self, chat_id: u32)),
    /// Pin, or back to time order.
    pub set_pinned: qt_method!(fn(&mut self, chat_id: u32, pinned: bool)),
    /// Mute or unmute.
    pub set_muted: qt_method!(fn(&mut self, chat_id: u32, muted: bool)),
    /// Move chat out of list.
    pub archive: qt_method!(fn(&mut self, chat_id: u32)),
    /// Accept contact request. QML calls.
    pub accept_chat: qt_method!(fn(&mut self, chat_id: u32)),
    /// Block contact request sender. QML calls.
    pub block_chat: qt_method!(fn(&mut self, chat_id: u32)),
    /// Move chat back to ordinary list.
    pub unarchive: qt_method!(fn(&mut self, chat_id: u32)),
    /// Delete chat and messages on device.
    pub delete_chat: qt_method!(fn(&mut self, chat_id: u32)),

    /// Arrivals unannounced. Apart from own refresh, rarely landing: core follows `IncomingMsg`
    /// with `ChatlistItemChanged`, `ChatlistChanged` same ms, staling it (`generation`).
    pending_announcements: HashSet<u32>,

    /// Muted chats with reader's mention, unannounced. See `check_mention`.
    mentioned: HashSet<u32>,

    /// Edited/deleted chats awaiting read back. As `pending_announcements`.
    pending_restatements: HashSet<u32>,

    /// Counts refreshes: stale answer must not land on newer (archived page sets `account_id`,
    /// `archived`, each fetching; search per keystroke).
    generation: u64,

    /// Owed, not yet asked. Reused rows are as old as last kept answer (mark read, message
    /// elsewhere: badge back for good): owed chats refetched until one lands.
    awaiting: Awaiting,

    /// What refresh in flight asks; back to `awaiting` if it fails or a newer one replaces it.
    asking: Awaiting,

    /// Generation of refresh in flight. One at a time: arrival is three events, burst hundreds;
    /// those wait in `awaiting` for one refresh after it lands (`again`).
    in_flight: Option<u64>,

    /// Asked while one in flight or paused.
    again: bool,

    /// Account, query or filter changed: answer in flight is for old ones. Acted on from event
    /// loop, once QML has set every property (`paused` may come after `account_id`).
    restart_due: bool,

    /// Every chat core listed, unfiltered: refresh, announcements read here. Cover reads `rows`.
    all_rows: Vec<ChatListItem>,

    /// Unread since filter on. Cleared on filter on and by `forget_read`.
    kept: HashSet<u32>,

    /// Told of transcript changes (`transcripts::follow`).
    following: bool,
    /// `transcripts::epoch` when refresh in flight read transcripts.
    asked_epoch: u64,
}

impl ChatList {
    /// Row count.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Unread across all chats.
    pub fn unread_total(&self) -> u32 {
        self.rows
            .borrow()
            .iter()
            .fold(0u32, |total, row| total.saturating_add(row.unread_count))
    }

    /// People JSON; see property.
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

    /// Set account; reload if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.restart(Refresh::All);
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
                // Deleted message may be row's preview.
                | "MsgDeleted"
                | "MsgsNoticed"
                | "MsgDelivered"
                | "MsgRead"
                | "MsgFailed"
                // Pin, mute, archive, delete land here.
                | "ChatModified"
                | "ChatDeleted"
                | "ChatlistChanged"
                | "ChatlistItemChanged"
                // Core dropped events: trust nothing held.
                | "EventChannelOverflow"
        ) {
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        // No chat id (absent, null, 0): re-read all visible (upstream says so). Overflow has
        // no id, could hide anything.
        if kind == "EventChannelOverflow" {
            self.refresh(Refresh::All);
            return;
        }
        let chat_id = json::u32_opt(&payload, "chatId").filter(|id| *id != 0);
        let scope = match chat_id {
            Some(chat_id) => Refresh::One(chat_id),
            // No id, no row change ("set of chats or order changed"), with every message,
            // send, draft beside `ChatlistItemChanged`. As `All`: seconds on long list.
            None if kind == "ChatlistChanged" => Refresh::Order,
            None => Refresh::All,
        };
        // Announce real arrivals only: MsgsChanged fires for sends, receipts, pins.
        let announce = if kind == "IncomingMsg" { chat_id } else { None };
        // Edit (MsgsChanged), delete (MsgDeleted) restate notification; any MsgsChanged too.
        if matches!(kind.as_str(), "MsgsChanged" | "MsgDeleted") {
            if let Some(chat_id) = chat_id {
                self.pending_restatements.insert(chat_id);
            }
        }
        // Muted: announce only if for reader (core query).
        if let Some(chat_id) = announce {
            if self.notify_mentions && self.is_muted(chat_id) {
                if let Some(message_id) = json::u32_opt(&payload, "msgId").filter(|id| *id != 0) {
                    self.check_mention(chat_id, message_id);
                }
            }
        }
        self.refresh_announcing(scope, announce);
    }

    /// Chat muted? Unlisted is new: no.
    fn is_muted(&self, chat_id: u32) -> bool {
        self.all_rows
            .iter()
            .any(|row| row.chat_id == chat_id && row.is_muted)
    }

    /// Announce if muted-chat arrival answers reader's message.
    ///
    /// Answer comes after event's refresh, so mention starts own, chat let past mute once.
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

    /// Set query, reload if changed. Only order: held rows reused, only new ones read (else
    /// clearing search re-reads whole list).
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.restart(Refresh::Order);
        }
    }

    /// Show archived, or ordinary chats.
    pub fn set_archived(&mut self, archived: bool) {
        if self.archived != archived {
            self.archived = archived;
            self.archived_changed();
            self.restart(Refresh::All);
        }
    }

    /// Only this contact's chats, or all.
    pub fn set_contact_id(&mut self, contact_id: u32) {
        if self.contact_id != contact_id {
            self.contact_id = contact_id;
            self.contact_changed();
            self.refresh(Refresh::All);
        }
    }

    /// Only forwardable chats.
    pub fn set_for_forwarding(&mut self, for_forwarding: bool) {
        if self.for_forwarding != for_forwarding {
            self.for_forwarding = for_forwarding;
            self.for_forwarding_changed();
            self.restart(Refresh::All);
        }
    }

    /// Pause, or resume with one refresh of what was owed meanwhile.
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused != paused {
            self.paused = paused;
            self.paused_changed();
            if !paused && (self.restart_due || (self.again && self.in_flight.is_none())) {
                self.restart_due = false;
                self.start_refresh();
            }
        }
    }

    /// Show only unread chats, or all.
    pub fn set_unread_only(&mut self, unread_only: bool) {
        if self.unread_only != unread_only {
            self.unread_only = unread_only;
            // Turned on afresh: from current unread.
            self.kept.clear();
            self.unread_only_changed();
            self.show_later();
        }
    }

    /// Show only chats without channels, or all.
    pub fn set_chats_only(&mut self, on: bool) {
        if self.chats_only != on {
            self.chats_only = on;
            self.channels_only &= !on;
            self.kind_filter_changed();
            self.show_later();
        }
    }

    /// Show only channels, or all.
    pub fn set_channels_only(&mut self, on: bool) {
        if self.channels_only != on {
            self.channels_only = on;
            self.chats_only &= !on;
            self.kind_filter_changed();
            self.show_later();
        }
    }

    /// Release chats read since filter on.
    pub fn forget_read(&mut self) {
        if self.unread_only && !self.kept.is_empty() {
            self.kept.clear();
            self.show_later();
        }
    }

    /// [`Self::show`], from event loop: callers (QML writes/calls) hold object borrowed, and
    /// new delegates' headings read `pinned_count` (second borrow).
    fn show_later(&mut self) {
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let show = queued_callback(move |(): ()| {
            if let Some(this) = ptr.as_pinned() {
                Self::show(this);
            }
        });
        show(());
    }

    /// Rows filters pass, core order.
    fn shown(&mut self) -> Vec<ChatListItem> {
        let (chats_only, channels_only) = (self.chats_only, self.channels_only);
        let kind = move |row: &ChatListItem| {
            if chats_only {
                !row.is_channel
            } else if channels_only {
                row.is_channel
            } else {
                true
            }
        };
        if !self.unread_only {
            return self
                .all_rows
                .iter()
                .filter(|row| kind(row))
                .cloned()
                .collect();
        }
        // Gone chats' ids dropped too.
        let listed: HashSet<u32> = self.all_rows.iter().map(|row| row.chat_id).collect();
        self.kept.retain(|id| listed.contains(id));
        for row in &self.all_rows {
            if row.unread_count > 0 {
                self.kept.insert(row.chat_id);
            }
        }
        self.all_rows
            .iter()
            .filter(|row| kind(row) && self.kept.contains(&row.chat_id))
            .cloned()
            .collect()
    }

    /// Bring model in line with `all_rows` and filter.
    fn show(this: QObjectPinned<'_, Self>) {
        let (target, kind_dropped) = {
            let mut this_mut = this.borrow_mut();
            this_mut.has_channels = this_mut.all_rows.iter().any(|row| row.is_channel);
            // Pills hide with last channel: filter they'd turn off goes too.
            let kind_dropped =
                !this_mut.has_channels && (this_mut.chats_only || this_mut.channels_only);
            if kind_dropped {
                this_mut.chats_only = false;
                this_mut.channels_only = false;
            }
            let target = this_mut.shown();
            // Count before setting rows: setting makes QML read counts.
            let pinned = target.iter().filter(|row| row.is_pinned).count();
            this_mut.pinned_count = u32::try_from(pinned).unwrap_or(u32::MAX);
            this_mut.unpinned_count =
                u32::try_from(target.len().saturating_sub(pinned)).unwrap_or(u32::MAX);
            (target, kind_dropped)
        };
        {
            let this_ref = this.borrow();
            let mut rows = this_ref.rows.borrow_mut();
            reconcile(&mut rows, target);
        }
        this.borrow().rows_changed();
        if kind_dropped {
            this.borrow().kind_filter_changed();
        }
    }

    /// Register once for transcript changes; registration ends with this object.
    fn follow_transcripts(&mut self) {
        if std::mem::replace(&mut self.following, true) {
            return;
        }
        let alive: QPointer<Self> = QPointer::from(&*self);
        let ptr = alive.clone();
        // Deferred: teller's stack may hold this object.
        let changed = queued_callback(move |change: Change| {
            if let Some(this) = ptr.as_pinned() {
                if this.borrow_mut().owe_voice_rows(change) {
                    this.borrow_mut().refresh(Refresh::Order);
                }
            }
        });
        transcripts::follow(move |change| {
            if alive.is_null() {
                return false;
            }
            changed(change);
            true
        });
    }

    /// Owe rows whose last message is voice `change` touches. False: none.
    fn owe_voice_rows(&mut self, change: Change) -> bool {
        let account = self.account_id;
        let chats: Vec<u32> = self
            .all_rows
            .iter()
            .filter(|row| {
                row.voice_message_id != 0
                    && match change {
                        Change::All => true,
                        Change::Message(key) => key == (account, row.voice_message_id),
                    }
            })
            .map(|row| row.chat_id)
            .collect();
        for chat_id in &chats {
            self.awaiting.add(Refresh::One(*chat_id));
            // Notification up shows row's text: transcript in, or core's words back.
            self.pending_restatements.insert(*chat_id);
        }
        !chats.is_empty()
    }

    /// Accept contact request; chat becomes ordinary.
    pub fn accept_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "accept_chat", serde_json::Value::Null);
    }

    /// Block contact request sender.
    pub fn block_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "block_chat", serde_json::Value::Null);
    }

    /// Mark chat read.
    pub fn mark_read(&mut self, chat_id: u32) {
        self.act(chat_id, "marknoticed_chat", serde_json::Value::Null);
    }

    /// Mark chat unread: `markfresh_chat` sets last incoming fresh (one unread until opened).
    /// No incoming message: left alone.
    pub fn mark_unread(&mut self, chat_id: u32) {
        self.act(chat_id, "markfresh_chat", serde_json::Value::Null);
    }

    /// Pin or unpin.
    pub fn set_pinned(&mut self, chat_id: u32, pinned: bool) {
        let visibility = if pinned { "Pinned" } else { "Normal" };
        self.act(chat_id, "set_chat_visibility", json!([visibility]));
    }

    /// Mute or unmute.
    pub fn set_muted(&mut self, chat_id: u32, muted: bool) {
        let kind = if muted { "Forever" } else { "NotMuted" };
        self.act(chat_id, "set_chat_mute_duration", json!([{"kind": kind}]));
    }

    /// Move chat out of list.
    pub fn archive(&mut self, chat_id: u32) {
        self.act(chat_id, "set_chat_visibility", json!(["Archived"]));
    }

    /// Move chat back to ordinary list.
    pub fn unarchive(&mut self, chat_id: u32) {
        self.act(chat_id, "set_chat_visibility", json!(["Normal"]));
    }

    /// Delete chat and messages on device.
    pub fn delete_chat(&mut self, chat_id: u32) {
        self.act(chat_id, "delete_chat", serde_json::Value::Null);
    }

    /// Call `method` on `(account, chat)` plus `extra`, then refresh row (core doesn't
    /// announce all).
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

    /// Waiting announcements, rows current, unmuted chats; all waiting (`pending_announcements`).
    /// Mute pass spent either way.
    fn settled_announcements(&mut self) -> Vec<(u32, QString, QString, QString)> {
        let waiting: Vec<u32> = self.pending_announcements.drain().collect();
        let mut announcements = Vec::new();
        for chat_id in waiting {
            let mentioned = self.mentioned.remove(&chat_id);
            let row = self
                .all_rows
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

    /// Waiting restatements, rows current. Unlisted chats dropped; filtered-out ones kept: their
    /// notifications stay up.
    fn settled_restatements(&mut self) -> Vec<(u32, QString, QString, QString, u32)> {
        let rows = &self.all_rows;
        self.pending_restatements
            .drain()
            .filter_map(|chat_id| rows.iter().find(|row| row.chat_id == chat_id))
            .map(|row| {
                (
                    row.chat_id,
                    row.name.clone(),
                    row.preview_sender.clone(),
                    row.preview.clone(),
                    row.unread_count,
                )
            })
            .collect()
    }

    /// Bring model in line with core: refetch per [`Refresh`] plus `awaiting`, reuse rest.
    fn refresh(&mut self, scope: Refresh) {
        self.refresh_announcing(scope, None);
    }

    /// First screenful of first load shown early, unless newer asked since `generation`.
    /// Settles nothing (owed answers, announcements wait for whole list).
    fn show_early(&self, generation: u64) -> impl Fn(Vec<ChatListItem>) + Send + Sync + Clone {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |rows: Vec<ChatListItem>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().all_rows = rows;
            Self::show(this);
        })
    }

    /// [`Self::refresh`], announcing `announce` once row holds new preview.
    fn refresh_announcing(&mut self, scope: Refresh, announce: Option<u32>) {
        if self.account_id == 0 {
            return;
        }
        if let Some(chat_id) = announce {
            self.pending_announcements.insert(chat_id);
        }
        self.awaiting.add(scope);
        if self.paused || self.in_flight.is_some() || self.restart_due {
            self.again = true;
            return;
        }
        self.start_refresh();
    }

    /// Account, query, filter changed: answer in flight is for old ones, ask anew.
    fn restart(&mut self, scope: Refresh) {
        self.awaiting.add(scope);
        if self.restart_due {
            return;
        }
        self.restart_due = true;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let kick = queued_callback(move |(): ()| {
            if let Some(this) = ptr.as_pinned() {
                let mut this_mut = this.borrow_mut();
                if !this_mut.paused {
                    this_mut.restart_due = false;
                    this_mut.start_refresh();
                }
            }
        });
        kick(());
    }

    /// Refresh `generation` answered: rows and announcements, then refresh owed meanwhile.
    fn landed(
        this: QObjectPinned<'_, Self>,
        generation: u64,
        result: Result<Vec<ChatListItem>, String>,
    ) {
        // Superseded: newer one asks this one's chats too.
        if this.borrow().in_flight != Some(generation) {
            return;
        }
        {
            let mut this_mut = this.borrow_mut();
            this_mut.in_flight = None;
            let asked = std::mem::take(&mut this_mut.asking);
            if result.is_err() {
                this_mut.awaiting.merge(asked);
            }
        }
        match result {
            Ok(target) => {
                this.borrow_mut().all_rows = target;
                Self::show(this);
                let announcements = this.borrow_mut().settled_announcements();
                for (chat_id, name, sender, preview) in announcements {
                    this.borrow()
                        .message_arrived(chat_id, name, sender, preview);
                }
                let restatements = this.borrow_mut().settled_restatements();
                for (chat_id, name, sender, preview, unread) in restatements {
                    this.borrow()
                        .message_restated(chat_id, name, sender, preview, unread);
                }
                // Transcript written or switch flipped meanwhile: voice rows read again. After
                // restatements: those owed now wait for re-read rows.
                let stale = this.borrow().asked_epoch != transcripts::epoch();
                if stale && this.borrow_mut().owe_voice_rows(Change::All) {
                    this.borrow_mut().again = true;
                }
            }
            Err(err) => this.borrow().error(err.into()),
        }
        let again = {
            let this_ref = this.borrow();
            this_ref.again && !this_ref.paused && !this_ref.restart_due
        };
        if again {
            this.borrow_mut().start_refresh();
        }
    }

    /// Ask core for `awaiting` (plus what replaced refresh asked); one in flight after.
    fn start_refresh(&mut self) {
        let account_id = self.account_id;
        self.again = false;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.follow_transcripts();
        let previews = transcripts::previews();
        self.asked_epoch = transcripts::epoch();
        let query = self.query.to_string();
        let archived = self.archived;
        let for_forwarding = self.for_forwarding;
        let contact_id = self.contact_id;
        let cached: Vec<ChatListItem> = self.all_rows.clone();
        // Nothing held: app start, profile switch, archived page.
        let first_load = cached.is_empty();
        // Set: list scan per entry is quadratic.
        let known: HashSet<u32> = cached.iter().map(|row| row.chat_id).collect();

        let mut asked = std::mem::take(&mut self.awaiting);
        if self.in_flight.is_some() {
            asked.merge(std::mem::take(&mut self.asking));
        }
        self.asking = asked.clone();
        let refetch_all = matches!(asked, Awaiting::Everything);
        let awaiting = match &asked {
            Awaiting::Chats(chats) => chats.clone(),
            Awaiting::Nothing | Awaiting::Everything => HashSet::new(),
        };

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.in_flight = Some(generation);

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ChatListItem>, String>| {
            if let Some(this) = ptr.as_pinned() {
                Self::landed(this, generation, result);
            }
        });

        let early = self.show_early(generation);

        runtime.spawn(async move {
            let result = async {
                let entries = if contact_id == 0 {
                    chat_entries(&rpc, account_id, &query, archived, for_forwarding).await?
                } else {
                    contact_entries(&rpc, account_id, contact_id).await?
                };
                let wanted: Vec<u32> = entries
                    .iter()
                    .copied()
                    .filter(|id| refetch_all || awaiting.contains(id) || !known.contains(id))
                    .collect();
                // Core builds rows serially (1.9 s for 200 chats on desktop CPU, several times
                // on phone): screenful first.
                let (first, rest) = if first_load {
                    wanted.split_at(wanted.len().min(FIRST_SCREENFUL))
                } else {
                    (wanted.as_slice(), &[][..])
                };
                let mut fresh = if first.is_empty() {
                    HashMap::new()
                } else {
                    chat_items(&rpc, account_id, first).await?
                };
                with_transcripts(previews.as_ref(), account_id, &mut fresh).await;
                if !rest.is_empty() {
                    early(
                        entries
                            .iter()
                            .filter_map(|chat_id| fresh.get(chat_id).cloned())
                            .collect(),
                    );
                    let mut more = chat_items(&rpc, account_id, rest).await?;
                    with_transcripts(previews.as_ref(), account_id, &mut more).await;
                    fresh.extend(more);
                }
                // Order from core; contents from fetch, else model.
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

/// Arrival quotes account's own message in group (`deltachat-android` mention). Text-only
/// quote could be anyone's: not counted.
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
    // Contact id 1 = DC_CONTACT_ID_SELF.
    json::u32_opt(&original, "fromId") == Some(1)
}

/// Unanswered core asks. See field.
#[derive(Clone, Default)]
enum Awaiting {
    /// Every question answered.
    #[default]
    Nothing,
    /// These chats asked about.
    Chats(HashSet<u32>),
    /// Whole list asked. One row re-read won't do: answer carrying rest may be dropped stale.
    Everything,
}

impl Awaiting {
    /// Owe both.
    fn merge(&mut self, other: Awaiting) {
        match other {
            Awaiting::Nothing => {}
            Awaiting::Everything => *self = Awaiting::Everything,
            Awaiting::Chats(chats) => {
                for chat_id in chats {
                    self.add(Refresh::One(chat_id));
                }
            }
        }
    }

    /// Add what refresh will ask for.
    fn add(&mut self, scope: Refresh) {
        match (&mut *self, scope) {
            (Awaiting::Everything, _) | (_, Refresh::All) => *self = Awaiting::Everything,
            // Only what listing turns up new.
            (_, Refresh::Order) => {}
            (Awaiting::Chats(chats), Refresh::One(chat_id)) => {
                chats.insert(chat_id);
            }
            (Awaiting::Nothing, Refresh::One(chat_id)) => {
                *self = Awaiting::Chats(HashSet::from([chat_id]));
            }
        }
    }
}

/// How much of list refresh re-reads.
#[derive(Clone, Copy)]
enum Refresh {
    /// Every visible row (unattributed change, `reload`).
    All,
    /// Order and unlisted chats; held rows still right.
    Order,
    /// This chat and unlisted chats.
    One(u32),
}

/// Move, insert, remove rows until model matches `target`.
///
/// Chat to top: one remove, one insert; other rows keep identity, view keeps place.
fn reconcile(rows: &mut ChatListModel, target: Vec<ChatListItem>) {
    // Ids kept in step: rebuild or `nth` makes no-op refresh quadratic.
    let mut current: Vec<u32> = rows.iter().map(|row| row.chat_id).collect();
    let keep: HashSet<u32> = target.iter().map(|row| row.chat_id).collect();

    // Gone chats dropped first: rest only moves.
    let mut index = 0;
    while let Some(id) = current.get(index) {
        if keep.contains(id) {
            index = index.saturating_add(1);
        } else {
            rows.remove(index);
            current.remove(index);
        }
    }

    for (index, wanted) in target.iter().enumerate() {
        // Before `index` right: scan tail.
        let found = current
            .iter()
            .skip(index)
            .position(|id| *id == wanted.chat_id)
            .map(|offset| index.saturating_add(offset));
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

/// Account's chat ids, core display order. `DC_GCL_ARCHIVED_ONLY`: archived only (mode, not
/// filter).
const ARCHIVED_ONLY: u32 = 0x01;

/// `DC_GCL_FOR_FORWARDING`: forwardable only.
const FOR_FORWARDING: u32 = 0x08;

async fn chat_entries(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
    archived: bool,
    for_forwarding: bool,
) -> Result<Vec<u32>, String> {
    // Core matches (loaded rows hold only on-screen chats). Spaces-only: core rejects, trim.
    let query = query.trim();
    let mut flags = 0;
    if for_forwarding {
        flags |= FOR_FORWARDING;
    }

    if archived && !query.is_empty() {
        // Two calls, no "archived matching": on pinned binary ARCHIVED_ONLY ignores query
        // ("Beta" returns "Alpha group"); plain query searches all. Intersection is this page.
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
        // Archived order: search mustn't reshuffle.
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

/// Chats the contact is in, in core's order.
async fn contact_entries(
    rpc: &RpcClient,
    account_id: u32,
    contact_id: u32,
) -> Result<Vec<u32>, String> {
    rpc.call(
        "get_chatlist_entries",
        (
            account_id,
            Option::<u32>::None,
            Option::<String>::None,
            Some(contact_id),
        ),
    )
    .await
    .map_err(|err| err.to_string())
}

/// Rows first load reads: over a phone screen.
const FIRST_SCREENFUL: usize = 20;

/// Rows for given chats, one call.
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
                    // Core: ms here, s on message; UI one unit.
                    last_updated: json::i64_at(&item, "lastUpdated") / 1000,
                    summary_state: json::u32_at(&item, "summaryStatus"),
                    is_encrypted: json::flag(&item, "isEncrypted"),
                    is_pinned: json::flag(&item, "isPinned"),
                    is_muted: json::flag(&item, "isMuted"),
                    is_contact_request: json::flag(&item, "isContactRequest"),
                    is_self_talk: json::flag(&item, "isSelfTalk"),
                    is_device_talk: json::flag(&item, "isDeviceTalk"),
                    is_channel: matches!(
                        json::str_at(&item, "chatType"),
                        "OutBroadcast" | "InBroadcast"
                    ),
                    color: json::text(&item, "color"),
                    avatar_path: json::text(&item, "avatarPath"),
                    voice_message_id: if json::str_at(&item, "lastMessageType") == "Voice" {
                        json::u32_at(&item, "lastMessageId")
                    } else {
                        0
                    },
                },
            )
        })
        .collect())
}

/// Core's mark on voice previews (`🎤 Voice message`); transcript follows it as text would.
const VOICE_MARK: &str = "🎤";

/// Voice rows with done transcript preview it in place of core's words.
async fn with_transcripts(
    previews: Option<&Previews>,
    account_id: u32,
    items: &mut HashMap<u32, ChatListItem>,
) {
    let Some(previews) = previews else { return };
    let voice: Vec<u32> = items
        .values()
        .map(|item| item.voice_message_id)
        .filter(|msg| *msg != 0)
        .collect();
    let texts = previews.texts(account_id, voice).await;
    for item in items.values_mut() {
        if let Some(text) = texts.get(&item.voice_message_id) {
            item.preview = voice_preview(text).into();
        }
    }
}

/// `🎤 text`, one line as core previews text (`real_server.rs`).
pub(crate) fn voice_preview(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("{VOICE_MARK} {line}")
}

#[cfg(test)]
mod tests {
    use super::voice_preview;

    #[test]
    fn a_transcript_previews_on_one_line() {
        assert_eq!(voice_preview(" hello\n there\t "), "🎤 hello there");
        let long = "word ".repeat(400);
        assert_eq!(voice_preview(&long), format!("🎤 {}", long.trim_end()));
    }
}
