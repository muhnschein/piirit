//! One chat's messages, as a QML type.
//!
//! One per conversation page: no shared model. Batch load; events update rows in place.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Local, TimeZone};
use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;
use crate::models::{MessageListItem, MessageListModel};
use crate::video::Prepared;
use crate::{calls, links, markdown, media, video, webxdc};

/// `DC_STATE_IN_FRESH`, `DC_STATE_IN_NOTICED`: unread incoming.
const UNSEEN_STATES: [u32; 2] = [10, 13];

/// Messages per fetch.
///
/// Ids cheap, messages not: model holds a row per message once id list arrives, fills only
/// viewed ones. First message stays row 0: moving window would lose view's place
/// (Whisperfish, deltachat-android likewise).
const PAGE: usize = 50;

/// Rows filled past view. Screen ~thirty one-line placeholders; less and fast scroll hits
/// blanks.
const MARGIN: usize = 40;

/// Max fetches in flight: fling outruns one round trip, and fetch for passed rows would
/// block rows ahead. Bounded: long flick asks only what it passes.
const IN_FLIGHT: usize = 4;

/// Message in id list: id and day.
///
/// Day with id, not message: unknown-day placeholder re-sectioned on fetch drew heading over
/// row beneath; waiting rows shared "1 January 1970". Core's day markers cost one flag.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    pub message_id: u32,
    pub day_number: i64,
}

/// Page a chat opens with: at `find`, or newest if not in chat (0 too).
pub(crate) fn opening_page(entries: &[Entry], find: u32) -> &[Entry] {
    let start = match entries.iter().position(|entry| entry.message_id == find) {
        // Half page above: not on top edge.
        Some(index) => index.saturating_sub(PAGE / 2),
        None => entries.len().saturating_sub(PAGE),
    };
    let end = start.saturating_add(PAGE).min(entries.len());
    entries.get(start..end).unwrap_or_default()
}

/// Message ids of entries, for `get_messages`.
pub(crate) fn ids_of(entries: &[Entry]) -> Vec<u32> {
    entries.iter().map(|entry| entry.message_id).collect()
}

/// One row per message; `items` filled where fetched.
fn rows_for(entries: &[Entry], items: Vec<MessageListItem>) -> Vec<MessageListItem> {
    let mut fetched: BTreeMap<u32, MessageListItem> = items
        .into_iter()
        .map(|item| (item.message_id, item))
        .collect();
    entries
        .iter()
        .map(|entry| {
            fetched
                .remove(&entry.message_id)
                .unwrap_or_else(|| placeholder(*entry))
        })
        .collect()
}

/// Messages of one chat.
///
/// ```qml
/// ChatMessages { id: messages; account_id: 1; chat_id: 7 }
/// SilicaListView { model: messages.rows }
/// ```
#[derive(QObject, Default)]
// Seven facts QML binds to, any mix legal: not one state enum.
#[allow(clippy::struct_excessive_bools)]
pub struct ChatMessages {
    base: qt_base_class!(trait QObject),

    /// Account of chat. Set reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY chat_changed),
    /// Chat id. Set reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY chat_changed),
    /// True once load finished, however. `count` can't tell empty from not-arrived ("no
    /// messages yet" flashed).
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted on `loaded` change.
    pub loaded_changed: qt_signal!(),
    /// Emitted on chat change.
    pub chat_changed: qt_signal!(),

    /// Group, mailing list or broadcast: senders shown.
    pub is_group: qt_property!(bool; NOTIFY is_group_changed),
    /// Emitted once chat kind known.
    pub is_group_changed: qt_signal!(),
    /// Member count incl. reader, for header; 0 until known, and for 1:1.
    pub member_count: qt_property!(u32; NOTIFY member_count_changed),
    /// Emitted when member count read and changed.
    pub member_count_changed: qt_signal!(),
    /// Account can write (not if left group / unaccepted request). Gates edit offer, as
    /// reference client.
    pub can_send: qt_property!(bool; NOTIFY can_send_changed),
    /// Emitted once can-send known.
    pub can_send_changed: qt_signal!(),
    /// End-to-end encrypted. Plain-email chat gets no edit: mail program shows it as another
    /// mail (reference client likewise).
    pub is_encrypted: qt_property!(bool; NOTIFY is_encrypted_changed),
    /// Emitted once encryption known.
    pub is_encrypted_changed: qt_signal!(),
    /// Call placeable: one other person, encrypted, writable, not "Saved messages" (reference
    /// clients' rule; core refuses elsewhere).
    pub can_call: qt_property!(bool; NOTIFY can_call_changed),
    /// Emitted once call-ability known.
    pub can_call_changed: qt_signal!(),
    /// Unaccepted contact request (someone new wrote, or added reader to a group). Page shows
    /// accept and block (group: delete) in field's place, as reference clients.
    pub is_contact_request: qt_property!(bool; NOTIFY is_contact_request_changed),
    /// Emitted once request-ness known, and when accepted (here or other device).
    pub is_contact_request_changed: qt_signal!(),
    /// Accept request: core makes it ordinary chat; shape re-read (field comes back).
    pub accept_chat: qt_method!(fn(&mut self)),
    /// Block request's sender: core hides chat, contact goes on block list. Answers `closed`.
    pub block_chat: qt_method!(fn(&mut self)),
    /// Delete chat on this device (group request turned down). Answers `closed`.
    pub delete_chat: qt_method!(fn(&mut self)),
    /// Chat blocked or deleted: nothing left to show, page leaves.
    pub closed: qt_signal!(),
    /// Chat name as core shows; empty until read. Re-read on events (renames elsewhere).
    pub chat_name: qt_property!(QString; NOTIFY chat_name_changed),
    /// Emitted when `chat_name` re-read and changed.
    pub chat_name_changed: qt_signal!(),

    /// Rows, for a `SilicaListView` `model`.
    pub rows: qt_property!(RefCell<MessageListModel>; CONST),

    /// Row count: `QAbstractListModel` has no QML `count`.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted on any `rows` change.
    pub rows_changed: qt_signal!(),

    /// Load or send failure; core's message.
    pub error: qt_signal!(message: QString),

    /// Reload chat. Called on chat change.
    pub reload: qt_method!(fn(&mut self)),

    /// Fill rows `first..last` plus margin as view scrolls; only message fetch after open.
    pub hydrate: qt_method!(fn(&mut self, first: i32, last: i32)),
    /// Rows filling; quiet indicator.
    pub hydrating: qt_property!(bool; NOTIFY hydrating_changed),
    /// Emitted on `hydrating` change.
    pub hydrating_changed: qt_signal!(),

    /// Scroll target for `message_id`.
    pub reveal: qt_method!(fn(&mut self, message_id: u32)),
    /// `message_id` at `row`, or -1 if not in this chat.
    pub revealed: qt_signal!(message_id: u32, row: i32),

    /// Unsent text, kept by core: survives app close, in chat list summary.
    pub draft: qt_property!(QString; NOTIFY draft_changed),
    /// Emitted on draft load or write.
    pub draft_changed: qt_signal!(),
    /// Set draft; empty clears.
    pub save_draft: qt_method!(fn(&mut self, text: QString)),

    /// Feed `core_event`. Other accounts' and chats' events ignored.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Reader up in history: only visible messages marked read; arrivals below stay unread
    /// (badge too). Default false; set after chat id.
    pub reading_history: qt_property!(bool; WRITE set_reading_history NOTIFY reading_history_changed),
    /// Emitted on `reading_history` change.
    pub reading_history_changed: qt_signal!(),

    /// Message next send replies to; 0 = none. Send clears.
    pub quoted_message_id: qt_property!(u32; NOTIFY quote_changed),
    /// Emitted on `quoted_message_id` change.
    pub quote_changed: qt_signal!(),

    /// Strip tracking params from sent links. Set by page; see `links.rs`.
    pub clean_links: qt_property!(bool; NOTIFY clean_links_changed),
    /// Emitted on `clean_links` change.
    pub clean_links_changed: qt_signal!(),

    /// File on attachment bar; empty if none. Here so core says if it exceeds what some relays
    /// take.
    pub pending_file: qt_property!(QString; WRITE set_pending_file NOTIFY pending_file_changed),
    /// Emitted on `pending_file` change.
    pub pending_file_changed: qt_signal!(),
    /// `pending_file` send size, bytes; 0 if none. Shrinking video: output size (`video.rs`);
    /// else phone size. f64: QML has no 64-bit int.
    pub attachment_bytes: qt_property!(f64; NOTIFY attachment_large_changed),
    /// Phone size of `pending_file` if sent shrunk, else 0: bar shows saving. f64.
    pub original_bytes: qt_property!(f64; NOTIFY attachment_large_changed),
    /// Core's recommended max attachment, bytes; 0 until known. Not relay's real limit
    /// (`media.rs`). f64.
    pub attachment_limit: qt_property!(f64; NOTIFY attachment_limit_changed),
    /// Emitted when limit read.
    pub attachment_limit_changed: qt_signal!(),
    /// `pending_file` over limit: page warns, still sends. False while limit unknown, and for
    /// pictures (core recodes).
    pub attachment_large: qt_property!(bool; NOTIFY attachment_large_changed),
    /// Emitted on any attachment size or limit change.
    pub attachment_large_changed: qt_signal!(),

    /// Outgoing media quality, core's `media_quality`: 0 balanced, 1 worse. Video shrink target
    /// (`video.rs`); core's for pictures.
    pub media_quality: qt_property!(u32; WRITE set_media_quality NOTIFY media_quality_changed),
    /// Emitted on `media_quality` change.
    pub media_quality_changed: qt_signal!(),
    /// Bar video being shrunk. Nothing sent until done and reader sends; caption writable
    /// meanwhile.
    pub preparing: qt_property!(bool; NOTIFY preparing_changed),
    /// Progress of `preparing`, 0 to 1.
    pub preparing_progress: qt_property!(f64; NOTIFY preparing_changed),
    /// Emitted on `preparing` or progress change.
    pub preparing_changed: qt_signal!(),
    /// Stop shrinking video. Nothing sent; file stays on bar.
    pub cancel_preparing: qt_method!(fn(&mut self)),

    /// Fetch rest of header-only message. Core announces result as message change.
    pub download_full: qt_method!(fn(&mut self, message_id: u32)),

    /// Send plain-text message.
    pub send: qt_method!(fn(&mut self, text: QString)),
    /// Send message with one file (core's shape); `text` may be empty. Core types it from file,
    /// copies into blob dir: picked file may go after.
    pub send_file: qt_method!(fn(&mut self, text: QString, file_path: QString)),
    /// Send recording as `Voice` type; typed by file it would be music.
    pub send_voice: qt_method!(fn(&mut self, file_path: QString)),
    /// Mark loaded unread messages read. Called at newest message.
    pub mark_seen_all: qt_method!(fn(&mut self)),

    /// Row of `message_id`, or -1 if not loaded. Search opens at match, not newest.
    pub row_of: qt_method!(fn(&self, message_id: u32) -> i32),
    /// Delete here and on mail server, where this client keeps it.
    pub delete_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Delete for everyone (`delete_messages_for_all`). Core takes own encrypted messages only
    /// (menu checks `is_outgoing`, `show_padlock`); refusal on `error`.
    pub delete_message_for_all: qt_method!(fn(&mut self, message_id: u32)),
    /// Retry failed message.
    pub resend_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Replace own message text everywhere (`send_edit_request`). Answers `edited` or `error`.
    pub edit_message: qt_method!(fn(&mut self, message_id: u32, text: QString)),
    /// Edit reached core; row shows new text.
    pub edited: qt_signal!(message_id: u32),
    /// Copy message into another chat; QML calls this.
    pub forward_to: qt_method!(fn(&mut self, message_id: u32, chat_id: u32)),
    /// Toggle `emoji` as own reaction; one per person, other emoji replaces.
    pub react: qt_method!(fn(&mut self, message_id: u32, emoji: QString)),
    /// Send request until core answers. Compose clears on answer (failed send keeps input), so
    /// content stays for seconds (large video copy): guards double send.
    pub sending: qt_property!(bool; NOTIFY sending_changed),
    /// Emitted on `sending` change.
    pub sending_changed: qt_signal!(),
    /// Message "new messages" line sits above; 0 if none. Read once per chat: visible rows are
    /// marked read just after open, so a live line would vanish unseen.
    pub unread_from: qt_property!(u32; NOTIFY unread_from_changed),
    /// Emitted when line moves (chat opened).
    pub unread_from_changed: qt_signal!(),

    /// Own message reached core and is in `rows`.
    pub sent: qt_signal!(message_id: u32),
    /// Others' messages just added. Row count can't tell: moves on deletes, still when removal
    /// and arrival share a reload.
    pub arrived: qt_signal!(count: u32),

    /// Chat `unread_from` read for. Same-chat reload (core up, overflow, reorder) keeps line:
    /// re-ask says nothing unread.
    unread_marked_chat: u32,

    /// Bar video shrunk if worth it; restarted on limit or quality change.
    recode: Option<Recode>,
    /// Numbers each `Recode`: stale finished work told apart.
    next_recode: u64,
    /// Shrunk video core is copying to blob dir: kept even if bar moves on.
    in_flight: Option<String>,

    /// Account `attachment_limit` read for; 0 none. Per profile in core: asked once each.
    limit_account: u32,

    /// Rows asked, not yet answered; next ask skips them.
    pending: HashSet<u32>,
    /// Fetches in flight; `hydrating` if any.
    fetches: usize,
    /// Reload counter: fill in flight during reload lands nowhere.
    generation: u64,

    /// Id-list read in flight; one at a time: each reads whole chat, and bursts (night
    /// catch-up, busy group) started one per message, each diffed on Qt thread.
    syncing: bool,
    /// Change during read: one more read follows.
    sync_again: bool,
    /// Messages re-read one at a time, each once (as `syncing`).
    refreshing: HashSet<u32>,
    /// See [`Self::refreshing`].
    refresh_again: HashSet<u32>,
}

impl ChatMessages {
    /// Row count.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set account; reload if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.loaded = false;
            self.clear_unread_mark();
            self.loaded_changed();
            self.chat_changed();
            self.reload();
        }
    }

    /// Forget last chat's unread run start.
    fn clear_unread_mark(&mut self) {
        self.unread_marked_chat = 0;
        if self.unread_from != 0 {
            self.unread_from = 0;
            self.unread_from_changed();
        }
    }

    /// Ask core where reader left off, once per chat (`unread_marked_chat`). From `reload`
    /// before fetch marked read: answer is state on arrival.
    fn load_unread_mark(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 || self.unread_marked_chat == chat_id {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |first: Option<u32>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Reader may have left: answer not this chat's.
            if this.borrow().chat_id != chat_id {
                return;
            }
            let changed = {
                let mut this_mut = this.borrow_mut();
                this_mut.unread_marked_chat = chat_id;
                let mark = first.unwrap_or(0);
                let changed = this_mut.unread_from != mark;
                this_mut.unread_from = mark;
                changed
            };
            if changed {
                this.borrow().unread_from_changed();
            }
        });

        runtime.spawn(async move {
            let first = rpc
                .call::<_, Option<u32>>("get_first_unread_message_of_chat", (account_id, chat_id))
                .await
                .ok()
                .flatten();
            done(first);
        });
    }

    /// Take file on attachment bar; say if relay takes it.
    pub fn set_pending_file(&mut self, file_path: QString) {
        if self.pending_file == file_path {
            return;
        }
        self.pending_file = file_path;
        self.pending_file_changed();
        self.weigh_pending_file();
    }

    /// Take outgoing media quality; replan bar file.
    pub fn set_media_quality(&mut self, quality: u32) {
        if self.media_quality == quality {
            return;
        }
        self.media_quality = quality;
        self.media_quality_changed();
        self.weigh_pending_file();
    }

    /// Bar path as core opens it (picker gives URL).
    fn pending_path(&self) -> String {
        local_path(&self.pending_file.to_string())
    }

    /// Plan bar file vs limit; shrink if worth it; say weight.
    fn weigh_pending_file(&mut self) {
        let path = self.pending_path();
        let limit = self.recommended_limit();
        let plan = video::plan(&path, self.media_quality, limit);
        // Same file and plan: keep. As-is file (stopped, unshrinkable) stays.
        let same = match (&self.recode, &plan) {
            (Some(recode), _) if recode.source == path && recode.as_is => true,
            (Some(recode), Some(target)) => recode.source == path && recode.target == *target,
            _ => false,
        };
        if !same {
            self.drop_recode();
            if let Some(target) = plan {
                self.start_recode(path, target);
            }
        }
        self.weigh();
    }

    /// Bar file's send weight; large if past limit. Shrinking video: planned, then output size.
    /// Else (incl. unshrinkable): phone size.
    fn weigh(&mut self) {
        let path = self.pending_path();
        let limit = self.recommended_limit();
        let smaller = self.recode.as_ref().and_then(|recode| {
            if recode.as_is {
                None
            } else if let Some(made) = &recode.made {
                Some(media::file_bytes(made))
            } else {
                Some(recode.target.predicted_bytes())
            }
        });
        let (bytes, large, original) = match smaller {
            Some(bytes) => (bytes, limit > 0 && bytes > limit, media::file_bytes(&path)),
            None => (
                media::file_bytes(&path),
                media::exceeds_limit(&path, limit),
                0,
            ),
        };
        // Exact to 2^53 bytes: no phone holds more.
        #[allow(clippy::cast_precision_loss)]
        let (bytes, original) = (bytes as f64, original as f64);
        #[allow(clippy::float_cmp)]
        let changed = self.attachment_large != large
            || self.attachment_bytes != bytes
            || self.original_bytes != original;
        if changed {
            self.attachment_large = large;
            self.attachment_bytes = bytes;
            self.original_bytes = original;
            self.attachment_large_changed();
        }
    }

    /// Shrink `source` to `target` on own thread.
    fn start_recode(&mut self, source: String, target: piirit_video::Target) {
        self.next_recode = self.next_recode.wrapping_add(1);
        let id = self.next_recode;
        let cancel = Arc::new(AtomicBool::new(false));
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let progress = queued_callback(move |fraction: f64| {
            let Some(this) = ptr.as_pinned() else { return };
            let current = this
                .borrow()
                .recode
                .as_ref()
                .is_some_and(|recode| recode.id == id && recode.made.is_none());
            if current {
                this.borrow_mut().preparing_progress = fraction;
                this.borrow().preparing_changed();
            }
        });
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |prepared: Prepared| {
            let Some(this) = ptr.as_pinned() else {
                // Conversation went meanwhile.
                if let Prepared::Made(made) = prepared {
                    video::discard(&made);
                }
                return;
            };
            this.borrow_mut().recoded(id, prepared);
        });
        let started = video::start(source.clone(), target, Arc::clone(&cancel), progress, done);
        let as_is = match started {
            Ok(()) => false,
            Err(err) => {
                eprintln!("piirit: sending the video as it is: {err}");
                true
            }
        };
        self.recode = Some(Recode {
            id,
            source,
            target,
            cancel,
            made: None,
            as_is,
        });
        if !as_is {
            self.set_preparing(true);
        }
    }

    /// Shrink done or not: hold result for reader to send.
    fn recoded(&mut self, id: u64, prepared: Prepared) {
        let Some(recode) = self.recode.as_mut().filter(|recode| recode.id == id) else {
            // Moved on or plan changed meanwhile.
            if let Prepared::Made(made) = prepared {
                video::discard(&made);
            }
            return;
        };
        match prepared {
            // x264 may not beat input (short, mostly still): send original.
            Prepared::Made(made)
                if media::file_bytes(&made) >= media::file_bytes(&recode.source) =>
            {
                eprintln!("piirit: sending the video as it is: it came out no smaller");
                video::discard(&made);
                recode.as_is = true;
            }
            Prepared::Made(made) => recode.made = Some(made),
            // Courtesy to relay: unreadable video goes as is.
            Prepared::Failed(reason) => {
                eprintln!("piirit: sending the video as it is: {reason}");
                recode.as_is = true;
            }
            // Reader stopped it: file kept as is; see `cancel_preparing`.
            Prepared::Cancelled => {}
        }
        self.set_preparing(false);
        self.weigh();
    }

    /// Stop shrinking bar's video; drop output unless core is copying it.
    fn drop_recode(&mut self) {
        if let Some(recode) = self.recode.take() {
            recode.cancel.store(true, Ordering::Relaxed);
            if let Some(made) = recode.made {
                if self.in_flight.as_deref() != Some(made.as_str()) {
                    video::discard(&made);
                }
            }
        }
        self.set_preparing(false);
    }

    /// Set preparing flag from start.
    fn set_preparing(&mut self, preparing: bool) {
        #[allow(clippy::float_cmp)]
        if self.preparing != preparing || self.preparing_progress != 0.0 {
            self.preparing = preparing;
            self.preparing_progress = 0.0;
            self.preparing_changed();
        }
    }

    /// Core's recommended limit, bytes; 0 until known.
    fn recommended_limit(&self) -> u64 {
        // Exact to 2^53 bytes: no relay takes more.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        {
            self.attachment_limit.max(0.0) as u64
        }
    }

    /// Ask core recommended max attachment, once per profile (`limit_account`).
    fn load_attachment_limit(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 || self.limit_account == account_id {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |limit: u64| {
            let Some(this) = ptr.as_pinned() else { return };
            // Reader may have moved profile: answer not this one's.
            if this.borrow().account_id != account_id {
                return;
            }
            {
                let mut this_mut = this.borrow_mut();
                this_mut.limit_account = account_id;
                // Exact to 2^53 bytes: no relay takes more.
                #[allow(clippy::cast_precision_loss)]
                {
                    this_mut.attachment_limit = limit as f64;
                }
            }
            this.borrow().attachment_limit_changed();
            this.borrow_mut().weigh_pending_file();
        });

        runtime.spawn(async move {
            done(media::attachment_limit(&rpc, account_id).await);
        });
    }

    /// Set chat; reload if changed.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.loaded = false;
            // Old chat's draft is not this one's.
            self.draft = QString::default();
            self.clear_unread_mark();
            self.loaded_changed();
            self.draft_changed();
            self.chat_changed();
            self.reload();
            self.load_draft();
        }
    }

    /// Read draft: own call, so unsent words don't wait on slow page fetch.
    fn load_draft(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |text: String| {
            let Some(this) = ptr.as_pinned() else { return };
            // Reader moved on or typed meanwhile: keep newer.
            if this.borrow().chat_id != chat_id || !this.borrow().draft.to_string().is_empty() {
                return;
            }
            if text.is_empty() {
                return;
            }
            this.borrow_mut().draft = text.into();
            this.borrow().draft_changed();
        });

        runtime.spawn(async move {
            // Null or a message object. Pinned: deltachat-jsonrpc/tests/real_server.rs.
            let draft: serde_json::Value = rpc
                .call("get_draft", (account_id, chat_id))
                .await
                .unwrap_or(serde_json::Value::Null);
            done(json::str_at(&draft, "text").to_string());
        });
    }

    /// Set draft; empty clears.
    pub fn save_draft(&mut self, text: QString) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let text = text.to_string();
        if self.draft.to_string() == text {
            return;
        }
        self.draft = QString::from(text.clone());
        self.draft_changed();

        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |failure: Option<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if let Some(message) = failure {
                this.borrow().error(message.into());
            }
        });

        runtime.spawn(async move {
            let result = if text.is_empty() {
                rpc.call::<_, serde_json::Value>("remove_draft", (account_id, chat_id))
                    .await
            } else {
                // `misc_set_draft` params: account, chat, text, file, filename, quote, view type
                // (not `misc_send_msg`'s). Pinned: deltachat-jsonrpc/tests/real_server.rs.
                // Text only: file, reply stay page's.
                rpc.call::<_, serde_json::Value>(
                    "misc_set_draft",
                    (
                        account_id,
                        chat_id,
                        Some(text),
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<u32>::None,
                        Option::<String>::None,
                    ),
                )
                .await
            };
            done(result.err().map(|err| err.to_string()));
        });
    }

    /// Reload whole chat.
    pub fn reload(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        // In-flight fills: generation drops replies; view asks again.
        self.generation = self.generation.wrapping_add(1);
        self.pending.clear();
        self.fetches = 0;
        self.set_hydrating(false);
        // Sync in flight: generation drops its read; re-read lands by id. Neither blocks next
        // event's, so one never answering (runtime gone) holds nothing.
        self.syncing = false;
        self.sync_again = false;
        self.refreshing.clear();
        self.refresh_again.clear();
        // Name: own small fetch (prefetch lacks it; page shows list's).
        self.refresh_name();
        // Read-mark asked before rows marked read.
        self.load_unread_mark();
        // Limit asked before file picked.
        self.load_attachment_limit();
        // Prefetched by opener: no round trip; transition starts with rows.
        if let Some((shape, entries, items)) = crate::prefetch::take(account_id, chat_id) {
            self.take_shape(shape);
            self.rows.borrow_mut().reset_data(rows_for(&entries, items));
            self.loaded = true;
            self.loaded_changed();
            self.rows_changed();
            if !self.reading_history {
                self.mark_chat_seen();
            }
            return;
        }

        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(
            move |result: Result<(ChatShape, Vec<Entry>, Vec<MessageListItem>), String>| {
                let Some(this) = ptr.as_pinned() else { return };
                match result {
                    Ok((shape, entries, items)) => {
                        {
                            let mut this_mut = this.borrow_mut();
                            this_mut.take_shape(shape);
                            this_mut
                                .rows
                                .borrow_mut()
                                .reset_data(rows_for(&entries, items));
                            this_mut.loaded = true;
                        }
                        this.borrow().loaded_changed();
                        this.borrow().rows_changed();
                        // Asked now: reader may scroll away, or app go behind.
                        let looking = !this.borrow().reading_history;
                        if looking {
                            this.borrow_mut().mark_chat_seen();
                        }
                    }
                    Err(err) => {
                        // Left false, view stays empty forever.
                        this.borrow_mut().loaded = true;
                        this.borrow().loaded_changed();
                        this.borrow().error(err.into());
                    }
                }
            },
        );

        runtime.spawn(async move {
            // Callback marks read: visible rows known only once landed.
            done(open_chat(&rpc, account_id, chat_id, 0).await);
        });
    }

    /// Take in chat kind; each fact announced alone (own binding).
    fn take_shape(&mut self, shape: ChatShape) {
        self.is_group = shape.is_group;
        self.can_send = shape.can_send;
        self.is_encrypted = shape.is_encrypted;
        self.can_call = shape.can_call;
        self.member_count = shape.member_count;
        self.is_contact_request = shape.is_contact_request;
        self.is_group_changed();
        self.can_send_changed();
        self.is_encrypted_changed();
        self.can_call_changed();
        self.member_count_changed();
        self.is_contact_request_changed();
    }

    /// Accept contact request; see `accept_chat` property.
    pub fn accept_chat(&mut self) {
        self.act_on_chat("accept_chat", AfterChatAct::Reshape);
    }

    /// Block request's sender; see `block_chat` property.
    pub fn block_chat(&mut self) {
        self.act_on_chat("block_chat", AfterChatAct::Close);
    }

    /// Delete chat here; see `delete_chat` property.
    pub fn delete_chat(&mut self) {
        self.act_on_chat("delete_chat", AfterChatAct::Close);
    }

    /// Whole-chat call (`method(account, chat)`); on success re-read shape or close page.
    /// Refusal reported, page stays.
    fn act_on_chat(&mut self, method: &'static str, after: AfterChatAct) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answer for a chat already left.
            if this.borrow().chat_id != chat_id {
                return;
            }
            match (result, after) {
                (Ok(()), AfterChatAct::Reshape) => this.borrow_mut().refresh_shape(),
                (Ok(()), AfterChatAct::Close) => this.borrow().closed(),
                (Err(err), _) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(method, (account_id, chat_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Reread chat shape (request accepted here or elsewhere: field, edit, calls follow).
    fn refresh_shape(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |shape: ChatShape| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answer for a chat already left.
            if this.borrow().chat_id != chat_id {
                return;
            }
            this.borrow_mut().take_shape(shape);
        });
        runtime.spawn(async move {
            done(chat_shape(&rpc, account_id, chat_id).await);
        });
    }

    /// Scroll target for `message_id`.
    pub fn reveal(&mut self, message_id: u32) {
        let row = self.row_of(message_id);
        self.revealed(message_id, row);
    }

    /// Fill rows `first..=last` plus `MARGIN` each side, skipping filled/asked. Up to
    /// `IN_FLIGHT` overlap; past that ask drops, page re-asks when one lands.
    pub fn hydrate(&mut self, first: i32, last: i32) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 || self.fetches >= IN_FLIGHT {
            return;
        }
        let wanted = self.unloaded_between(first, last);
        if wanted.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        self.pending.extend(wanted.iter().copied());
        self.fetches = self.fetches.saturating_add(1);
        self.set_hydrating(true);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let asked = wanted.clone();
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            if let Some(this) = ptr.as_pinned() {
                this.borrow_mut().hydrated(generation, &asked, result);
            }
        });

        runtime.spawn(async move {
            done(fetch_messages(&rpc, account_id, &wanted).await);
        });
    }

    /// Set hydrating flag on change.
    fn set_hydrating(&mut self, hydrating: bool) {
        if self.hydrating != hydrating {
            self.hydrating = hydrating;
            self.hydrating_changed();
        }
    }

    /// Unfetched row ids for view plus margin, max one page (fling would ask every row). On
    /// screen first, then margin nearest first (margin-first left screen bottom to next fetch).
    fn unloaded_between(&self, first: i32, last: i32) -> Vec<u32> {
        let rows = self.rows.borrow();
        let first = usize::try_from(first.max(0)).unwrap_or(0);
        let last = usize::try_from(last.max(0)).unwrap_or(0);
        fill_order(first, last, rows.iter().count())
            .into_iter()
            .map(|index| &rows[index])
            .filter(|item| !item.loaded && !self.pending.contains(&item.message_id))
            .map(|item| item.message_id)
            .take(PAGE)
            .collect()
    }

    /// `hydrate` rows came back: fill in place.
    fn hydrated(
        &mut self,
        generation: u64,
        asked: &[u32],
        result: Result<Vec<MessageListItem>, String>,
    ) {
        // Replaced by reload: drop; view asked again.
        if generation != self.generation {
            return;
        }
        for id in asked {
            self.pending.remove(id);
        }
        self.fetches = self.fetches.saturating_sub(1);
        let filled = result.map(|items| self.fill_rows(items).len());
        self.set_hydrating(self.fetches > 0);
        match filled {
            // In place: view keeps position, delegates.
            Ok(filled) if filled > 0 => self.rows_changed(),
            Ok(_) => {}
            Err(err) => self.error(err.into()),
        }
    }

    /// Fill fetched rows in place by id (rows move mid-fetch); returns landed. Map, not scan:
    /// 10k rows times fifty.
    fn fill_rows(&mut self, items: Vec<MessageListItem>) -> Vec<MessageListItem> {
        let mut rows = self.rows.borrow_mut();
        let index_of: BTreeMap<u32, usize> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.message_id, index))
            .collect();
        let mut landed = Vec::new();
        for item in items {
            let Some(index) = index_of.get(&item.message_id).copied() else {
                continue;
            };
            landed.push(item.clone());
            // Fill keeps order and count: map valid.
            rows.change_line(index, item);
        }
        landed
    }

    /// Apply core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.chat_id == 0 {
            return;
        }
        let kind = kind.to_string();
        let Some(payload) = json::chat_event(&payload_json.to_string(), self.chat_id) else {
            return;
        };

        match kind.as_str() {
            // New or changed content: sync what's lacking.
            "IncomingMsg" | "MsgDeleted" => self.sync_rows(),
            // Plus re-read: limit-held download and other devices' edits land as this event, ids
            // unchanged (row said "Downloading…" till reopen).
            "MsgsChanged" => {
                self.sync_rows();
                if let Some(message_id) = json::u32_opt(&payload, "msgId").filter(|id| *id != 0) {
                    self.refresh_one(message_id);
                }
            }
            // Dropped events: rows may be wrong, nothing will say. Restart.
            "EventChannelOverflow" => self.reload(),
            // Delivery state, reaction, webxdc status ("3 votes", shown even if app closed): no
            // id-list change; names its row.
            "MsgDelivered" | "MsgRead" | "MsgFailed" | "ReactionsChanged"
            | "WebxdcStatusUpdate" => {
                if let Some(message_id) = json::u32_opt(&payload, "msgId") {
                    self.refresh_one(message_id);
                }
            }
            // Group or 1:1 contact rename (core doesn't say whose).
            "ChatModified" | "ContactsChanged" => {
                self.refresh_name();
                // Join or leave: other half of header.
                self.refresh_member_count();
                // Request accepted on other device: field comes back here too.
                if self.is_contact_request {
                    self.refresh_shape();
                }
            }
            _ => {}
        }
    }

    /// Reread chat name (`get_basic_chat_info`). Silent if unchanged.
    fn refresh_name(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |name: Option<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            let Some(name) = name else { return };
            // Answer for a chat already left.
            if this.borrow().chat_id != chat_id {
                return;
            }
            if this.borrow().chat_name.to_string() != name {
                this.borrow_mut().chat_name = name.into();
                this.borrow().chat_name_changed();
            }
        });
        runtime.spawn(async move {
            let name = rpc
                .call::<_, serde_json::Value>("get_basic_chat_info", (account_id, chat_id))
                .await
                .ok()
                .map(|info| json::str_at(&info, "name").to_string());
            done(name);
        });
    }

    /// Reread group member count (known groups only: 1:1 can't become one). On join/leave.
    fn refresh_member_count(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 || !self.is_group {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |count: u32| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answer for a chat already left.
            if this.borrow().chat_id != chat_id {
                return;
            }
            if this.borrow().member_count != count {
                this.borrow_mut().member_count = count;
                this.borrow().member_count_changed();
            }
        });
        runtime.spawn(async move {
            done(member_count(&rpc, account_id, chat_id).await);
        });
    }

    /// Bring rows in line with chat's id list: arrivals and removals in place, reader stays.
    /// One read at a time ([`Self::syncing`]); event during read sets follow-up.
    fn sync_rows(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        if self.syncing {
            self.sync_again = true;
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.syncing = true;
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<Entry>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Pre-reload read: generation drops it.
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().syncing = false;
            let current = {
                let this = this.borrow();
                this.account_id == account_id && this.chat_id == chat_id
            };
            if current {
                match result {
                    Ok(entries) => this.borrow_mut().synced(&entries),
                    Err(err) => this.borrow().error(err.into()),
                }
            }
            // Flag taken after rows in: follow-up diffs vs them. Reorder reloaded.
            let again = std::mem::take(&mut this.borrow_mut().sync_again);
            if again {
                this.borrow_mut().sync_rows();
            }
        });

        runtime.spawn(async move {
            done(message_entries(&rpc, account_id, chat_id).await);
        });
    }

    /// Id list came back: remove gone rows, insert arrivals as placeholders under day. Not only
    /// at end: core sorts received below newest *seen*, so late group message lands among
    /// unread; reorder would restart model, losing reader's place.
    fn synced(&mut self, entries: &[Entry]) {
        let current: Vec<u32> = self
            .rows
            .borrow()
            .iter()
            .map(|item| item.message_id)
            .collect();
        match RowDiff::between(&current, &ids_of(entries)) {
            RowDiff::Same => {}
            RowDiff::Reordered => self.reload(),
            // Arrivals at end (common): append placeholders, no walk.
            RowDiff::Extended { arrived } => {
                {
                    let mut rows = self.rows.borrow_mut();
                    for entry in entries.get(current.len()..).unwrap_or_default() {
                        rows.push(placeholder(*entry));
                    }
                }
                self.rows_changed();
                self.absorb(arrived);
            }
            RowDiff::Changed { gone, arrived } => {
                self.remove_rows(&gone);
                place_arrivals(&mut self.rows.borrow_mut(), entries, &arrived);
                self.rows_changed();
                if !arrived.is_empty() {
                    self.absorb(arrived);
                }
            }
        }
    }

    /// Remove rows by index, backwards so indices stay valid.
    fn remove_rows(&mut self, gone: &[usize]) {
        let mut rows = self.rows.borrow_mut();
        for index in gone.iter().rev() {
            rows.remove(*index);
        }
    }

    /// Fetch arrived messages now, not on view ask: reader told (`arrived`); placeholder can't
    /// tell own from others'.
    fn absorb(&mut self, ids: Vec<u32>) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().chat_id != chat_id {
                return;
            }
            let Ok(items) = result else { return };
            // By id: rows may move; send's reply may have filled one.
            let landed = this.borrow_mut().fill_rows(items);
            if landed.is_empty() {
                return;
            }
            this.borrow().rows_changed();
            // After fill: reader gone mid-fetch not credited.
            let looking = !this.borrow().reading_history;
            let incoming = u32::try_from(landed.iter().filter(|item| !item.is_outgoing).count())
                .unwrap_or(u32::MAX);
            if looking {
                this.borrow().mark_items_seen(&landed);
            }
            if incoming > 0 {
                this.borrow().arrived(incoming);
            }
        });

        runtime.spawn(async move {
            done(fetch_messages(&rpc, account_id, &ids).await);
        });
    }

    /// Reread one message, replace row. One read per message at a time; one more if marked
    /// meanwhile (receipt bursts).
    fn refresh_one(&mut self, message_id: u32) {
        let account_id = self.account_id;
        if !self
            .rows
            .borrow()
            .iter()
            .any(|item| item.message_id == message_id)
        {
            return;
        }
        if self.refreshing.contains(&message_id) {
            self.refresh_again.insert(message_id);
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.refreshing.insert(message_id);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Marks are this generation's: reload cleared them.
            let again = {
                let mut this_mut = this.borrow_mut();
                if this_mut.generation == generation {
                    this_mut.refreshing.remove(&message_id);
                    this_mut.refresh_again.remove(&message_id)
                } else {
                    false
                }
            };
            // Follow-up started before answer applied: no return skips it.
            if again {
                this.borrow_mut().refresh_one(message_id);
            }
            // Message ids unique per account only.
            if this.borrow().account_id != account_id {
                return;
            }
            let Some(item) = result.ok().and_then(|items| items.into_iter().next()) else {
                return;
            };
            // Index re-found (reload may replace rows); `change_line` doesn't check bounds.
            let existing = this
                .borrow()
                .rows
                .borrow()
                .iter()
                .position(|row| row.message_id == item.message_id);
            let Some(index) = existing else { return };
            this.borrow_mut().rows.borrow_mut().change_line(index, item);
            this.borrow().rows_changed();
        });

        runtime.spawn(async move {
            let result = fetch_messages(&rpc, account_id, &[message_id]).await;
            done(result);
        });
    }

    /// Row of `message_id`, or -1 if not loaded.
    pub fn row_of(&self, message_id: u32) -> i32 {
        self.rows
            .borrow()
            .iter()
            .position(|row| row.message_id == message_id)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    }

    /// Mark every loaded unread message read.
    pub fn mark_seen_all(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let unseen = self.unseen_ids();
        if unseen.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            mark_seen(&rpc, account_id, unseen).await;
        });
    }

    /// Incoming unread rows owing receipts, read in place: cloning 10k rows ran on Qt thread per
    /// foreground and end-reach.
    fn unseen_ids(&self) -> Vec<u32> {
        self.rows
            .borrow()
            .iter()
            .filter(|item| is_unseen(item))
            .map(|item| item.message_id)
            .collect()
    }

    /// Send read receipts for these rows; items, not model: sync marks only what it added.
    fn mark_items_seen(&self, items: &[MessageListItem]) {
        let account_id = self.account_id;
        let unseen: Vec<u32> = items
            .iter()
            .filter(|item| is_unseen(item))
            .map(|item| item.message_id)
            .collect();
        if account_id == 0 || unseen.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            mark_seen(&rpc, account_id, unseen).await;
        });
    }

    /// Someone started or stopped looking. Entry-only marking failed: local fetch beats
    /// transition, sees `reading_history` true, skips; badge never cleared. Catches page
    /// settling and return to foreground.
    pub fn set_reading_history(&mut self, reading_history: bool) {
        if self.reading_history == reading_history {
            return;
        }
        self.reading_history = reading_history;
        self.reading_history_changed();
        if !reading_history {
            self.mark_chat_seen();
        }
    }

    /// Mark chat seen (`mark_seen_all` alone keeps badge); receipts for loaded rows.
    fn mark_chat_seen(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let unseen = self.unseen_ids();
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            if let Err(err) = rpc
                .call::<_, ()>("marknoticed_chat", (account_id, chat_id))
                .await
            {
                eprintln!("piirit: marknoticed_chat: {err}");
            }
            mark_seen(&rpc, account_id, unseen).await;
        });
    }

    /// Delete here and on mail server.
    pub fn delete_message(&mut self, message_id: u32) {
        self.remove_message("delete_messages", message_id);
    }

    /// Delete here and for everyone (peers' cores act on deletion message). Refusals reported
    /// as failed sends.
    pub fn delete_message_for_all(&mut self, message_id: u32) {
        self.remove_message("delete_messages_for_all", message_id);
    }

    /// Ask core to delete; take row out once it agrees, not on core's event: id-list read can't
    /// show *newest* message gone (row past every id = list too old, `RowDiff::between`), and
    /// own last message is likeliest. Refusal leaves row.
    fn remove_message(&mut self, method: &'static str, message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().drop_row(message_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            done(act_on_message(&rpc, method, account_id, message_id).await);
        });
    }

    /// Take one row out if still here; core's event then agrees.
    fn drop_row(&mut self, message_id: u32) {
        let index = self
            .rows
            .borrow()
            .iter()
            .position(|row| row.message_id == message_id);
        let Some(index) = index else {
            return;
        };
        self.remove_rows(&[index]);
        self.rows_changed();
    }

    /// Retry failed message.
    pub fn resend_message(&mut self, message_id: u32) {
        self.act("resend_messages", message_id);
    }

    /// Replace own message text. Not `act`: takes text, page awaits landing. Links cleaned as
    /// sent. Core refuses others', notices, no-text (menu offers none): failed send. Same text:
    /// no-op success.
    pub fn edit_message(&mut self, message_id: u32, text: QString) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let text = self.outgoing_text(text.to_string().trim());

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => {
                    // Re-read: `MsgsChanged` may come after `edited` clears field.
                    this.borrow_mut().refresh_one(message_id);
                    this.borrow().edited(message_id);
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            // send_edit_request params: account, message, new text.
            let result = rpc
                .call::<_, ()>("send_edit_request", (account_id, message_id, text))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Fetch rest of download-limited message (one id, not `act`). Row re-read on `MsgsChanged`
    /// once landed; return says only started.
    pub fn download_full(&mut self, message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                // InProgress now: show it.
                Ok(()) => this.borrow_mut().refresh_one(message_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("download_full_message", (account_id, message_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Copy message into another chat (needs destination, not `act`). Nothing refreshed: copy
    /// lands elsewhere.
    pub fn forward_to(&mut self, message_id: u32, chat_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if let Err(err) = result {
                this.borrow().error(err.into());
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("forward_messages", (account_id, vec![message_id], chat_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// React, or remove own reaction. Toggle from row: core sets whole list, same emoji gives
    /// empty. Not `act`. Row re-read on answer (instant tap) and `ReactionsChanged` (others').
    pub fn react(&mut self, message_id: u32, emoji: QString) {
        let account_id = self.account_id;
        let emoji = emoji.to_string().trim().to_string();
        if account_id == 0 || message_id == 0 || emoji.is_empty() {
            return;
        }
        let mine = self
            .rows
            .borrow()
            .iter()
            .find(|row| row.message_id == message_id)
            .map(|row| row.my_reaction.to_string())
            .unwrap_or_default();
        let reaction: Vec<String> = if mine == emoji {
            Vec::new()
        } else {
            vec![emoji]
        };
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().refresh_one(message_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            // send_reaction params: account, message, reactions; answers carrier id. Pinned:
            // deltachat-jsonrpc/tests/real_server.rs.
            let result = rpc
                .call::<_, serde_json::Value>("send_reaction", (account_id, message_id, reaction))
                .await
                .map(|_| ())
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Call `method(account, [message])`, re-read row. Resend leaves id list unchanged (sync
    /// fetches missing only): row would keep "Send again".
    fn act(&mut self, method: &'static str, message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => this.borrow_mut().refresh_one(message_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            done(act_on_message(&rpc, method, account_id, message_id).await);
        });
    }

    /// Send plain-text message.
    pub fn send(&mut self, text: QString) {
        self.send_message(self.outgoing_text(&text.to_string()), None);
    }

    /// Text as sent: links cleaned if asked.
    fn outgoing_text(&self, text: &str) -> String {
        if self.clean_links {
            links::clean_text(text)
        } else {
            text.to_string()
        }
    }

    /// Send message with file.
    ///
    /// Bar video goes shrunk, once done (page holds button meanwhile). Other video started here
    /// for reader to send when ready: no send goes out on its own later.
    pub fn send_file(&mut self, text: QString, file_path: QString) {
        let path = local_path(&file_path.to_string());
        if path.is_empty() {
            self.error(QString::from("no file to send"));
            return;
        }
        if self.sending {
            return;
        }
        // Not held to recommended size (page warns; relays often take more).
        let name = file_name_of(&path);
        let text = self.outgoing_text(&text.to_string());
        let ours = self.recode.as_ref().filter(|recode| recode.source == path);
        match ours {
            Some(recode) if recode.as_is => self.send_message(text, Some((path, name))),
            Some(recode) => {
                if let Some(made) = recode.made.clone() {
                    self.in_flight = Some(made.clone());
                    self.send_message(text, Some((made, video::recoded_name(&name))));
                }
            }
            None => match video::plan(&path, self.media_quality, self.recommended_limit()) {
                Some(target) => {
                    self.drop_recode();
                    self.start_recode(path, target);
                    self.weigh();
                }
                None => self.send_message(text, Some((path, name))),
            },
        }
    }

    /// Stop shrinking: file goes as is; kept as not-to-make recode so limit or quality change
    /// doesn't restart it.
    pub fn cancel_preparing(&mut self) {
        let Some(recode) = self.recode.as_mut() else {
            return;
        };
        recode.cancel.store(true, Ordering::Relaxed);
        if let Some(made) = recode.made.take() {
            if self.in_flight.as_deref() != Some(made.as_str()) {
                video::discard(&made);
            }
        }
        recode.as_is = true;
        self.set_preparing(false);
        self.weigh();
    }

    /// Send recording as voice message.
    pub fn send_voice(&mut self, file_path: QString) {
        let path = local_path(&file_path.to_string());
        if path.is_empty() {
            self.error(QString::from("no recording to send"));
            return;
        }
        self.send_message(String::new(), Some((path, String::new())));
    }

    /// The one send. `file`: path and shown name; empty name for voice.
    fn send_message(&mut self, text: String, file: Option<(String, String)>) {
        // One at a time; guarded here, not only by button: other ways in, model knows answer.
        if self.sending {
            return;
        }
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        // No chat yet: skip round trip for core's error.
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let quoted = self.quoted_message_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        self.sending = true;
        self.sending_changed();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<MessageListItem, String>| {
            if let Some(this) = ptr.as_pinned() {
                this.borrow_mut().sent_or_not(quoted, result);
            }
        });

        let outgoing = Outgoing { text, file, quoted };
        runtime.spawn(async move {
            done(outgoing.send(&rpc, account_id, chat_id).await);
        });
    }

    /// Core answered send: clear the way, put message on row or report failure.
    fn sent_or_not(&mut self, quoted: u32, result: Result<MessageListItem, String>) {
        // Both paths: failed send retryable.
        self.sending = false;
        self.sending_changed();
        // Core copied it or won't. Kept while on bar (retry); else removed.
        if let Some(made) = self.in_flight.take() {
            let held = self
                .recode
                .as_ref()
                .is_some_and(|recode| recode.made.as_deref() == Some(made.as_str()));
            if !held {
                video::discard(&made);
            }
        }
        match result {
            Ok(item) => self.place_sent(quoted, item),
            Err(err) => self.error(err.into()),
        }
    }

    /// Sent message goes at chat end, or fills row own event made first.
    fn place_sent(&mut self, quoted: u32, item: MessageListItem) {
        let message_id = item.message_id;
        {
            let mut rows = self.rows.borrow_mut();
            let existing = rows.iter().position(|row| row.message_id == message_id);
            if let Some(index) = existing {
                rows.change_line(index, item);
            } else {
                rows.push(item);
            }
        }
        self.rows_changed();
        // Cleared once gone: failed send keeps reply.
        if quoted != 0 {
            self.quoted_message_id = 0;
            self.quote_changed();
        }
        self.sent(message_id);
    }
}

impl Drop for ChatMessages {
    /// Drop this conversation's video being made or made; unsent.
    fn drop(&mut self) {
        if let Some(recode) = self.recode.take() {
            recode.cancel.store(true, Ordering::Relaxed);
            if let Some(made) = recode.made {
                if self.in_flight.as_deref() != Some(made.as_str()) {
                    video::discard(&made);
                }
            }
        }
    }
}

/// Video on bar, being made or made.
struct Recode {
    /// See `ChatMessages::next_recode`.
    id: u64,
    /// Picked file it is made from.
    source: String,
    /// Target it is made into.
    target: piirit_video::Target,
    /// Set to stop.
    cancel: Arc<AtomicBool>,
    /// Smaller file, once made.
    made: Option<String>,
    /// Goes as is: not shrunk, not smaller, or stopped.
    as_is: bool,
}

/// One message on its way to core.
struct Outgoing {
    /// Body; may be empty beside file.
    text: String,
    /// Path to attach and name shown; empty name for voice.
    file: Option<(String, String)>,
    /// Message replied to, or 0.
    quoted: u32,
}

impl Outgoing {
    /// Hand message to core, fetch the row it became.
    async fn send(
        self,
        rpc: &Arc<RpcClient>,
        account_id: u32,
        chat_id: u32,
    ) -> Result<MessageListItem, String> {
        // Two kinds named, rest typed from file. Empty name = voice (just sound to core). Picture
        // named so core recodes it: `misc_send_msg` keeps `File` original size, and media quality
        // is read only in that recode (`media.rs`).
        let viewtype = match &self.file {
            Some((_, name)) if name.is_empty() => Some("Voice"),
            Some((path, _)) => media::outgoing_viewtype(path),
            None => None,
        };
        if let Some(viewtype) = viewtype {
            self.send_as(rpc, account_id, chat_id, viewtype).await
        } else {
            self.send_text_or_file(rpc, account_id, chat_id).await
        }
    }

    /// Send with view type via `send_msg`.
    async fn send_as(
        self,
        rpc: &Arc<RpcClient>,
        account_id: u32,
        chat_id: u32,
        viewtype: &str,
    ) -> Result<MessageListItem, String> {
        let (path, name) = self.file.unzip();
        // send_msg params: account, chat, MessageData (camelCase, view type by variant name).
        // Pinned: deltachat-jsonrpc/tests/real_server.rs.
        let data = serde_json::json!({
            // Empty would be body ""; voice is named for nobody.
            "text": (!self.text.is_empty()).then_some(self.text),
            "file": path,
            "filename": name.filter(|name| !name.is_empty()),
            "viewtype": viewtype,
            "quotedMessageId": (self.quoted != 0).then_some(self.quoted),
        });
        match rpc
            .call::<_, u32>("send_msg", (account_id, chat_id, data))
            .await
        {
            Ok(message_id) => fetch_messages(rpc, account_id, &[message_id])
                .await
                .and_then(|items| {
                    items
                        .into_iter()
                        .next()
                        .ok_or_else(|| "the core lost the message it just sent".to_string())
                }),
            Err(err) => Err(err.to_string()),
        }
    }

    /// Text, file, or both, via `misc_send_msg`.
    async fn send_text_or_file(
        self,
        rpc: &Arc<RpcClient>,
        account_id: u32,
        chat_id: u32,
    ) -> Result<MessageListItem, String> {
        let (path, name) = self.file.unzip();
        // Caption-only is text; "" would be body "".
        let text = (!self.text.is_empty()).then_some(self.text);
        // misc_send_msg params: account, chat, text, file, filename, location, quote.
        // Pinned: deltachat-jsonrpc/tests/real_server.rs.
        let sent = rpc
            .call::<_, (u32, serde_json::Value)>(
                "misc_send_msg",
                (
                    account_id,
                    chat_id,
                    text,
                    path,
                    name,
                    Option::<(f64, f64)>::None,
                    (self.quoted != 0).then_some(self.quoted),
                ),
            )
            .await
            .map_err(|err| err.to_string());
        match sent {
            Ok((message_id, message)) => {
                let mut row = row_from(message_id, &message);
                // Sent .xdc an app on its row at once, not a file until reopen.
                with_extras(rpc, account_id, std::slice::from_mut(&mut row)).await;
                Ok(row)
            }
            Err(err) => Err(err),
        }
    }
}

/// Rows vs chat's current id list.
enum RowDiff {
    /// Nothing: rows are the list.
    Same,
    /// Moved: only a reload can fix.
    Reordered,
    /// Rows plus `arrived` after them: nothing gone, arrivals at end only.
    Extended { arrived: Vec<u32> },
    /// Gone rows by index; arrived ids in chat order.
    Changed { gone: Vec<usize>, arrived: Vec<u32> },
}

impl RowDiff {
    /// Compare row ids with chat's in order. Kept rows must keep chat order, else reorder;
    /// arrivals anywhere. Missing row with id past every listed id is newer, not gone: ids
    /// count up; send's reply adds row during older list read (else it vanished and returned).
    fn between(current: &[u32], ids: &[u32]) -> Self {
        if current == ids {
            return Self::Same;
        }
        // Common case first: one comparison, no set.
        if let Some(tail) = ids.strip_prefix(current) {
            return Self::Extended {
                arrived: tail.to_vec(),
            };
        }
        let present: HashSet<u32> = ids.iter().copied().collect();
        let newest = ids.iter().copied().max().unwrap_or(0);
        let kept: Vec<u32> = current
            .iter()
            .copied()
            .filter(|id| present.contains(id))
            .collect();
        if !in_order(&kept, ids) {
            return Self::Reordered;
        }
        let gone: Vec<usize> = current
            .iter()
            .enumerate()
            .filter(|(_, id)| !present.contains(id) && **id <= newest)
            .map(|(index, _)| index)
            .collect();
        // Set: per-id row scan is 10k squared per event.
        let held: HashSet<u32> = kept.iter().copied().collect();
        let arrived: Vec<u32> = ids
            .iter()
            .copied()
            .filter(|id| !held.contains(id))
            .collect();
        if gone.is_empty() && arrived.is_empty() {
            Self::Same
        } else {
            Self::Changed { gone, arrived }
        }
    }
}

/// `kept` is `ids` minus some: same order, nothing moved.
fn in_order(kept: &[u32], ids: &[u32]) -> bool {
    let mut kept = kept.iter();
    let mut next = kept.next();
    for id in ids {
        if next == Some(id) {
            next = kept.next();
        }
    }
    next.is_none()
}

/// Placeholder for each of `arrived` at its list position. Rows are list minus arrivals, in
/// order (`RowDiff` ensures), so each entry is arrival (after previous) or held row: at
/// walk, or past rows newer than list.
fn place_arrivals(rows: &mut MessageListModel, entries: &[Entry], arrived: &[u32]) {
    if arrived.is_empty() {
        return;
    }
    let arrived: HashSet<u32> = arrived.iter().copied().collect();
    let mut held = rows.iter().count();
    let mut at = 0;
    for entry in entries {
        if arrived.contains(&entry.message_id) {
            rows.insert(at, placeholder(*entry));
            held = held.saturating_add(1);
            at = at.saturating_add(1);
        } else if let Some(offset) =
            (at..held).position(|index| rows[index].message_id == entry.message_id)
        {
            at = at.saturating_add(offset).saturating_add(1);
        }
    }
}

/// Rows to fill for view `first..=last` of `count`: on-screen, then margin below/above
/// alternately, nearest first.
fn fill_order(first: usize, last: usize, count: usize) -> Vec<usize> {
    let Some(bottom) = count.checked_sub(1).filter(|_| first <= last) else {
        return Vec::new();
    };
    let last = last.min(bottom);
    let first = first.min(last);
    let mut order: Vec<usize> = (first..=last).collect();
    for distance in 1..=MARGIN {
        if let Some(below) = last.checked_add(distance).filter(|&below| below < count) {
            order.push(below);
        }
        if let Some(above) = first.checked_sub(distance) {
            order.push(above);
        }
    }
    order
}

/// Chat's messages, oldest first, under their day. Last arg asks core for day markers at
/// local midnight (not UTC; checked vs `deltachat-rpc-server`, three zones), as
/// `local_day_number` on fetched rows: day can't change on fill.
pub(crate) async fn message_entries(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
) -> Result<Vec<Entry>, String> {
    let items: Vec<serde_json::Value> = rpc
        .call("get_message_list_items", (account_id, chat_id, false, true))
        .await
        .map_err(|err| err.to_string())?;
    let mut entries = Vec::with_capacity(items.len());
    // Day from last marker: core emits one before each day's first message.
    let mut day_number = 0;
    for item in &items {
        match json::str_at(item, "kind") {
            "dayMarker" => {
                if let Some(timestamp) = item.get("timestamp").and_then(serde_json::Value::as_i64) {
                    day_number = local_day_number(timestamp);
                }
            }
            "message" => {
                // `rename_all` renames variants, not fields: snake_case on wire.
                // `msgId` too, in case.
                if let Some(message_id) =
                    json::u32_opt(item, "msg_id").or_else(|| json::u32_opt(item, "msgId"))
                {
                    entries.push(Entry {
                        message_id,
                        day_number,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(entries)
}

/// Call `method(account, [message])`: one-message action (delete, resend), for rows and
/// media pages.
pub(crate) async fn act_on_message(
    rpc: &RpcClient,
    method: &'static str,
    account_id: u32,
    message_id: u32,
) -> Result<(), String> {
    rpc.call::<_, ()>(method, (account_id, vec![message_id]))
        .await
        .map_err(|err| err.to_string())
}

/// Fetch several messages in one round trip.
pub(crate) async fn fetch_messages(
    rpc: &Arc<RpcClient>,
    account_id: u32,
    ids: &[u32],
) -> Result<Vec<MessageListItem>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let loaded: BTreeMap<u32, serde_json::Value> = rpc
        .call("get_messages", (account_id, ids))
        .await
        .map_err(|err| err.to_string())?;
    let mut rows: Vec<MessageListItem> = ids
        .iter()
        .filter_map(|id| {
            let message = loaded.get(id)?;
            // Core's `{kind: "loadingError"}`: skip, no blank row.
            if json::str_at(message, "kind") == "loadingError" {
                return None;
            }
            Some(row_from(*id, message))
        })
        .collect();
    with_extras(rpc, account_id, &mut rows).await;
    Ok(rows)
}

/// Extras round trips in flight. Calls/Apps page: fifty rows; serial = fifty trips before
/// anything shows, all at once queues ahead of other calls.
const EXTRAS_IN_FLIGHT: usize = 8;

/// Extra a row needs from core, for kinds that need one.
enum Extra {
    /// App row's drawing data.
    Webxdc(webxdc::Extras),
    /// Call state; nothing if core can't describe it.
    Call(Option<calls::Summary>),
}

/// Extras for app and call rows, one round trip each. App: name, icon, self-description.
/// Call: state, drawn instead of core's English sentence. Up to `EXTRAS_IN_FLIGHT`.
async fn with_extras(rpc: &Arc<RpcClient>, account_id: u32, rows: &mut [MessageListItem]) {
    let mut wanted: VecDeque<(usize, u32, bool)> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| match row.view_type.to_string().as_str() {
            "Webxdc" => Some((index, row.message_id, false)),
            "Call" => Some((index, row.message_id, true)),
            _ => None,
        })
        .collect();
    let mut asked = tokio::task::JoinSet::new();
    loop {
        while asked.len() < EXTRAS_IN_FLIGHT {
            let Some((index, message_id, is_call)) = wanted.pop_front() else {
                break;
            };
            let rpc = Arc::clone(rpc);
            asked.spawn(async move {
                let extra = if is_call {
                    Extra::Call(calls::summary(&rpc, account_id, message_id).await.ok())
                } else {
                    Extra::Webxdc(webxdc::extras(&rpc, account_id, message_id).await)
                };
                (index, extra)
            });
        }
        let Some(answered) = asked.join_next().await else {
            break;
        };
        // Task lost with runtime: row stays as built.
        if let Ok((index, extra)) = answered {
            if let Some(row) = rows.get_mut(index) {
                put_extra(row, extra);
            }
        }
    }
}

/// Put `with_extras` result on `row`.
fn put_extra(row: &mut MessageListItem, extra: Extra) {
    match extra {
        Extra::Webxdc(extras) => {
            row.webxdc_name = extras.name.into();
            row.webxdc_document = extras.document.into();
            row.webxdc_summary = extras.summary.into();
            row.webxdc_icon = extras.icon_path.into();
        }
        Extra::Call(Some(call)) => {
            row.call_state = call.state.into();
            row.call_has_video = call.has_video;
            row.call_duration = call.duration;
        }
        // Undescribed call draws as core's text.
        Extra::Call(None) => {}
    }
}

/// Incoming and not yet read by account.
fn is_unseen(item: &MessageListItem) -> bool {
    !item.is_outgoing && UNSEEN_STATES.contains(&item.state)
}

/// Mark read here and on other devices, sending read receipt (`marknoticed_chat` does
/// neither).
async fn mark_seen(rpc: &RpcClient, account_id: u32, unseen: Vec<u32>) {
    if unseen.is_empty() {
        return;
    }
    if let Err(err) = rpc
        .call::<_, ()>("markseen_msgs", (account_id, unseen))
        .await
    {
        eprintln!("piirit: markseen_msgs: {err}");
    }
}

/// Group, mailing list or broadcast (senders shown).
pub(crate) async fn chat_is_group(rpc: &RpcClient, account_id: u32, chat_id: u32) -> bool {
    basic_chat_info(rpc, account_id, chat_id)
        .await
        .is_some_and(|info| is_group_info(&info))
}

/// Core's `get_basic_chat_info`; nothing if undescribable.
async fn basic_chat_info(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
) -> Option<serde_json::Value> {
    rpc.call("get_basic_chat_info", (account_id, chat_id))
        .await
        .ok()
}

/// Basic info is a group, mailing list or broadcast.
fn is_group_info(info: &serde_json::Value) -> bool {
    !matches!(json::str_at(info, "chatType"), "Single" | "")
}

/// Chat facts beside messages: sender shown, writable, encrypted, calls, request.
#[derive(Clone, Copy, Default)]
// Five facts, each bound on its own; see `ChatMessages`.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ChatShape {
    /// Group, mailing list or broadcast.
    pub is_group: bool,
    /// Account can send (not if left group / unaccepted request).
    pub can_send: bool,
    /// End-to-end encrypted (all chatmail chats are).
    pub is_encrypted: bool,
    /// Call placeable; see `ChatMessages::can_call`.
    pub can_call: bool,
    /// Member count incl. reader, for group header; 0 elsewhere (1:1 too).
    pub member_count: u32,
    /// Unaccepted contact request.
    pub is_contact_request: bool,
}

/// What a whole-chat call leaves: same chat, new shape (accepted), or nothing (blocked, deleted).
#[derive(Clone, Copy)]
enum AfterChatAct {
    Reshape,
    Close,
}

/// Opening content: shape, messages by day, one page around `find_message_id`
/// (`opening_page`). Page waits on all, so shape asked beside messages.
pub(crate) async fn open_chat(
    rpc: &Arc<RpcClient>,
    account_id: u32,
    chat_id: u32,
    find_message_id: u32,
) -> Result<(ChatShape, Vec<Entry>, Vec<MessageListItem>), String> {
    let shape = {
        let rpc = Arc::clone(rpc);
        tokio::spawn(async move { chat_shape(&rpc, account_id, chat_id).await })
    };
    let messages = async {
        let entries = message_entries(rpc, account_id, chat_id).await?;
        // One page's content: ten thousand messages would stall Qt.
        let page = ids_of(opening_page(&entries, find_message_id));
        let items = fetch_messages(rpc, account_id, &page).await?;
        Ok::<_, String>((entries, items))
    }
    .await;
    match messages {
        // Shape unreadable (task lost): undescribable chat.
        Ok((entries, items)) => Ok((shape.await.unwrap_or_default(), entries, items)),
        Err(err) => {
            shape.abort();
            Err(err)
        }
    }
}

/// Chat shape: kind, encryption (`get_basic_chat_info`), `can_send` (asked beside), group
/// member count. Unknown = 1:1 taking nothing, offers least.
async fn chat_shape(rpc: &Arc<RpcClient>, account_id: u32, chat_id: u32) -> ChatShape {
    let can_send = {
        let rpc = Arc::clone(rpc);
        tokio::spawn(async move {
            rpc.call::<_, bool>("can_send", (account_id, chat_id))
                .await
                .unwrap_or(false)
        })
    };
    let Some(info) = basic_chat_info(rpc, account_id, chat_id).await else {
        can_send.abort();
        return ChatShape::default();
    };
    let is_group = is_group_info(&info);
    let is_encrypted = json::flag(&info, "isEncrypted");
    let member_count = if is_group {
        member_count(rpc, account_id, chat_id).await
    } else {
        0
    };
    let can_send = can_send.await.unwrap_or(false);
    ChatShape {
        is_group,
        can_send,
        is_encrypted,
        can_call: takes_calls(&info, is_group, can_send),
        member_count,
        is_contact_request: json::flag(&info, "isContactRequest"),
    }
}

/// Call placeable, from core flags (basic and full chat info). `can_send` rules out device
/// chat and contact request; rest pre-asks core's refusal.
pub(crate) fn takes_calls(info: &serde_json::Value, is_group: bool, can_send: bool) -> bool {
    !is_group
        && can_send
        && json::flag(info, "isEncrypted")
        && !json::flag(info, "isSelfTalk")
        && !json::flag(info, "isDeviceChat")
}

/// Member count, 0 if unlisted (`get_chat_contacts` ids; members are `chat_info.rs`'s).
pub(crate) async fn member_count(rpc: &RpcClient, account_id: u32, chat_id: u32) -> u32 {
    rpc.call::<_, Vec<u32>>("get_chat_contacts", (account_id, chat_id))
        .await
        .ok()
        .and_then(|ids| u32::try_from(ids.len()).ok())
        .unwrap_or(0)
}

/// Days since epoch in viewer's timezone, offset *at that instant*: current one put
/// DST-crossing messages an hour out. Unplaceable timestamp: UTC, not missing list.
#[must_use]
pub fn local_day_number(timestamp: i64) -> i64 {
    let Some(when) = Local.timestamp_opt(timestamp, 0).single() else {
        return timestamp.div_euclid(86_400);
    };
    // Whole days: zone behind UTC doesn't borrow a day as (ts + offset) / 86400 would.
    when.date_naive()
        .signed_duration_since(DateTime::UNIX_EPOCH.date_naive())
        .num_days()
}

/// Placeholder for unfetched entry, one per id-list message: row 0 stays first.
fn placeholder(entry: Entry) -> MessageListItem {
    MessageListItem {
        message_id: entry.message_id,
        // Day known before message: heading right, never resizes.
        day_number: entry.day_number,
        loaded: false,
        ..MessageListItem::default()
    }
}

/// One row from core's message object.
fn row_from(message_id: u32, message: &serde_json::Value) -> MessageListItem {
    let timestamp = json::i64_at(message, "timestamp");
    let sender_name = match json::str_at(message, "overrideSenderName") {
        "" => json::text(message, "/sender/displayName"),
        name => name.into(),
    };
    let text = json::str_at(message, "text");
    let is_outgoing = json::u32_opt(message, "fromId") == Some(1);
    let state = json::u32_at(message, "state");
    let file_path = json::str_at(message, "file");
    let view_type = json::str_at(message, "viewType");
    // Core gives 0x0 for GIF (probes PNG/JPEG only): read header so row height holds on
    // decode.
    let (image_width, image_height) = match (
        json::i32_at(message, "dimensionsWidth"),
        json::i32_at(message, "dimensionsHeight"),
    ) {
        (0, 0) if view_type == "Gif" => gif_dimensions(file_path).unwrap_or((0, 0)),
        // Core doesn't probe video, few clients send size: track header gives real shape, not
        // 16:9 guess.
        (0, 0) if view_type == "Video" => video_dimensions(file_path).unwrap_or((0, 0)),
        known => known,
    };
    MessageListItem {
        message_id,
        loaded: true,
        text: text.into(),
        // Rendered per fetch: switch rendered/raw needs no round trip.
        styled_text: markdown::render(text).into(),
        is_edited: json::flag(message, "isEdited"),
        // Absent from send's short object; composed messages never held back.
        download_state: json::text(message, "/downloadState"),
        // Core marks cut messages both ways, own long sends too.
        has_html: json::flag(message, "hasHtml"),
        // Contact 1 is DC_CONTACT_ID_SELF.
        is_outgoing,
        timestamp,
        day_number: local_day_number(timestamp),
        show_padlock: json::flag(message, "showPadlock"),
        state,
        // Set at build, rebuilt on change; playing GIF keeps its runs, unscrolled-to one still new.
        is_new: !is_outgoing && UNSEEN_STATES.contains(&state),
        sender_name,
        sender_color: json::text(message, "/sender/color"),
        is_info: json::flag(message, "isInfo"),
        // Absent from fake core's short `misc_send_msg` object; composed isn't forward. Real
        // forwards via `get_messages`.
        is_forwarded: json::flag(message, "isForwarded"),
        quote_text: json::text(message, "/quote/text"),
        quote_author: json::text(message, "/quote/authorDisplayName"),
        file_path: file_path.into(),
        file_name: json::text(message, "/fileName"),
        view_type: view_type.into(),
        // 0 if unsized: nothing may divide by these.
        image_width,
        image_height,
        file_mime: json::text(message, "/fileMime"),
        file_bytes: message
            .get("fileBytes")
            .and_then(serde_json::Value::as_u64)
            .map_or(0.0, |bytes| {
                // f64 (QML has no 64-bit int); saturates at 4 GiB, exact to 2^53.
                f64::from(u32::try_from(bytes).unwrap_or(u32::MAX))
            }),
        duration_ms: json::i32_at(message, "duration"),
        vcard_name: json::text(message, "/vcardContact/displayName"),
        vcard_addr: json::text(message, "/vcardContact/addr"),
        vcard_color: json::text(message, "/vcardContact/color"),
        reactions: reactions_json(message).into(),
        my_reaction: own_reaction(message).into(),
        // Filled by `with_extras` for app rows (another call).
        ..MessageListItem::default()
    }
}

/// GIF size from screen descriptor: two LE u16 after 6-byte signature; `None` if not GIF.
fn gif_dimensions(path: &str) -> Option<(i32, i32)> {
    use std::io::Read;
    if path.is_empty() {
        return None;
    }
    let mut header = [0_u8; 10];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    if &header[..4] != b"GIF8" {
        return None;
    }
    let width = i32::from(u16::from_le_bytes([header[6], header[7]]));
    let height = i32::from(u16::from_le_bytes([header[8], header[9]]));
    (width > 0 && height > 0).then_some((width, height))
}

/// Video size from MP4 `tkhd`, swapped if matrix turns it upright (phones store upright
/// video sideways). Box headers only (`mdat` skipped by size): movie header before or after
/// media. `None` if missing, not ISO media (MP4, MOV, 3GP), or no video track.
fn video_dimensions(path: &str) -> Option<(i32, i32)> {
    if path.is_empty() {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    // `box_header` keeps boxes inside file: sums stay far from u64 top.
    let mut at = 0_u64;
    let mut first = true;
    while at.saturating_add(8) <= len {
        let (size, kind) = mp4::box_header(&mut file, at, len)?;
        if first && &kind != b"ftyp" {
            return None;
        }
        first = false;
        if &kind == b"moov" {
            return mp4::video_track_size(&mut file, at.saturating_add(8), at.saturating_add(size));
        }
        at = at.saturating_add(size);
    }
    None
}

/// ISO media layout walked by `video_dimensions`.
mod mp4 {
    use std::io::{Read, Seek, SeekFrom};

    /// Box at `at`: size with header, type; `None` if impossible.
    pub(super) fn box_header(
        file: &mut std::fs::File,
        at: u64,
        end: u64,
    ) -> Option<(u64, [u8; 4])> {
        let mut header = [0_u8; 8];
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(&mut header).ok()?;
        let kind = [header[4], header[5], header[6], header[7]];
        let size = match u32::from_be_bytes([header[0], header[1], header[2], header[3]]) {
            // To end of file.
            0 => end.checked_sub(at)?,
            // 64-bit size follows type.
            1 => {
                let mut large = [0_u8; 8];
                file.read_exact(&mut large).ok()?;
                u64::from_be_bytes(large)
            }
            size => u64::from(size),
        };
        // Checked: wrapped size would walk back to start forever.
        (size >= 8 && at.checked_add(size).is_some_and(|stop| stop <= end)).then_some((size, kind))
    }

    /// First sized track in `from..to`: audio tracks have none.
    pub(super) fn video_track_size(
        file: &mut std::fs::File,
        from: u64,
        to: u64,
    ) -> Option<(i32, i32)> {
        let mut at = from;
        while at.saturating_add(8) <= to {
            let (size, kind) = box_header(file, at, to)?;
            if &kind == b"trak" {
                if let Some(found) = track_size(file, at.saturating_add(8), at.saturating_add(size))
                {
                    return Some(found);
                }
            }
            at = at.saturating_add(size);
        }
        None
    }

    /// Size `tkhd` gives, turned per matrix.
    fn track_size(file: &mut std::fs::File, from: u64, to: u64) -> Option<(i32, i32)> {
        let mut at = from;
        while at.saturating_add(8) <= to {
            let (size, kind) = box_header(file, at, to)?;
            if &kind == b"tkhd" {
                return header_size(file, at.saturating_add(8), size.saturating_sub(8));
            }
            at = at.saturating_add(size);
        }
        None
    }

    /// Width, height from `tkhd`, swapped for quarter turn. Version 1 has 64-bit times. Matrix:
    /// nine 16.16; quarter turn has zeros where identity has ones.
    fn header_size(file: &mut std::fs::File, at: u64, size: u64) -> Option<(i32, i32)> {
        // Version 1 fields reach 96 bytes, size at end.
        let mut body = [0_u8; 96];
        let wanted = usize::try_from(size.min(96)).ok()?;
        let body = body.get_mut(..wanted)?;
        file.seek(SeekFrom::Start(at)).ok()?;
        file.read_exact(body).ok()?;
        // Matrix, then width and height: eleven numbers.
        let (start, stop) = if body.first() == Some(&1) {
            (52, 96)
        } else {
            (40, 84)
        };
        let fields = body.get(start..stop)?;
        let mut numbers = [0_i32; 11];
        for (number, field) in numbers.iter_mut().zip(fields.chunks_exact(4)) {
            *number = i32::from_be_bytes(field.try_into().ok()?);
        }
        let [a, _, _, _, d, _, _, _, _, width, height] = numbers;
        let (width, height) = (width >> 16, height >> 16);
        if width <= 0 || height <= 0 {
            return None;
        }
        if a == 0 && d == 0 {
            Some((height, width))
        } else {
            Some((width, height))
        }
    }
}

/// Reactions for row: core's counted `reactions.reactions`, most first, order kept,
/// per-contact breakdown dropped. Null/absent/empty = none. Emoji unchecked: plain text.
fn reactions_json(message: &serde_json::Value) -> String {
    let Some(list) = message
        .pointer("/reactions/reactions")
        .and_then(serde_json::Value::as_array)
    else {
        return String::new();
    };
    let chips: Vec<serde_json::Value> = list
        .iter()
        .filter_map(|reaction| {
            let emoji = json::str_at(reaction, "emoji");
            if emoji.is_empty() {
                return None;
            }
            Some(serde_json::json!({
                "emoji": emoji,
                // Never 0: listed only if someone sent it.
                "count": json::u32_at(reaction, "count").max(1),
                "self": json::flag(reaction, "isFromSelf"),
            }))
        })
        .collect();
    if chips.is_empty() {
        String::new()
    } else {
        serde_json::Value::Array(chips).to_string()
    }
}

/// Own reaction on a message; empty if none.
fn own_reaction(message: &serde_json::Value) -> String {
    message
        .pointer("/reactions/reactions")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find(|reaction| json::flag(reaction, "isFromSelf"))
        .map(|reaction| json::str_at(reaction, "emoji").to_string())
        .unwrap_or_default()
}

/// Local path from picker: Silica `filePath` plain; `selectedContent`, `url` give
/// percent-encoded `file://` URLs. Core takes a path.
pub(crate) fn local_path(raw: &str) -> String {
    let path = raw.strip_prefix("file://").unwrap_or(raw);
    if !path.contains('%') {
        return path.to_string();
    }
    let mut out = Vec::with_capacity(path.len());
    let mut rest = path.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        // Incomplete escape is literal '%' ("100%.png" is real).
        // `from_str_radix` accepts a sign: check digits.
        let escape = match tail {
            [high, low, after @ ..]
                if byte == b'%' && high.is_ascii_hexdigit() && low.is_ascii_hexdigit() =>
            {
                std::str::from_utf8(&[*high, *low])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                    .map(|decoded| (decoded, after))
            }
            _ => None,
        };
        if let Some((decoded, after)) = escape {
            out.push(decoded);
            rest = after;
        } else {
            out.push(byte);
            rest = tail;
        }
    }
    // Not UTF-8: keep undecoded string.
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

/// Name recipient sees; core would use path (gallery: camera serial). Pictures get core's
/// dated name (it recodes them).
fn file_name_of(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        file_name_of, fill_order, gif_dimensions, local_path, own_reaction, place_arrivals,
        reactions_json, video_dimensions, Entry, RowDiff, MARGIN,
    };
    use crate::models::MessageListModel;
    use serde_json::json;

    /// ISO media box: size, type, body.
    fn mp4_box(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(
            &u32::try_from(body.len() + 8)
                .expect("box size")
                .to_be_bytes(),
        );
        out.extend_from_slice(&kind);
        out.extend_from_slice(body);
        out
    }

    /// `tkhd` body, version 0: size and matrix, identity or quarter turn.
    fn tkhd(width: u32, height: u32, turned: bool) -> Vec<u8> {
        let mut body = vec![0_u8; 40];
        // Version 0, flags, times, etc. all zero.
        let one = 0x0001_0000_u32.to_be_bytes();
        let (a, b, c, d) = if turned {
            ([0; 4], one, (-0x0001_0000_i32).to_be_bytes(), [0; 4])
        } else {
            (one, [0; 4], [0; 4], one)
        };
        for part in [
            a,
            b,
            [0; 4],
            c,
            d,
            [0; 4],
            [0; 4],
            [0; 4],
            0x4000_0000_u32.to_be_bytes(),
        ] {
            body.extend_from_slice(&part);
        }
        body.extend_from_slice(&(width << 16).to_be_bytes());
        body.extend_from_slice(&(height << 16).to_be_bytes());
        body
    }

    #[test]
    fn a_videos_size_is_read_off_its_track_header_turned_as_the_matrix_says() {
        let dir = std::env::temp_dir().join(format!("piirit-mp4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let ftyp = mp4_box(*b"ftyp", b"isom\0\0\0\0isomiso2mp41");
        let mdat = mp4_box(*b"mdat", &[0_u8; 64]);
        let audio = mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(0, 0, false)));

        // Movie header after media, as camera writes: size found.
        let landscape = mp4_box(
            *b"moov",
            &[
                audio.clone(),
                mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(1920, 1080, false))),
            ]
            .concat(),
        );
        let plain = dir.join("landscape.mp4");
        std::fs::write(&plain, [ftyp.clone(), mdat.clone(), landscape].concat()).expect("write");
        assert_eq!(
            video_dimensions(&plain.to_string_lossy()),
            Some((1920, 1080))
        );

        // Upright: stored sideways, turn in matrix.
        let upright = mp4_box(
            *b"moov",
            &mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(1920, 1080, true))),
        );
        let turned = dir.join("upright.mp4");
        std::fs::write(&turned, [ftyp.clone(), upright, mdat].concat()).expect("write");
        assert_eq!(
            video_dimensions(&turned.to_string_lossy()),
            Some((1080, 1920))
        );

        // Media box with 64-bit size: stepped over like any other.
        let mut large = Vec::new();
        large.extend_from_slice(&1_u32.to_be_bytes());
        large.extend_from_slice(b"mdat");
        large.extend_from_slice(&(16_u64 + 64).to_be_bytes());
        large.extend_from_slice(&[0_u8; 64]);
        let big = dir.join("large.mp4");
        std::fs::write(
            &big,
            [
                ftyp.clone(),
                large,
                mp4_box(
                    *b"moov",
                    &mp4_box(*b"trak", &mp4_box(*b"tkhd", &tkhd(640, 480, false))),
                ),
            ]
            .concat(),
        )
        .expect("write");
        assert_eq!(video_dimensions(&big.to_string_lossy()), Some((640, 480)));

        // Not there, not a movie, no video track: nothing, no guess.
        let gif = dir.join("clip.gif");
        std::fs::write(&gif, b"GIF89a\x2c\x01\x02\x00rest").expect("write");
        assert_eq!(video_dimensions(&gif.to_string_lossy()), None);
        let sound = dir.join("sound.mp4");
        std::fs::write(&sound, [ftyp, mp4_box(*b"moov", &audio)].concat()).expect("write");
        assert_eq!(video_dimensions(&sound.to_string_lossy()), None);
        assert_eq!(
            video_dimensions(&dir.join("missing.mp4").to_string_lossy()),
            None
        );
        assert_eq!(video_dimensions(""), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_box_whose_size_wraps_past_the_end_ends_the_walk() {
        // 64-bit size wrapping to 0 walked back forever (release), overflowed (debug).
        let dir = std::env::temp_dir().join(format!("piirit-mp4-wrap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let ftyp = mp4_box(*b"ftyp", b"isom\0\0\0\0isomiso2mp41");
        let at = u64::try_from(ftyp.len()).expect("ftyp size");
        let mut wraps = Vec::new();
        wraps.extend_from_slice(&1_u32.to_be_bytes());
        wraps.extend_from_slice(b"free");
        wraps.extend_from_slice(&0_u64.wrapping_sub(at).to_be_bytes());
        let path = dir.join("wraps.mp4");
        std::fs::write(&path, [ftyp, wraps].concat()).expect("write");
        assert_eq!(video_dimensions(&path.to_string_lossy()), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_gifs_size_is_read_off_its_header_and_nothing_elses_is() {
        let dir = std::env::temp_dir().join(format!("piirit-gif-dims-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        // 300 wide, 2 high (little-endian).
        let gif = dir.join("wide.gif");
        std::fs::write(&gif, b"GIF89a\x2c\x01\x02\x00\x00\x00\x00;").expect("write gif");
        let png = dir.join("dot.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR").expect("write png");
        let short = dir.join("short.gif");
        std::fs::write(&short, b"GIF8").expect("write short");

        assert_eq!(gif_dimensions(&gif.to_string_lossy()), Some((300, 2)));
        assert_eq!(gif_dimensions(&png.to_string_lossy()), None);
        assert_eq!(gif_dimensions(&short.to_string_lossy()), None);
        assert_eq!(gif_dimensions(""), None);
        assert_eq!(
            gif_dimensions(&dir.join("missing.gif").to_string_lossy()),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reactions_keep_the_cores_order_and_say_which_is_ours() {
        let message = json!({
            "reactions": {
                "reactions": [
                    {"emoji": "👍", "count": 2, "isFromSelf": false},
                    {"emoji": "❤️", "count": 1, "isFromSelf": true},
                ],
                "reactionsByContact": {"1": ["❤️"], "10": ["👍"], "11": ["👍"]},
            }
        });
        assert_eq!(
            reactions_json(&message),
            r#"[{"count":2,"emoji":"👍","self":false},{"count":1,"emoji":"❤️","self":true}]"#
        );
        assert_eq!(own_reaction(&message), "❤️");
    }

    #[test]
    fn no_reactions_read_as_nothing_however_the_core_says_it() {
        for message in [
            json!({}),
            json!({"reactions": null}),
            json!({"reactions": {"reactions": [], "reactionsByContact": {}}}),
            // Reaction with no emoji: nothing to draw.
            json!({"reactions": {"reactions": [{"emoji": "", "count": 1}]}}),
        ] {
            assert_eq!(reactions_json(&message), "", "{message}");
            assert_eq!(own_reaction(&message), "", "{message}");
        }
        // A missing count is still one person's reaction.
        let counted = json!({"reactions": {"reactions": [{"emoji": "🙏"}]}});
        assert_eq!(
            reactions_json(&counted),
            r#"[{"count":1,"emoji":"🙏","self":false}]"#
        );
    }

    #[test]
    fn a_picked_path_survives_whichever_way_it_arrives() {
        assert_eq!(local_path("/home/user/a.png"), "/home/user/a.png");
        assert_eq!(local_path("file:///home/user/a.png"), "/home/user/a.png");
        assert_eq!(
            local_path("file:///home/user/holiday%20photo.png"),
            "/home/user/holiday photo.png"
        );
        // Multi-byte chars are percent-encoded per byte.
        assert_eq!(
            local_path("file:///home/user/caf%C3%A9.png"),
            "/home/user/café.png"
        );
    }

    #[test]
    fn a_literal_percent_is_not_an_escape() {
        assert_eq!(local_path("/home/user/100%.png"), "/home/user/100%.png");
        assert_eq!(local_path("/home/user/%zz.png"), "/home/user/%zz.png");
        // Truncated at end: nothing to decode.
        assert_eq!(local_path("/home/user/x%2"), "/home/user/x%2");
        // Sign would decode to 0x0a with from_str_radix.
        assert_eq!(local_path("/home/user/%+a.png"), "/home/user/%+a.png");
    }

    #[test]
    fn the_name_is_the_last_component() {
        assert_eq!(file_name_of("/home/user/a.png"), "a.png");
        assert_eq!(file_name_of("a.png"), "a.png");
        assert_eq!(file_name_of("/home/user/"), "user");
        assert_eq!(file_name_of(""), "");
    }

    #[test]
    fn a_row_diff_tells_arrivals_and_departures_from_a_reorder() {
        assert!(matches!(
            RowDiff::between(&[1, 2, 3], &[1, 2, 3]),
            RowDiff::Same
        ));
        // One gone from front, two arrived at end: both in place.
        match RowDiff::between(&[1, 2, 3], &[2, 3, 4, 5]) {
            RowDiff::Changed { gone, arrived } => {
                assert_eq!(gone, vec![0]);
                assert_eq!(arrived, vec![4, 5]);
            }
            _ => panic!("gone and arrived should be a change"),
        }
        // Same ids, other order: nothing in place.
        assert!(matches!(
            RowDiff::between(&[1, 2, 3], &[2, 1, 3]),
            RowDiff::Reordered
        ));
    }

    #[test]
    fn a_message_sorted_into_the_middle_is_an_arrival_not_a_reorder() {
        // Late message before last: still chat order, so arrival.
        match RowDiff::between(&[1, 2, 3], &[1, 2, 7, 3]) {
            RowDiff::Changed { gone, arrived } => {
                assert_eq!(gone, Vec::<usize>::new());
                assert_eq!(arrived, vec![7]);
            }
            _ => panic!("a message sorted into the middle should be an arrival"),
        }
        // Arrived in chat order, wherever they landed.
        match RowDiff::between(&[2, 4], &[1, 2, 3, 4, 5]) {
            RowDiff::Changed { gone, arrived } => {
                assert_eq!(gone, Vec::<usize>::new());
                assert_eq!(arrived, vec![1, 3, 5]);
            }
            _ => panic!("arrivals around the held rows should be a change"),
        }
    }

    #[test]
    fn a_row_newer_than_the_list_is_not_gone() {
        // Send reply landed before list was read.
        assert!(matches!(
            RowDiff::between(&[1, 2, 3, 9], &[1, 2, 3]),
            RowDiff::Same
        ));
        // Kept while what list knows is taken in.
        match RowDiff::between(&[1, 2, 3, 9], &[2, 3, 7]) {
            RowDiff::Changed { gone, arrived } => {
                assert_eq!(gone, vec![0]);
                assert_eq!(arrived, vec![7]);
            }
            _ => panic!("a departure beside a newer row should be a change"),
        }
        // Only past the whole list: a row it lacks is gone.
        match RowDiff::between(&[1, 2, 3, 9], &[1, 3, 10]) {
            RowDiff::Changed { gone, arrived } => {
                assert_eq!(gone, vec![1, 3]);
                assert_eq!(arrived, vec![10]);
            }
            _ => panic!("a deletion under a newer list should be a change"),
        }
    }

    #[test]
    fn arrivals_only_at_the_end_are_an_extension() {
        // Common case, answered without sets.
        match RowDiff::between(&[1, 2, 3], &[1, 2, 3, 4, 5]) {
            RowDiff::Extended { arrived } => assert_eq!(arrived, vec![4, 5]),
            _ => panic!("arrivals at the end should be an extension"),
        }
        // From no rows.
        match RowDiff::between(&[], &[1, 2]) {
            RowDiff::Extended { arrived } => assert_eq!(arrived, vec![1, 2]),
            _ => panic!("arrivals into no rows should be an extension"),
        }
        // Not with a departure, nor an arrival not at end.
        assert!(matches!(
            RowDiff::between(&[1, 2, 3], &[2, 3, 4]),
            RowDiff::Changed { .. }
        ));
        assert!(matches!(
            RowDiff::between(&[1, 2, 3], &[1, 2, 7, 3, 8]),
            RowDiff::Changed { .. }
        ));
        // Nor past a newer row: it keeps its place; arrival goes before.
        match RowDiff::between(&[1, 2, 3, 9], &[1, 2, 3, 4]) {
            RowDiff::Changed { gone, arrived } => {
                assert_eq!(gone, Vec::<usize>::new());
                assert_eq!(arrived, vec![4]);
            }
            _ => panic!("an arrival before a newer row should be a change"),
        }
        // Nothing is still nothing.
        assert!(matches!(RowDiff::between(&[], &[]), RowDiff::Same));
    }

    fn entries(ids: &[u32]) -> Vec<Entry> {
        ids.iter()
            .map(|id| Entry {
                message_id: *id,
                day_number: 19_000 + i64::from(*id),
            })
            .collect()
    }

    fn rows_of(ids: &[u32]) -> MessageListModel {
        entries(ids).into_iter().map(super::placeholder).collect()
    }

    fn ids_in(rows: &MessageListModel) -> Vec<u32> {
        rows.iter().map(|row| row.message_id).collect()
    }

    #[test]
    fn arrivals_are_placed_where_the_chat_has_them() {
        // At end, as most are.
        let mut rows = rows_of(&[1, 2, 3]);
        place_arrivals(&mut rows, &entries(&[1, 2, 3, 4, 5]), &[4, 5]);
        assert_eq!(ids_in(&rows), vec![1, 2, 3, 4, 5]);

        // In middle, as late ones are; under its day from start.
        let mut rows = rows_of(&[1, 2, 3]);
        place_arrivals(&mut rows, &entries(&[1, 2, 7, 3]), &[7]);
        assert_eq!(ids_in(&rows), vec![1, 2, 7, 3]);
        assert_eq!(rows[2].day_number, 19_007);
        assert!(!rows[2].loaded);

        // Front, around, and after taken-out.
        let mut rows = rows_of(&[2, 4]);
        place_arrivals(&mut rows, &entries(&[1, 2, 3, 4, 5]), &[1, 3, 5]);
        assert_eq!(ids_in(&rows), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_row_newer_than_the_list_keeps_its_place() {
        // 9: a send the list lacks yet; arrival goes after entry before it.
        let mut rows = rows_of(&[1, 2, 3, 9]);
        place_arrivals(&mut rows, &entries(&[1, 2, 3, 7, 8]), &[7, 8]);
        assert_eq!(ids_in(&rows), vec![1, 2, 3, 7, 8, 9]);

        // Mid-list row stays when an older in-flight list lands after.
        let mut rows = rows_of(&[1, 2, 9, 3]);
        place_arrivals(&mut rows, &entries(&[1, 2, 3, 4]), &[4]);
        assert_eq!(ids_in(&rows), vec![1, 2, 9, 3, 4]);
    }

    #[test]
    fn the_screen_is_filled_before_the_margin_and_the_margin_nearest_first() {
        // Window with a whole margin either side.
        let (first, last, count) = (2 * MARGIN, 2 * MARGIN + 4, 5 * MARGIN);
        let order = fill_order(first, last, count);
        assert_eq!(&order[..5], &[first, first + 1, first + 2, first + 3, last]);
        assert_eq!(&order[5..9], &[last + 1, first - 1, last + 2, first - 2]);
        assert_eq!(order.len(), 5 + 2 * MARGIN);
        assert_eq!(order.iter().max(), Some(&(last + MARGIN)));
        assert_eq!(order.iter().min(), Some(&(first - MARGIN)));

        // Against either end, margin is only what's there.
        let top = fill_order(0, 4, count);
        assert_eq!(&top[..6], &[0, 1, 2, 3, 4, 5]);
        assert_eq!(top.len(), 5 + MARGIN);
        let bottom = fill_order(count - 5, count - 1, count);
        assert_eq!(
            &bottom[..6],
            &[
                count - 5,
                count - 4,
                count - 3,
                count - 2,
                count - 1,
                count - 6
            ]
        );
        assert_eq!(bottom.len(), 5 + MARGIN);

        // View past end, no rows.
        assert_eq!(fill_order(count + 20, count + 30, count)[0], count - 1);
        assert_eq!(fill_order(0, 0, 0), Vec::<usize>::new());
        assert_eq!(fill_order(5, 2, count), Vec::<usize>::new());
    }
}
