//! Fake `deltachat-rpc-server`: records requests.
//!
//! Each request appended as one JSON line to `PIIRIT_FAKE_JOURNAL`: tests assert which calls a
//! UI action makes, order, params. `get_next_event_batch` left out: polled in loop, would
//! bury sequence.
//!
//! Keyed on input, not env switch: one process drives success and failure (QR payload or
//! address containing `fail` is rejected).

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;

use chrono::{Local, TimeZone};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// `DC_CONTACT_ID_SELF`: own contact.
const SELF: u32 = 1;

/// Config key for largest recommended attachment, and real core's answer: `(30 - 1) MiB * 3/4`
/// (30 MiB chatmail relay, less 1 MiB headers, less base64 overhead). Computed, never stored.
const RECOMMENDED_SIZE_KEY: &str = "sys.msgsize_max_recommended";
const RECOMMENDED_SIZE: u64 = (30 - 1) * 1024 * 1024 / 4 * 3;

/// `sys.msgsize_max_recommended` answer. `PIIRIT_FAKE_MSGSIZE_MAX` lowers it, so oversize-file
/// tests write kilobytes, not twenty-odd megabytes.
fn recommended_size() -> String {
    std::env::var("PIIRIT_FAKE_MSGSIZE_MAX")
        .ok()
        .filter(|value| value.parse::<u64>().is_ok())
        .unwrap_or_else(|| RECOMMENDED_SIZE.to_string())
}

/// Account, as `get_all_accounts` reports.
struct Account {
    id: u32,
    configured: bool,
}

/// Call, as real core keeps it on its message.
struct FakeCall {
    /// Came in, not placed here.
    incoming: bool,
    has_video: bool,
    /// Caller's SDP.
    offer: String,
    /// Answered at either end.
    accepted: bool,
    /// Core's `CallState` kind.
    state: &'static str,
}

impl FakeCall {
    /// Message text core writes, for rows not reading state.
    fn text(&self) -> &'static str {
        match (self.state, self.incoming, self.has_video) {
            ("Missed", ..) => "Missed call",
            ("Declined", ..) => "Declined call",
            ("Canceled", ..) => "Canceled call",
            (_, true, false) => "Incoming audio call",
            (_, true, true) => "Incoming video call",
            (_, false, false) => "Outgoing audio call",
            (_, false, true) => "Outgoing video call",
        }
    }

    /// `call_info` as core serialises: `camelCase`, state tagged `kind`, duration only when
    /// completed.
    fn info(&self) -> Value {
        let state = if self.state == "Completed" {
            json!({"kind": "Completed", "duration": 185})
        } else {
            json!({"kind": self.state})
        };
        json!({"sdpOffer": self.offer, "hasVideo": self.has_video, "state": state})
    }
}

/// `send_msg` `MessageData` fields the row must carry back.
struct Sent {
    file: Value,
    view_type: String,
    text: Option<String>,
    file_name: Option<String>,
}

#[derive(Default)]
struct State {
    accounts: Vec<Account>,
    /// Highest account id handed out. Never reused.
    last_account: u32,
    /// Profile app last said it shows. Real core keeps it on disk, selects new account itself,
    /// falls back to first left when selected one removed.
    selected: Option<u32>,
    /// Events for `get_next_event_batch`.
    events: VecDeque<Value>,
    /// Message ids per chat, oldest first. Seeded on first use.
    chats: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Chat ids in display order, newest first.
    chat_order: Vec<u32>,
    /// Archived chats; disjoint from `chat_order`.
    archived_order: Vec<u32>,
    next_message_id: u32,
    /// Contact id -> address. Seeded with two contacts.
    contacts: std::collections::BTreeMap<u32, String>,
    next_chat_id: u32,
    /// Group members by chat; a chat with an entry is a group. Seeded one includes self, as
    /// real core lists it.
    group_members: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Group names set after creation.
    group_names: std::collections::BTreeMap<u32, String>,
    /// Group descriptions; `PIIRIT_FAKE_DESCRIPTION` sets seeded one.
    descriptions: std::collections::BTreeMap<u32, String>,
    /// Group pictures: path given, not core's copy.
    group_images: std::collections::BTreeMap<u32, String>,
    /// Left groups: core refuses all changes.
    left_groups: std::collections::BTreeSet<u32>,
    /// Deleted chats: core cannot load them.
    deleted_chats: std::collections::BTreeSet<u32>,
    /// Accounts with unanswered transport call. Core allows one ongoing process per account,
    /// refuses second; see `add_transport_from_qr`.
    configuring: std::collections::BTreeSet<u32>,
    /// Accounts asked to stop ongoing process.
    stopped: std::collections::BTreeSet<u32>,
    /// IO stopped for all accounts, not restarted: core then reports not connected, no relays.
    io_stopped: bool,
    /// Seconds until chat messages vanish.
    timers: std::collections::BTreeMap<u32, u32>,
    /// Config values per account.
    config: std::collections::BTreeMap<(u32, String), String>,
    /// Unsent draft text per chat.
    drafts: std::collections::BTreeMap<u32, String>,
    /// Contact nicknames. Empty name restores theirs.
    contact_names: std::collections::BTreeMap<u32, String>,
    /// Blocked contacts: kept out of `get_contacts` whatever flags, listed by
    /// `get_blocked_contacts` only, as real core.
    blocked: std::collections::BTreeSet<u32>,
    /// Messages whose remainder was requested; see `downloadState`.
    downloaded: std::collections::BTreeSet<u32>,
    /// Chats marked unread from list, by account; cleared when noticed. Core counts fresh
    /// messages; this only whether a chat has one.
    fresh: std::collections::BTreeSet<(u32, u32)>,
    /// Reactions by message, then contact: one list per contact, as core. Account's send
    /// replaces its own list.
    reactions: std::collections::BTreeMap<u32, std::collections::BTreeMap<u32, Vec<String>>>,
    /// What `send_msg` handed over, by message.
    sent_files: std::collections::BTreeMap<u32, Sent>,
    /// Webxdc status updates per instance, send order. Numbered from 1; all after asked
    /// serial handed out, as core.
    webxdc_updates: std::collections::BTreeMap<u32, Vec<Value>>,
    /// Text edits by message; marks message edited.
    edits: std::collections::BTreeMap<u32, String>,
    /// Muted chats. Keeps flag, not duration.
    muted: std::collections::BTreeSet<u32>,
    /// Message each sent message quotes.
    quotes: std::collections::BTreeMap<u32, u32>,
    /// Messages sent from here, send order. Read as own only when test asks; see `full_message`.
    sent: std::collections::BTreeSet<u32>,
    /// Calls by message; placed here or rung in via `PIIRIT_FAKE_INCOMING_CALL_MS`.
    calls: std::collections::BTreeMap<u32, FakeCall>,
    /// Per-account transports in add order: core's `transports` table, `list_transports`
    /// oldest first. Own-address one is `configured_addr` in `config`, as there.
    transports: std::collections::BTreeMap<u32, Vec<String>>,
}

impl State {
    /// Config value, as core reads back.
    fn config(&self, account: u32, key: &str) -> Option<String> {
        self.config.get(&(account, key.to_string())).cloned()
    }

    /// Account's transports, oldest first.
    fn transports_for(&self, account: u32) -> Vec<String> {
        self.transports.get(&account).cloned().unwrap_or_default()
    }

    /// Keep transport just given, as core: appended once; own address if none yet (second
    /// transport is a second address, not a change of the first).
    fn add_transport(&mut self, account: u32, addr: &str) {
        let list = self.transports.entry(account).or_default();
        if !list.iter().any(|known| known == addr) {
            list.push(addr.to_string());
        }
        self.config
            .entry((account, "configured_addr".to_string()))
            .or_insert_with(|| addr.to_string());
        self.transports_modified(account);
    }

    /// Configured profiles preset via `PIIRIT_FAKE_ACCOUNTS` (onboarding tests leave unset).
    /// Seeded before first request: page asking about profile before account list read
    /// finds its transports.
    ///
    /// Profile set up twice has relay besides own address's (`configured_addr`):
    /// `PIIRIT_FAKE_OLDER_RELAY` seeds it as first transport core lists.
    fn seed_accounts(&mut self) {
        let older = std::env::var("PIIRIT_FAKE_OLDER_RELAY").ok();
        for account in env_ids("PIIRIT_FAKE_ACCOUNTS") {
            let Ok(account) = u32::try_from(account) else {
                continue;
            };
            self.accounts.push(Account {
                id: account,
                configured: true,
            });
            let list = self.transports.entry(account).or_default();
            if let Some(older) = &older {
                list.push(older.clone());
            }
            let own = format!("account{account}@example.org");
            list.push(own.clone());
            self.config
                .insert((account, "configured_addr".to_string()), own);
        }
    }

    /// Event core sends when account transports change.
    fn transports_modified(&mut self, account: u32) {
        self.events.push_back(json!({
            "contextId": account,
            "event": {"kind": "TransportsModified"},
        }));
    }

    /// Accounts as core shapes them: configured carries profile (name, picture, colour) but no
    /// address (dropped in 2.61); unconfigured is id only.
    fn account_list(&self) -> Value {
        Value::Array(
            self.accounts
                .iter()
                .map(|account| {
                    if account.configured {
                        json!({
                            "id": account.id,
                            "kind": "Configured",
                            "displayName": self.config(account.id, "displayname").unwrap_or_default(),
                            "profileImage": self.config(account.id, "selfavatar"),
                            "color": "#4a90d9",
                        })
                    } else {
                        json!({"id": account.id, "kind": "Unconfigured"})
                    }
                })
                .collect(),
        )
    }

    /// Two seeded chats, for paging and adding one more.
    fn seed_chats(&mut self) {
        if self.chats.is_empty() {
            // Long chat for paging, on request. Ids count from 1 (newest highest); seeded
            // quote and picture keep their ids.
            let long = std::env::var("PIIRIT_FAKE_LONG_CHAT")
                .ok()
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|count| *count > 2);
            self.chats.insert(
                1,
                long.map_or_else(|| vec![1, 2], |count| (1..=count).collect()),
            );
            self.chats.insert(2, vec![10]);
            // Other media kinds on request: file, voice, app, video.
            if std::env::var_os("PIIRIT_FAKE_MEDIA").is_some() {
                self.chats.insert(2, vec![10, 12, 13, 14, 15]);
            }
            self.chat_order = vec![1, 2];
            // Extra empty chats from 201 up, on request.
            let many = std::env::var("PIIRIT_FAKE_MANY_CHATS")
                .ok()
                .and_then(|count| count.parse::<u32>().ok())
                .unwrap_or(0);
            for chat in 201..201u32.saturating_add(many) {
                self.chats.insert(chat, Vec::new());
                self.chat_order.push(chat);
            }
            // Chat 3 archived: else archived and ordinary lists look alike.
            self.chats.insert(3, vec![30]);
            // Empty archive is own case: page hides search with nothing to search.
            self.archived_order = if std::env::var("PIIRIT_FAKE_NO_ARCHIVED").is_ok() {
                Vec::new()
            } else {
                vec![3]
            };
            // Above seeded ids: messages added in tests can't collide.
            self.next_message_id = long.unwrap_or(0).max(100);
            self.contacts.insert(10, "ada@example.org".to_string());
            self.contacts.insert(11, "grace@example.org".to_string());
            // Chat 2 is group: self and one contact, so test can add other, remove this one.
            self.group_members.insert(2, vec![SELF, 10]);
            self.next_chat_id = 500;
            // Someone else's reaction on first message, on request: non-own chip, and what
            // ours on top counts up to.
            if std::env::var_os("PIIRIT_FAKE_REACTED").is_some() {
                self.reactions
                    .entry(1)
                    .or_default()
                    .insert(10, vec!["👍".to_string()]);
            }
        }
    }

    /// Reactions as core shapes: counted, most frequent first; null if none.
    /// Pinned in `deltachat-jsonrpc/tests/real_server.rs`.
    fn reactions_object(&self, msg: u32) -> Value {
        let Some(by_contact) = self.reactions.get(&msg) else {
            return Value::Null;
        };
        let mut counts: std::collections::BTreeMap<&str, (u32, bool)> =
            std::collections::BTreeMap::new();
        for (contact, emojis) in by_contact {
            for emoji in emojis {
                let entry = counts.entry(emoji.as_str()).or_default();
                entry.0 += 1;
                entry.1 |= *contact == SELF;
            }
        }
        if counts.is_empty() {
            return Value::Null;
        }
        let mut sorted: Vec<(&str, u32, bool)> = counts
            .into_iter()
            .map(|(emoji, (count, from_self))| (emoji, count, from_self))
            .collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        json!({
            "reactions": sorted
                .iter()
                .map(|(emoji, count, from_self)| {
                    json!({"emoji": emoji, "count": count, "isFromSelf": from_self})
                })
                .collect::<Vec<Value>>(),
            "reactionsByContact": by_contact
                .iter()
                .filter(|(_, emojis)| !emojis.is_empty())
                .map(|(contact, emojis)| (contact.to_string(), json!(emojis)))
                .collect::<serde_json::Map<String, Value>>(),
        })
    }

    /// Group if created here or seeded.
    fn is_group(&self, chat: u32) -> bool {
        self.group_members.contains_key(&chat)
    }

    /// Chat name: renamed, else seeded.
    fn chat_name(&self, chat: u32) -> String {
        self.group_names
            .get(&chat)
            .cloned()
            .unwrap_or_else(|| format!("chat {chat}"))
    }

    /// Contact as `get_contacts` shapes it. Own contact not listed, askable by id (how chat
    /// members come back). Ada has a bio line.
    ///
    /// Core's three names: `authName` contact's own, `name` given here, `displayName` `name`
    /// if set, else `authName`.
    ///
    /// Every key 2.62 core sends, values for unseen contact (`lastSeen` 0): "Old" freshness
    /// (core's for key contacts). Own contact never highlighted: "Normal".
    ///
    /// No pictures, unless `PIIRIT_FAKE_CONTACT_PICTURE` gives Ada one (path core would return).
    fn contact_object(&self, contact: u32) -> Option<Value> {
        let address = if contact == SELF {
            "me@example.org"
        } else {
            self.contacts.get(&contact)?
        };
        let auth_name = address.split('@').next().unwrap_or(address);
        let name = self
            .contact_names
            .get(&contact)
            .cloned()
            .unwrap_or_default();
        let display_name = if name.is_empty() {
            auth_name.to_string()
        } else {
            name.clone()
        };
        Some(json!({
            "id": contact,
            "address": address,
            "authName": auth_name,
            "name": name,
            "displayName": display_name,
            "isKeyContact": true,
            "e2eeAvail": true,
            "isBlocked": self.blocked.contains(&contact),
            "isBot": false,
            "status": if contact == 10 { "Poet and mathematician" } else { "" },
            "color": "#00875a",
            "profileImage": if contact == 10 {
                std::env::var("PIIRIT_FAKE_CONTACT_PICTURE").ok()
            } else {
                None
            },
            "lastSeen": 0,
            "freshness": if contact == SELF { "Normal" } else { "Old" },
        }))
    }

    /// Announce name, picture or member change, as core does.
    fn chat_modified(&mut self, account_id: u32, chat: u32) {
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ChatModified", "chatId": chat},
        }));
    }

    /// Chat a message is in. Search results need it.
    fn chat_of(&self, message_id: u32) -> u32 {
        self.chats
            .iter()
            .find(|(_, messages)| messages.contains(&message_id))
            .map_or(0, |(chat, _)| *chat)
    }

    /// One message, everything fake knows laid over seeded shape: chat, what was sent, edited
    /// text, quote, download-limit hold. Shared by `get_message` and `get_messages`, so they
    /// can't disagree.
    ///
    /// Sent-from-here reads as own only under `PIIRIT_FAKE_SELF_SENT`: rest of suite relies
    /// on sent message returning as someone else's (arrival without network).
    fn full_message(&self, msg: u64) -> Value {
        let mut message = message_object(msg);
        let id = u32::try_from(msg).unwrap_or_default();
        message["chatId"] = json!(self.chat_of(id));
        if let Some(sent) = self.sent_files.get(&id) {
            message["file"] = sent.file.clone();
            message["viewType"] = json!(sent.view_type);
            message["fromId"] = json!(SELF);
            if let Some(text) = &sent.text {
                message["text"] = json!(text);
            }
            if let Some(file_name) = &sent.file_name {
                message["fileName"] = json!(file_name);
            }
        }
        if std::env::var_os("PIIRIT_FAKE_SELF_SENT").is_some() && self.sent.contains(&id) {
            message["fromId"] = json!(SELF);
        }
        if let Some(text) = self.edits.get(&id) {
            message["text"] = json!(text);
            message["isEdited"] = json!(true);
        }
        if let Some(call) = self.calls.get(&id) {
            message["viewType"] = json!("Call");
            message["text"] = json!(call.text());
            if !call.incoming {
                message["fromId"] = json!(SELF);
            }
        }
        if let Some(quoted) = self.quotes.get(&id) {
            // Core's `WithMessage` quote: id, text, author.
            message["quote"] = json!({
                "kind": "WithMessage",
                "messageId": quoted,
                "text": wordy(u64::from(*quoted)),
                "authorDisplayName": "Ada Lovelace",
            });
        }
        // Message held by download limit until remainder requested.
        let held_back = std::env::var("PIIRIT_FAKE_HELD_BACK_MSG")
            .ok()
            .and_then(|value| value.parse::<u64>().ok());
        let fetched = self.downloaded.contains(&id);
        message["downloadState"] = json!(if held_back == Some(msg) && !fetched {
            "Available"
        } else {
            "Done"
        });
        message["reactions"] = self.reactions_object(id);
        message
    }

    /// Remember what a sent message quoted.
    fn note_quote(&mut self, msg: u32, quoted: &Value) {
        self.sent.insert(msg);
        if let Some(quoted) = quoted.as_u64().and_then(|value| u32::try_from(value).ok()) {
            if quoted != 0 {
                self.quotes.insert(msg, quoted);
            }
        }
    }

    /// Append message to chat and announce, as send or incoming message does.
    fn add_message(&mut self, account_id: u32, chat_id: u32) -> u32 {
        self.seed_chats();
        self.next_message_id += 1;
        let id = self.next_message_id;
        self.chats.entry(chat_id).or_default().push(id);
        // Message moves chat to top, so list reorders.
        self.chat_order.retain(|chat| *chat != chat_id);
        self.chat_order.insert(0, chat_id);
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "IncomingMsg", "chatId": chat_id, "msgId": id},
        }));
        // Core follows within same ms (pinned with vendored server: `MsgDelivered`, then
        // `ChatlistItemChanged`), plus `ChatlistChanged` if order changed. Model announcing
        // only on first event's refresh never announced: next event staled that answer.
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ChatlistItemChanged", "chatId": chat_id},
        }));
        id
    }

    /// Put message one place before end, announce. Core sorts received message below newest
    /// *seen* and no further: one dated before unread tail lands among them (late message in
    /// busy group, older one synced from other device).
    fn add_late_message(&mut self, account_id: u32, chat_id: u32) -> u32 {
        self.seed_chats();
        self.next_message_id += 1;
        let id = self.next_message_id;
        let messages = self.chats.entry(chat_id).or_default();
        let at = messages.len().saturating_sub(1);
        messages.insert(at, id);
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "IncomingMsg", "chatId": chat_id, "msgId": id},
        }));
        id
    }

    /// Put call's message in chat, as core does for placed and incoming calls. `MsgsChanged`
    /// announces it: ringing call is not `IncomingMsg`.
    fn add_call(&mut self, account_id: u32, chat_id: u32, call: FakeCall) -> u32 {
        self.seed_chats();
        self.next_message_id += 1;
        let id = self.next_message_id;
        self.chats.entry(chat_id).or_default().push(id);
        if !call.incoming {
            self.sent.insert(id);
        }
        self.calls.insert(id, call);
        self.call_changed(account_id, id);
        id
    }

    /// Announce call state move: `MsgsChanged` for its message.
    fn call_changed(&mut self, account_id: u32, message_id: u32) {
        let chat_id = self.chat_of(message_id);
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "MsgsChanged", "chatId": chat_id, "msgId": message_id},
        }));
    }

    /// One of four call events, `snake_case`.
    fn call_event(&mut self, account_id: u32, kind: &str, message_id: u32, extra: &Value) {
        let mut event = json!({
            "kind": kind,
            "msg_id": message_id,
            "chat_id": self.chat_of(message_id),
        });
        if let (Some(event), Some(extra)) = (event.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                event.insert(key.clone(), value.clone());
            }
        }
        self.events
            .push_back(json!({"contextId": account_id, "event": event}));
    }

    /// Queue one import/export progress event: 0 failure, 1000 done.
    fn imex(&mut self, account_id: u32, progress: u32) {
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ImexProgress", "progress": progress},
        }));
    }

    /// Configure account, queue progress: permille steps, then 1000.
    fn configure(&mut self, account_id: u32) {
        for account in &mut self.accounts {
            if account.id == account_id {
                account.configured = true;
            }
        }
        for progress in [300_u32, 1000] {
            self.events.push_back(json!({
                "contextId": account_id,
                "event": {"kind": "ConfigureProgress", "progress": progress, "comment": null},
            }));
        }
    }
}

/// Server start time, for journal clock.
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn journal(method: &str, params: &Value) {
    let Ok(path) = std::env::var("PIIRIT_FAKE_JOURNAL") else {
        return;
    };
    // Polling noise would bury the sequence.
    if method == "get_next_event_batch" {
        return;
    }
    // Arrival ms since server start: call-overlap tests can't use own Qt timer (too coarse,
    // fires two probes at once).
    let at = STARTED
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis();
    let line = json!({"method": method, "params": params, "at": at}).to_string() + "\n";
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        // One write, not `writeln!`'s two: concurrent newlines tore lines.
        let _ = file.write_all(line.as_bytes());
    }
}

/// What a message says. Short by default (most tests assert on it).
///
/// `PIIRIT_FAKE_WORDY` makes it wrap, as real chats: row height then depends on draw width.
/// Nothing else here can make laid-out row resize; view that can't shift can't be shown
/// holding its place.
fn wordy(msg: u64) -> String {
    let text = format!("message {msg}");
    if std::env::var_os("PIIRIT_FAKE_WORDY").is_none() {
        return text;
    }
    format!("{text}, and then a good deal more of it, long enough that where it wraps depends on how wide the row is drawn")
}

/// When a message was sent.
///
/// 2023-11-14T22:13:20Z and a day later, so day separator has something to separate. Own fn
/// because day markers must agree: placeholder row takes day from marker, filled row from
/// message; disagreeing = heading changes under reader.
fn message_timestamp(msg: u64) -> i64 {
    if msg == 1 {
        1_700_000_000
    } else {
        1_700_090_000
    }
}

/// Local midnight of `timestamp`'s day, as core's day markers. Checked in three zones.
fn day_start(timestamp: i64) -> i64 {
    let Some(when) = Local.timestamp_opt(timestamp, 0).single() else {
        return timestamp.div_euclid(86_400) * 86_400;
    };
    when.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| midnight.and_local_timezone(Local).earliest())
        .map_or(timestamp, |midnight| midnight.timestamp())
}

/// Fake archive: a page, its script, an icon.
///
/// Nothing else in this webxdc. Page asks for script by absolute path into a dir, as bundlers
/// write (half the store's apps); host serving under prefix answers nothing to it.
fn webxdc_file(path: &str) -> Option<Vec<u8>> {
    match path {
        "index.html" => Some(
            b"<html><head><title>Checkers</title>              <script type=\"module\" crossorigin src=\"/assets/app.js\"></script>              </head><body>board</body></html>"
                .to_vec(),
        ),
        "assets/app.js" => Some(b"window.playing = true\n".to_vec()),
        "icon.png" => Some(b"\x89PNG\r\n\x1a\n icon".to_vec()),
        _ => None,
    }
}

/// Standard base64, unpadded, as core encodes blobs. Test double.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut block = [0_u8; 3];
        block[..chunk.len()].copy_from_slice(chunk);
        let packed = (u32::from(block[0]) << 16) | (u32::from(block[1]) << 8) | u32::from(block[2]);
        for index in 0..=chunk.len() {
            let shift = 18 - 6 * index;
            out.push(ALPHABET[((packed >> shift) & 0x3f) as usize] as char);
        }
    }
    out
}

/// Message shaped like real core's. Seeded 1 quotes, 2 unread, 10 has image: one fetch
/// covers what conversation view renders.
fn message_object(msg: u64) -> Value {
    let timestamp = message_timestamp(msg);
    let info_first = msg == 1 && std::env::var_os("PIIRIT_FAKE_INFO_FIRST").is_some();
    let mut message = json!({
        "kind": "message",
        "text": if info_first {
            "Messages are end-to-end encrypted.".to_string()
        } else {
            wordy(msg)
        },
        "fromId": 10,
        "timestamp": timestamp,
        "showPadlock": true,
        // Seeded message unread, and any added during a test: arrivals worth covering.
        "state": if msg == 2 || msg > 100 { 10 } else { 16 },
        // First message of real chat: core's "end-to-end encrypted" notice, the row day
        // heading was reported drawn over.
        "isInfo": info_first,
        "viewType": "Text",
        "sender": {"id": 10, "displayName": "Ada Lovelace", "color": "#00875a"},
        "overrideSenderName": null,
        "quote": null,
        "file": null,
        "fileName": null,
        "dimensionsWidth": 0,
        "dimensionsHeight": 0,
    });
    if msg == 1 {
        message["quote"] = json!({"text": "earlier", "authorDisplayName": "Grace Hopper"});
    }
    if msg == 10 {
        message["viewType"] = json!("Image");
        message["file"] = json!("/tmp/piirit-fake/photo.jpg");
        message["fileName"] = json!("photo.jpg");
        message["dimensionsWidth"] = json!(640);
        message["dimensionsHeight"] = json!(480);
    }
    // One of each other media kind, and a second picture that is video: in group under
    // `PIIRIT_FAKE_MEDIA`, by id always. Sizes/types as core reports for copied file.
    match msg {
        12 => {
            message["viewType"] = json!("File");
            // Under its hash, as core keeps received file; sender's name is `fileName`.
            message["file"] = json!("/tmp/piirit-fake/6f5902ac237024bdd0c176cb93063dc4.pdf");
            message["fileName"] = json!("notes.pdf");
            message["fileMime"] = json!("application/pdf");
            message["fileBytes"] = json!(20_480);
        }
        13 => {
            message["viewType"] = json!("Voice");
            message["file"] = json!("/tmp/piirit-fake/voice.aac");
            message["fileName"] = json!("voice.aac");
            message["fileMime"] = json!("audio/aac");
            message["fileBytes"] = json!(4_096);
            // Milliseconds, as sending client writes for recording: row shows it before play.
            message["duration"] = json!(4_000);
        }
        14 => {
            message["viewType"] = json!("Webxdc");
            message["file"] = json!("/tmp/piirit-fake/checkers.xdc");
            message["fileName"] = json!("checkers.xdc");
            message["fileMime"] = json!("application/webxdc+zip");
            message["fileBytes"] = json!(8_192);
        }
        15 => {
            message["viewType"] = json!("Video");
            message["file"] = json!("/tmp/piirit-fake/clip.mp4");
            message["fileName"] = json!("clip.mp4");
            message["fileMime"] = json!("video/mp4");
            message["fileBytes"] = json!(1_048_576);
        }
        _ => {}
    }
    // Message cut by sending core: ends in core's marker, whole only via `get_message_html`.
    // In no chat: reader asks by id, as whole-message page does.
    if msg == 11 {
        message["text"] = json!(format!("{LONG_MESSAGE_HEAD}\n[...]"));
        message["hasHtml"] = json!(true);
    }
    message
}

/// Start of long message: fits in `text`.
const LONG_MESSAGE_HEAD: &str = "# Groceries";

/// Whole message as core gives it, HTML part.
///
/// Written as pinned `deltachat-rpc-server` writes it. Newlines matter: core's template puts
/// head on own lines, turns each message newline into `<br/>` *plus newline*, so a reader
/// counting both double-spaces. On one line, that bug would reach phone unnoticed.
const LONG_MESSAGE_HTML: &str = "<!DOCTYPE html>\n\
     <html><head>\n\
     <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\" />\n\
     <meta name=\"color-scheme\" content=\"light dark\" />\n\
     </head><body dir=\"auto\" style=\"unicode-bidi: plaintext\">\n\
     Groceries<br/>\nmilk<br/>\nbread<br/>\nand a &amp; sign<br/>\n\
     </body></html>\n";

/// True for inputs standing in for "server cannot be reached".
fn should_fail(value: &str) -> bool {
    value.contains("fail")
}

/// Chat ids in `var`, comma-separated: test marks which seeded chats stand for something
/// (self chat, device chat) without a fixture each.
fn env_ids(var: &str) -> Vec<u64> {
    std::env::var(var)
        .ok()
        .map(|value| {
            value
                .split(',')
                .filter_map(|id| id.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Ask relay in `qr` for an account.
///
/// Instant unless `slow` in payload: answered after `PIIRIT_FAKE_SLOW_MS` (3 s default), as
/// down relay holds core through connection attempts. Pending = configuring, with core's
/// two rules: second transport call on account refused (core's words), and
/// `stop_ongoing_process` makes it fail, unless `deaf` (relay answers anyway: stop too late).
async fn add_transport_from_qr(
    state: &Arc<Mutex<State>>,
    id: &Value,
    account: u32,
    qr: &str,
) -> Value {
    if should_fail(qr) {
        return err(id, "cannot resolve chatmail server");
    }
    if !state.lock().await.configuring.insert(account) {
        return err(id, "There is already another ongoing process running.");
    }
    let stopped = if qr.contains("slow") {
        tokio::time::sleep(delay_or("PIIRIT_FAKE_SLOW_MS", 3000)).await;
        !qr.contains("deaf") && state.lock().await.stopped.contains(&account)
    } else {
        false
    };
    let mut state = state.lock().await;
    state.configuring.remove(&account);
    state.stopped.remove(&account);
    if stopped {
        return err(id, "Configuration was stopped");
    }
    // Address relay mints: account's name on payload's relay, enough to tell rows apart.
    let relay = qr
        .trim_start_matches("dcaccount:")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string();
    state.add_transport(account, &format!("account{account}@{relay}"));
    state.configure(account);
    ok(id, &Value::Null)
}

/// What `check_qr` makes of a payload: invite, relay code, or other (shim's distinctions).
/// Relay code names relay as core reads it: `dcaccount:` domain, `dclogin:` address.
fn qr_kind(content: &str) -> Value {
    let lower = content.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("dcaccount:") {
        let domain = rest
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or_default();
        return json!({"kind": "account", "domain": domain});
    }
    if let Some(rest) = lower.strip_prefix("dclogin:") {
        let address = rest
            .trim_start_matches("//")
            .split('?')
            .next()
            .unwrap_or_default();
        return json!({"kind": "login", "address": address});
    }
    let kind = if content.contains("i.delta.chat") || content.starts_with("OPENPGP4FPR:") {
        "askVerifyContact"
    } else if content.starts_with("DCBACKUP2:") {
        // Device offering itself as first device; `toonew` stands in for newer backup.
        if content.contains("toonew") {
            "backupTooNew"
        } else {
            "backup2"
        }
    } else {
        "text"
    };
    json!({"kind": kind})
}

/// Relay `init_transports` lands account on first. Slow (like `slow` payload) if
/// `PIIRIT_FAKE_AUTO_RELAY` says: no payload to say it with.
fn automatic_relay() -> String {
    std::env::var("PIIRIT_FAKE_AUTO_RELAY").unwrap_or_else(|_| "first.auto.example".to_string())
}

/// Take profile over from another device or file: `ImexProgress`, then configured.
///
/// `fail` refuses; `slow` waits (answers `stop_ongoing_process` unless `deaf`).
async fn import_into(state: &Arc<Mutex<State>>, id: &Value, account: u32, from: &str) -> Value {
    if should_fail(from) {
        return err(id, "backup could not be read");
    }
    if !state.lock().await.configuring.insert(account) {
        return err(id, "There is already another ongoing process running.");
    }
    state.lock().await.imex(account, 300);
    let stopped = if from.contains("slow") {
        tokio::time::sleep(delay_or("PIIRIT_FAKE_SLOW_MS", 3000)).await;
        !from.contains("deaf") && state.lock().await.stopped.contains(&account)
    } else {
        false
    };
    let mut state = state.lock().await;
    state.configuring.remove(&account);
    state.stopped.remove(&account);
    if stopped {
        state.imex(account, 0);
        return err(id, "Transfer was stopped");
    }
    state.imex(account, 1000);
    // Backup carries profile incl. address: same backup twice = same address twice. Keyed on
    // what import was handed (only thing two imports share here).
    let addr = backup_addr(from);
    state
        .config
        .insert((account, "configured_addr".to_string()), addr.clone());
    state.add_transport(account, &addr);
    state.configure(account);
    ok(id, &Value::Null)
}

/// One relay's block of connectivity report, as core writes (pinned in
/// `deltachat-jsonrpc/tests/real_server.rs`): domain bold, mailbox on bar. Each relay's
/// figure distinct, so page reading wrong relay's bar is caught.
fn transport_block(addr: &str) -> String {
    let domain = addr.rsplit('@').next().unwrap_or(addr);
    // Older relay: drifted, reached but not lately. Core draws it yellow.
    let (dot, status, words, colour, percent) = match domain {
        "example.org" => ("green", "Connected", "1.34 GiB of 2 GiB used", "grey", 67),
        relay if relay.starts_with("old.") => {
            ("yellow", "Connecting…", "1.9 GiB of 2 GiB used", "red", 95)
        }
        _ => ("green", "Connected", "12 MiB of 1 GiB used", "grey", 2),
    };
    format!(
        "<li class=\"transport\"><span class=\"{dot} dot\"></span> <b>{domain}:</b> {status}<br />\
         <ul class=\"quota-list\"><li>{words}\
         <div class=\"bar\"><div class=\"progress {colour}\" style=\"width: {percent}%\">{percent}%</div></div>\
         </li></ul></li>"
    )
}

/// Backup's address, from its file path or code: same thing, same address.
fn backup_addr(from: &str) -> String {
    let name: String = from
        .rsplit('/')
        .next()
        .unwrap_or(from)
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() {
                letter
            } else {
                '-'
            }
        })
        .collect();
    format!("{}@example.org", name.trim_matches('-'))
}

/// Write profile out as core's `export_backup`: `ImexProgress`, `.tar` in folder.
///
/// `fail` refuses; `slow` waits (answers `stop_ongoing_process` unless `deaf`).
async fn export_into(state: &Arc<Mutex<State>>, id: &Value, account: u32, folder: &str) -> Value {
    if should_fail(folder) {
        return err(id, "backup could not be written");
    }
    if !state.lock().await.configuring.insert(account) {
        return err(id, "There is already another ongoing process running.");
    }
    state.lock().await.imex(account, 300);
    let stopped = if folder.contains("slow") {
        tokio::time::sleep(delay_or("PIIRIT_FAKE_SLOW_MS", 3000)).await;
        !folder.contains("deaf") && state.lock().await.stopped.contains(&account)
    } else {
        false
    };
    let mut state = state.lock().await;
    state.configuring.remove(&account);
    state.stopped.remove(&account);
    if stopped {
        state.imex(account, 0);
        return err(id, "Export was stopped");
    }
    // Name core builds: date, number, address. Page finds file by what appeared in folder:
    // exactly one new file is what matters.
    let written = std::path::Path::new(folder).join(format!("delta-chat-2026-01-01-{account}.tar"));
    // Off runtime threads, as shim does: `std::fs` blocks, server keeps answering.
    let made = {
        let (folder, target) = (folder.to_string(), written.clone());
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&folder).and_then(|()| std::fs::write(&target, b"backup"))
        })
        .await
        .map_err(|joined| joined.to_string())
        .and_then(|wrote| wrote.map_err(|problem| problem.to_string()))
    };
    if let Err(problem) = made {
        state.imex(account, 0);
        return err(id, &format!("backup could not be written: {problem}"));
    }
    state.events.push_back(json!({
        "contextId": account,
        "event": {"kind": "ImexFileWritten", "path": written.to_string_lossy()},
    }));
    state.imex(account, 1000);
    ok(id, &Value::Null)
}

/// Offer profile to second device, as core's `provide_backup`: call open until taken or
/// stopped. `ImexProgress`: 1000 taken, 0 stopped. Only transfer reported: unread code on
/// screen emits nothing.
///
/// Keyed on account (call carries nothing else): `PIIRIT_FAKE_PROVIDE_FAIL` refuses,
/// `PIIRIT_FAKE_TAKEN` device turns up `PIIRIT_FAKE_SLOW_MS` later, `PIIRIT_FAKE_STALLED`
/// turns up then leaves. Others wait to be stopped (code left up).
async fn provide_backup(state: &Arc<Mutex<State>>, id: &Value, account: u32) -> Value {
    if env_ids("PIIRIT_FAKE_PROVIDE_FAIL").contains(&u64::from(account)) {
        return err(id, "backup could not be offered");
    }
    if !state.lock().await.configuring.insert(account) {
        return err(id, "There is already another ongoing process running.");
    }
    let stalls = env_ids("PIIRIT_FAKE_STALLED").contains(&u64::from(account));
    let taken = if stalls || env_ids("PIIRIT_FAKE_TAKEN").contains(&u64::from(account)) {
        // Device turns up half a wait in: transfer has a middle, so page bar has something
        // before the 1000 and mid-transfer test has room both sides.
        let wait = delay_or("PIIRIT_FAKE_SLOW_MS", 3000);
        tokio::time::sleep(wait / 2).await;
        state.lock().await.imex(account, 300);
        if stalls {
            false
        } else {
            tokio::time::sleep(wait).await;
            true
        }
    } else {
        wait_for_stop(state, account).await;
        false
    };
    let mut state = state.lock().await;
    state.configuring.remove(&account);
    state.stopped.remove(&account);
    if !taken {
        state.imex(account, 0);
        // `Ok` like core: stopped provider and one whose device left both just finish; only
        // progress (0 vs 1000) tells them apart.
        return ok(id, &Value::Null);
    }
    state.imex(account, 1000);
    ok(id, &Value::Null)
}

/// Wait for `stop_ongoing_process`: real provider holds call while code up, so this does.
/// Bounded at half a minute so a test never stopping it ends itself.
async fn wait_for_stop(state: &Arc<Mutex<State>>, account: u32) {
    for _ in 0..600 {
        if state.lock().await.stopped.contains(&account) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Code a running provider shows, as core's `get_backup_qr`: `DCBACKUP5:` payload (pinned
/// core's transfer version), opaque but to core at other end. Named for account, so two
/// profiles offered in turn differ.
///
/// Real call blocks until provider ready, fails after a minute; this waits for provider
/// start (much sooner). `PIIRIT_FAKE_NO_QR` accounts fail outright: provider behind blank page.
async fn backup_qr(state: &Arc<Mutex<State>>, id: &Value, account: u32) -> Value {
    if env_ids("PIIRIT_FAKE_NO_QR").contains(&u64::from(account)) {
        return err(id, "no backup provider is running");
    }
    for _ in 0..100 {
        if state.lock().await.configuring.contains(&account) {
            return ok(
                id,
                &json!(format!("DCBACKUP5:account{account}-example-org&fake")),
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    err(id, "no backup provider is running")
}

/// Reply delay in ms from `var`, or `default`.
fn delay_or(var: &str, default: u64) -> std::time::Duration {
    std::time::Duration::from_millis(
        std::env::var(var)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default),
    )
}

/// Reply delay in ms from `var`. Fixes order two replies land in.
fn delay(var: &str) -> std::time::Duration {
    delay_or(var, 0)
}

fn main() {
    // Not `#[tokio::main]`: needs `macros` feature, a proc-macro crate app build doesn't need.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("fake server: cannot build a runtime: {err}");
            std::process::exit(1);
        }
    };
    runtime.block_on(serve());
}

#[allow(clippy::too_many_lines)]
async fn serve() {
    // Seeded before first request: early profile asks find it.
    let mut seeded = State::default();
    seeded.seed_accounts();
    let state = Arc::new(Mutex::new(seeded));
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

    // Events of asked-for kinds, payload-free, queued first. Real core can't say things on
    // demand, and unread-kind cost is what `HANDLED_EVENT_KINDS` is about: tests name kinds.
    if let Ok(kinds) = std::env::var("PIIRIT_FAKE_SEED_EVENTS") {
        let mut state = state.lock().await;
        for kind in kinds.split(',').filter(|kind| !kind.is_empty()) {
            state.events.push_back(json!({
                "contextId": 1,
                "event": {"kind": kind},
            }));
        }
        drop(state);
    }
    // Server dying under client.
    if let Ok(after) = std::env::var("PIIRIT_FAKE_EXIT_AFTER_MS") {
        if let Ok(millis) = after.parse() {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                std::process::exit(0);
            });
        }
    }
    // Message sorting mid-chat: no app call can do it; tests need this.
    if let Ok(after) = std::env::var("PIIRIT_FAKE_LATE_ARRIVAL_MS") {
        if let Ok(millis) = after.parse() {
            let state = state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                state.lock().await.add_late_message(1, 1);
            });
        }
    }
    // Call ringing in after delay: first chat's other end, audio only, offer page would
    // answer. Core says `IncomingCall`, and `MsgsChanged` for its message.
    if let Ok(after) = std::env::var("PIIRIT_FAKE_INCOMING_CALL_MS") {
        if let Ok(millis) = after.parse() {
            let state = state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                let mut state = state.lock().await;
                let id = state.add_call(
                    1,
                    1,
                    FakeCall {
                        incoming: true,
                        has_video: false,
                        offer: "v=0 fake-offer".to_string(),
                        accepted: false,
                        state: "Alerting",
                    },
                );
                state.call_event(
                    1,
                    "IncomingCall",
                    id,
                    &json!({"place_call_info": "v=0 fake-offer", "has_video": false}),
                );
            });
        }
    }
    let mut lines = BufReader::new(tokio::io::stdin()).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let state = state.clone();
        let stdout = stdout.clone();
        tokio::spawn(async move {
            let Ok(request) = serde_json::from_str::<Value>(&line) else {
                return;
            };
            let Some(id) = request.get("id").cloned() else {
                return;
            };
            let method = request
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            journal(&method, &params);

            let positional = |index: usize| -> Value {
                params
                    .as_array()
                    .and_then(|array| array.get(index))
                    .cloned()
                    .unwrap_or(Value::Null)
            };
            let account_id = || -> u32 {
                positional(0)
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .unwrap_or_default()
            };

            let response = match method.as_str() {
                "get_system_info" => ok(&id, &json!({"name": "fake-core-server"})),
                "get_all_accounts" => {
                    let mut state = state.lock().await;
                    // Reseeded when empty: deleting all profiles gives the starting ones.
                    if state.accounts.is_empty() {
                        state.seed_accounts();
                    }
                    let list = state.account_list();
                    ok(&id, &list)
                }
                "remove_account" => {
                    let mut state = state.lock().await;
                    let gone = positional(0).as_u64().unwrap_or(0);
                    let gone = u32::try_from(gone).unwrap_or(0);
                    state.accounts.retain(|account| account.id != gone);
                    // Core deletes account directory, with transports and config.
                    state.transports.remove(&gone);
                    state.config.retain(|(account, _), _| *account != gone);
                    if state.selected == Some(gone) {
                        state.selected = state.accounts.first().map(|account| account.id);
                    }
                    ok(&id, &Value::Null)
                }
                "add_account" => {
                    let mut state = state.lock().await;
                    let next = state
                        .accounts
                        .iter()
                        .map(|account| account.id)
                        .fold(state.last_account, u32::max)
                        + 1;
                    state.last_account = next;
                    state.accounts.push(Account {
                        id: next,
                        configured: false,
                    });
                    state.selected = Some(next);
                    ok(&id, &json!(next))
                }
                "select_account" => {
                    let mut state = state.lock().await;
                    let chosen = account_id();
                    if state.accounts.iter().any(|account| account.id == chosen) {
                        state.selected = Some(chosen);
                        ok(&id, &Value::Null)
                    } else {
                        err(&id, "account not found")
                    }
                }
                "get_selected_account_id" => {
                    let state = state.lock().await;
                    ok(&id, &json!(state.selected))
                }
                "set_config" => {
                    let account = positional(0)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let key = positional(1).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    // Null clears, as core; empty string is a value. Core refuses "" for
                    // `selfavatar`, so app must send null.
                    match positional(2).as_str() {
                        // Own address: core takes only one it has transport for (its words),
                        // since 2.61 even with no transports: writing no longer makes one, nor
                        // clears. Pinned offline against pinned binary.
                        Some(value) if key == "configured_addr" => {
                            if state
                                .transports_for(account)
                                .iter()
                                .any(|addr| addr == value)
                            {
                                state.config.insert((account, key), value.to_string());
                                ok(&id, &Value::Null)
                            } else {
                                err(&id, "Address does not belong to any transport.")
                            }
                        }
                        None if key == "configured_addr" => {
                            err(&id, "Cannot unset configured_addr")
                        }
                        Some(value) => {
                            state.config.insert((account, key), value.to_string());
                            ok(&id, &Value::Null)
                        }
                        None => {
                            state.config.remove(&(account, key));
                            ok(&id, &Value::Null)
                        }
                    }
                }
                "get_config" => {
                    let account = positional(0)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let key = positional(1).as_str().unwrap_or_default().to_string();
                    let state = state.lock().await;
                    // `sys.` keys are computed answers, not stored.
                    let default = (key == RECOMMENDED_SIZE_KEY).then(recommended_size);
                    let value = state
                        .config
                        .get(&(account, key))
                        .cloned()
                        .or(default)
                        .map_or(Value::Null, Value::String);
                    ok(&id, &value)
                }
                // Read and unread, as list marks them. Core announces both (badges follow
                // announcements); `get_fresh_msgs` is what profiles page counts.
                "marknoticed_chat" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.fresh.remove(&(account, chat));
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "MsgsNoticed", "chatId": chat},
                    }));
                    ok(&id, &Value::Null)
                }
                "markfresh_chat" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.fresh.insert((account, chat));
                    // Core (chat.rs `markfresh_chat`) sets newest incoming fresh, announces chat
                    // changed (no message named) and its list item changed.
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "MsgsChanged", "chatId": chat, "msgId": 0},
                    }));
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "ChatlistItemChanged", "chatId": chat},
                    }));
                    ok(&id, &Value::Null)
                }
                // Read position: last run of unseen messages, walking back from newest to
                // first seen, as core. Uses `message_object`'s states: fake can't disagree.
                "get_first_unread_message_of_chat" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let mut first: Option<u32> = None;
                    for msg in state
                        .chats
                        .get(&chat)
                        .cloned()
                        .unwrap_or_default()
                        .iter()
                        .rev()
                    {
                        match message_object(u64::from(*msg))
                            .get("state")
                            .and_then(Value::as_u64)
                        {
                            // InSeen
                            Some(16) => break,
                            // InFresh, InNoticed
                            Some(10 | 13) => first = Some(*msg),
                            // Anything of ours, which is neither.
                            _ => {}
                        }
                    }
                    ok(&id, &first.map_or(Value::Null, |msg| json!(msg)))
                }
                "get_fresh_msgs" => {
                    let account = account_id();
                    // Held back on request: events arrive while a count is read.
                    tokio::time::sleep(delay("PIIRIT_FAKE_FRESH_DELAY_MS")).await;
                    let state = state.lock().await;
                    let ids: Vec<u32> = state
                        .fresh
                        .iter()
                        .filter(|(owner, _)| *owner == account)
                        .filter_map(|(_, chat)| {
                            state
                                .chats
                                .get(chat)
                                .and_then(|messages| messages.last().copied())
                        })
                        .collect();
                    ok(&id, &json!(ids))
                }
                "start_io"
                | "maybe_network"
                | "markseen_msgs"
                | "set_chat_visibility"
                | "resend_messages"
                // Taken and forgotten; journal is what tests read.
                | "set_stock_strings" => ok(&id, &Value::Null),
                "start_io_for_all_accounts" | "stop_io_for_all_accounts" => {
                    state.lock().await.io_stopped = method == "stop_io_for_all_accounts";
                    ok(&id, &Value::Null)
                }
                "stop_ongoing_process" => {
                    state.lock().await.stopped.insert(account_id());
                    ok(&id, &Value::Null)
                }
                // Relays core picks itself: first that answers, plus a second once up (core:
                // in background). Keyed on account: nothing else handed over.
                "init_transports" => {
                    let account = account_id();
                    let qr = format!("dcaccount:{}", automatic_relay());
                    let answer = add_transport_from_qr(&state, &id, account, &qr).await;
                    if answer.get("error").is_none() {
                        state
                            .lock()
                            .await
                            .add_transport(account, &format!("account{account}@second.auto.example"));
                    }
                    answer
                }
                "add_transport_from_qr" => {
                    let qr = positional(1).as_str().unwrap_or_default().to_string();
                    add_transport_from_qr(&state, &id, account_id(), &qr).await
                }
                // Both core's import, keyed on what's handed: other device's code or file path.
                "get_backup" | "import_backup" => {
                    let from = positional(1).as_str().unwrap_or_default().to_string();
                    import_into(&state, &id, account_id(), &from).await
                }
                // `get_backup` other end: this phone holds profile for a device reading the code.
                "provide_backup" => provide_backup(&state, &id, account_id()).await,
                "get_backup_qr" => backup_qr(&state, &id, account_id()).await,
                // Other direction: profile written to folder.
                "export_backup" => {
                    let folder = positional(1).as_str().unwrap_or_default().to_string();
                    export_into(&state, &id, account_id(), &folder).await
                }
                "add_or_update_transport" => {
                    let param = positional(1);
                    let addr = param
                        .get("addr")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let has_password = param
                        .get("password")
                        .and_then(Value::as_str)
                        .is_some_and(|password| !password.is_empty());
                    if addr.is_empty() || !has_password {
                        // Malformed EnteredLoginParam is invalid-params, as core.
                        err(&id, "invalid params: addr and password are required")
                    } else if should_fail(addr) {
                        err(&id, "could not connect to server")
                    } else {
                        let mut state = state.lock().await;
                        state.add_transport(account_id(), addr);
                        state.configure(account_id());
                        ok(&id, &Value::Null)
                    }
                }
                // Transports oldest first, each `EnteredLoginParam` (app reads `addr`).
                "list_transports" => {
                    let listed: Vec<Value> = state
                        .lock()
                        .await
                        .transports_for(account_id())
                        .into_iter()
                        .map(|addr| json!({"addr": addr, "password": ""}))
                        .collect();
                    ok(&id, &Value::Array(listed))
                }
                // Removing last or unknown transport refused (core's words). Removing
                // own-address one elects newest of rest: upstream `delete_transport`,
                // `maybe_update_sending_transport`.
                "delete_transport" => {
                    let account = account_id();
                    let addr = positional(1).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    let known = state.transports_for(account);
                    if known.len() <= 1 {
                        err(&id, "Cannot remove the last transport")
                    } else if !known.contains(&addr) {
                        err(&id, "Transport does not exist")
                    } else {
                        let kept: Vec<String> =
                            known.into_iter().filter(|kept| *kept != addr).collect();
                        if let Some(newest) = kept.last().filter(|_| {
                            state.config(account, "configured_addr").as_deref()
                                == Some(addr.as_str())
                        }) {
                            state
                                .config
                                .insert((account, "configured_addr".to_string()), newest.clone());
                        }
                        state.transports.insert(account, kept);
                        state.transports_modified(account);
                        ok(&id, &Value::Null)
                    }
                }
                "get_contacts" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let flags = positional(1).as_u64().unwrap_or(0);
                    let query = positional(2).as_str().unwrap_or_default().to_lowercase();
                    let mut ids: Vec<u32> = state
                        .contacts
                        .iter()
                        .filter(|(contact, address)| {
                            !state.blocked.contains(contact)
                                && (query.is_empty() || address.to_lowercase().contains(&query))
                        })
                        .map(|(contact, _)| *contact)
                        .collect();
                    // `DC_GCL_ADD_SELF` puts own contact last, only when not searching, as core
                    // (pinned in real_server.rs).
                    if flags & 0x02 != 0 && query.is_empty() {
                        ids.push(SELF);
                    }
                    let contacts: Vec<Value> = ids
                        .iter()
                        .filter_map(|contact| state.contact_object(*contact))
                        .collect();
                    ok(&id, &Value::Array(contacts))
                }
                // Core refusing to list blocked, for accounts test names.
                "get_blocked_contacts"
                    if env_ids("PIIRIT_FAKE_BLOCKED_FAIL").contains(&u64::from(account_id())) =>
                {
                    err(&id, "the block list could not be read")
                }
                // Blocked contacts: no `get_contacts` flag brings them back.
                "get_blocked_contacts" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let ids: Vec<u32> = state.blocked.iter().copied().collect();
                    let contacts: Vec<Value> = ids
                        .iter()
                        .filter_map(|contact| state.contact_object(*contact))
                        .collect();
                    ok(&id, &Value::Array(contacts))
                }
                // Block and unblock: answer nothing, announce change.
                "block_contact" | "unblock_contact" => {
                    let account = account_id();
                    let contact = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    if method == "block_contact" {
                        state.blocked.insert(contact);
                    } else {
                        state.blocked.remove(&contact);
                    }
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "ContactsChanged", "contactId": contact},
                    }));
                    ok(&id, &Value::Null)
                }
                "change_contact_name" => {
                    let account = account_id();
                    let contact = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let name = positional(2).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    if name.is_empty() {
                        state.contact_names.remove(&contact);
                    } else {
                        state.contact_names.insert(contact, name);
                    }
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "ContactsChanged", "contactId": contact},
                    }));
                    ok(&id, &Value::Null)
                }
                // Connection and mailbox as profile page reads: band, and web-view report with
                // quota bar, as core writes (pinned in deltachat-jsonrpc/tests/real_server.rs).
                // IO stopped: report says not connected, no relay.
                "get_connectivity" => {
                    let band = if state.lock().await.io_stopped {
                        1000
                    } else {
                        4000
                    };
                    ok(&id, &json!(band))
                }
                "get_connectivity_html" if state.lock().await.io_stopped => {
                    ok(&id, &json!("<h3>Not connected</h3>"))
                }
                "get_connectivity_html" => {
                    // Every transport, oldest first, own quota each: profile set up twice shows
                    // older relay's bar first. Distinct figures catch page reading wrong bar.
                    let blocks: Vec<String> = state
                        .lock()
                        .await
                        .transports_for(account_id())
                        .iter()
                        .map(|addr| transport_block(addr))
                        .collect();
                    let blocks = blocks.concat();
                    ok(
                        &id,
                        &json!(format!(
                            "<html><body><h3>Incoming messages</h3><ul>{blocks}</ul></body></html>"
                        )),
                    )
                }
                "get_account_file_size" => ok(&id, &json!(123_456)),
                // One message, as `get_messages` gives: reader page asks before asking whether
                // more of it (download-limited) exists.
                "get_message" => {
                    let msg = positional(1).as_u64().unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    ok(&id, &state.full_message(msg))
                }
                // Whole message sending core cut: seeded cut one only; core answers null for
                // whole ones.
                "get_message_html" => {
                    let msg = positional(1).as_u64().unwrap_or_default();
                    if msg == 11 {
                        ok(&id, &json!(LONG_MESSAGE_HTML))
                    } else {
                        ok(&id, &Value::Null)
                    }
                }
                // Core fetches and announces change; here fetch is instant.
                "download_full_message" => {
                    let account = account_id();
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.downloaded.insert(msg);
                    let chat = state.chat_of(msg);
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "MsgsChanged", "chatId": chat, "msgId": msg},
                    }));
                    ok(&id, &Value::Null)
                }
                // Account's reactions on a message, whole list; empty removes. Announced with
                // core's event; answers id of hidden carrier message (in no chat list).
                "send_reaction" => {
                    let account = account_id();
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let emojis: Vec<String> = positional(2)
                        .as_array()
                        .map(|array| {
                            array
                                .iter()
                                .filter_map(Value::as_str)
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let by_contact = state.reactions.entry(msg).or_default();
                    if emojis.is_empty() {
                        by_contact.remove(&SELF);
                    } else {
                        by_contact.insert(SELF, emojis);
                    }
                    state.next_message_id += 1;
                    let carrier = state.next_message_id;
                    let chat = state.chat_of(msg);
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "ReactionsChanged", "chatId": chat,
                                  "contactId": SELF, "msgId": msg},
                    }));
                    ok(&id, &json!(carrier))
                }
                // Join and 1:1 both end in fresh chat at top.
                "create_chat_by_contact_id" | "secure_join" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.next_chat_id += 1;
                    let chat = state.next_chat_id;
                    state.chats.insert(chat, Vec::new());
                    state.chat_order.insert(0, chat);
                    ok(&id, &json!(chat))
                }
                "create_group_chat" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.next_chat_id += 1;
                    let chat = state.next_chat_id;
                    state.chats.insert(chat, Vec::new());
                    state.chat_order.insert(0, chat);
                    state.group_members.insert(chat, Vec::new());
                    ok(&id, &json!(chat))
                }
                "add_contact_to_chat" => {
                    let account = account_id();
                    let mut state = state.lock().await;
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let contact = positional(2)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let members = state.group_members.entry(chat).or_default();
                    if !members.contains(&contact) {
                        members.push(contact);
                    }
                    state.chat_modified(account, chat);
                    ok(&id, &Value::Null)
                }
                // Chat as `ChatInfo` reads it. Members are ids; contacts come from
                // `get_contacts_by_ids`, keyed by id as strings.
                "get_full_chat_by_id" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let is_group = state.is_group(chat);
                    let left = state.left_groups.contains(&chat);
                    let members = if is_group {
                        state.group_members.get(&chat).cloned().unwrap_or_default()
                    } else {
                        vec![10]
                    };
                    // Core's words for a missing chat.
                    if state.deleted_chats.contains(&chat) {
                        err(
                            &id,
                            &format!("Failed to load chat {chat} from the database"),
                        )
                    } else {
                        ok(
                            &id,
                            &json!({
                                "id": chat,
                                "chatType": if is_group { "Group" } else { "Single" },
                                "name": state.chat_name(chat),
                                "profileImage": state.group_images.get(&chat),
                                "color": "#0071c7",
                                "contactIds": members,
                                // Both false once account left, as core; 1:1 is no group
                                // account is "in".
                                "selfInGroup": is_group && !left,
                                "canSend": !left,
                                // Every chat here is chatmail.
                                "isEncrypted": true,
                                "isSelfTalk": false,
                                "isDeviceChat": false,
                                "ephemeralTimer": state.timers.get(&chat).copied().unwrap_or(0),
                            }),
                        )
                    }
                }
                // Who is in a chat, ids only: header count. Same as `get_full_chat_by_id`.
                "get_chat_contacts" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let members = if state.is_group(chat) {
                        state.group_members.get(&chat).cloned().unwrap_or_default()
                    } else {
                        vec![10]
                    };
                    ok(&id, &json!(members))
                }
                "get_contacts_by_ids" => {
                    let ids: Vec<u32> = positional(1)
                        .as_array()
                        .map(|array| {
                            array
                                .iter()
                                .filter_map(Value::as_u64)
                                .filter_map(|value| u32::try_from(value).ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let mut found = serde_json::Map::new();
                    for contact in ids {
                        if let Some(object) = state.contact_object(contact) {
                            found.insert(contact.to_string(), object);
                        }
                    }
                    ok(&id, &Value::Object(found))
                }
                "set_chat_name" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let name = positional(2).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Core's two refusals: empty name, and chat not a group account is in.
                    if name.is_empty() {
                        err(&id, "Invalid name")
                    } else if !state.is_group(chat) || state.left_groups.contains(&chat) {
                        err(&id, "Failed to set name")
                    } else {
                        state.group_names.insert(chat, name);
                        state.chat_modified(account, chat);
                        ok(&id, &Value::Null)
                    }
                }
                // As core: trimmed; empty clears; refused unless group account is in.
                "get_chat_description" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let seeded = (chat == 2)
                        .then(|| std::env::var("PIIRIT_FAKE_DESCRIPTION").ok())
                        .flatten()
                        .unwrap_or_default();
                    let text = state.descriptions.get(&chat).cloned().unwrap_or(seeded);
                    ok(&id, &json!(text))
                }
                "set_chat_description" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let text = positional(2)
                        .as_str()
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    if !state.is_group(chat) || state.left_groups.contains(&chat) {
                        err(&id, "Cannot set chat description; self not in group")
                    } else if should_fail(&text) {
                        err(&id, "Cannot set chat description")
                    } else {
                        state.descriptions.insert(chat, text);
                        state.chat_modified(account, chat);
                        ok(&id, &Value::Null)
                    }
                }
                "set_chat_profile_image" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Null clears, as core.
                    let path = positional(2);
                    match path.as_str() {
                        Some(path) if should_fail(path) => err(&id, "Copying new blobfile failed"),
                        given => {
                            match given {
                                Some(path) => {
                                    state.group_images.insert(chat, path.to_string());
                                }
                                None => {
                                    state.group_images.remove(&chat);
                                }
                            }
                            state.chat_modified(account, chat);
                            ok(&id, &Value::Null)
                        }
                    }
                }
                "remove_contact_from_chat" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let contact = positional(2)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Removing non-member isn't an error to core either.
                    if let Some(members) = state.group_members.get_mut(&chat) {
                        members.retain(|member| *member != contact);
                    }
                    if contact == SELF {
                        state.left_groups.insert(chat);
                    }
                    state.chat_modified(account, chat);
                    ok(&id, &Value::Null)
                }
                // Verified against core: accepted on any chat; own event carries the timer.
                "set_chat_ephemeral_timer" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let timer = positional(2)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.timers.insert(chat, timer);
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "ChatEphemeralTimerModified", "chatId": chat, "timer": timer},
                    }));
                    ok(&id, &Value::Null)
                }
                "leave_group" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    if let Some(members) = state.group_members.get_mut(&chat) {
                        members.retain(|member| *member != SELF);
                    }
                    state.left_groups.insert(chat);
                    state.chat_modified(account, chat);
                    ok(&id, &Value::Null)
                }
                // Fingerprint code without invite secrets: no kind since 2.61. Core looks for
                // contact with that key, refuses if none: here, always.
                "check_qr"
                    if positional(1).as_str().is_some_and(|content| {
                        content.starts_with("OPENPGP4FPR:")
                            && !(content.contains("i=") && content.contains("s="))
                    }) =>
                {
                    err(
                        &id,
                        "failed to decode OPENPGP4FPR QR code: \
                         Contact matching the fingerprint is not found",
                    )
                }
                "check_qr" => {
                    let content = positional(1).as_str().unwrap_or_default().to_string();
                    ok(&id, &qr_kind(&content))
                }
                "get_chat_securejoin_qr_code" => ok(
                    &id,
                    &json!("https://i.delta.chat/#ABCDEF&a=me%40example.org&n=Me"),
                ),
                // Delete here only, or here and at every end (second sends deletion request as
                // own message). Nothing here delivers, so both do same to chat.
                "delete_messages" | "delete_messages_for_all" => {
                    let ids: Vec<u32> = positional(1)
                        .as_array()
                        .map(|array| {
                            array
                                .iter()
                                .filter_map(Value::as_u64)
                                .filter_map(|value| u32::try_from(value).ok())
                                .collect()
                        })
                        .unwrap_or_default();
                    let account = account_id();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    for messages in state.chats.values_mut() {
                        messages.retain(|msg| !ids.contains(msg));
                    }
                    // Core announces deletion; model reloads on it.
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "MsgsChanged", "chatId": 0, "msgId": 0},
                    }));
                    ok(&id, &Value::Null)
                }
                "delete_chat" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.chats.remove(&chat);
                    state.chat_order.retain(|id| *id != chat);
                    state.deleted_chats.insert(chat);
                    ok(&id, &Value::Null)
                }
                "get_basic_chat_info" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    ok(
                        &id,
                        &json!({
                            "id": chat,
                            "chatType": if state.is_group(chat) { "Group" } else { "Single" },
                            "name": state.chat_name(chat),
                            "isEncrypted": true,
                            "isMuted": state.muted.contains(&chat),
                        }),
                    )
                }
                // Can write into chat: not into a group left, as `get_full_chat_by_id`.
                "can_send" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    ok(&id, &json!(!state.left_groups.contains(&chat)))
                }
                // Replace own message text. Core refuses empty and others'; menu offers neither,
                // so only empty refused here, `fail` text fails like a send. Announced as core
                // does, so model re-reads row.
                "send_edit_request" => {
                    let account = account_id();
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let text = positional(2).as_str().unwrap_or_default().to_string();
                    if text.trim().is_empty() {
                        err(&id, "Edited text cannot be empty")
                    } else if should_fail(&text) {
                        err(&id, "could not send")
                    } else {
                        let mut state = state.lock().await;
                        state.seed_chats();
                        let chat = state.chat_of(msg);
                        state.edits.insert(msg, text);
                        state.events.push_back(json!({
                            "contextId": account,
                            "event": {"kind": "MsgsChanged", "chatId": chat, "msgId": msg},
                        }));
                        ok(&id, &Value::Null)
                    }
                }
                // Messages a deletion period would take now: all older than it in every chat;
                // with seeded timestamps, all for any period a page offers.
                "estimate_auto_deletion_count" => {
                    let seconds = positional(2).as_i64().unwrap_or_default();
                    let cutoff = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| {
                            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
                        })
                        .saturating_sub(seconds);
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let count = state
                        .chats
                        .values()
                        .flatten()
                        .filter(|msg| message_timestamp(u64::from(**msg)) < cutoff)
                        .count();
                    ok(&id, &json!(count))
                }
                // Mute or unmute. Core keeps duration, announces `ChatModified`; list reads
                // `isMuted` off row.
                "set_chat_mute_duration" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let kind = positional(2)
                        .get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    if kind == "NotMuted" {
                        state.muted.remove(&chat);
                    } else {
                        state.muted.insert(chat);
                    }
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "ChatModified", "chatId": chat},
                    }));
                    ok(&id, &Value::Null)
                }
                // One contact by id, own included: profile colour lookup.
                "get_contact" => {
                    let contact = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    match state.contact_object(contact) {
                        Some(object) => ok(&id, &object),
                        None => err(&id, "contact not found"),
                    }
                }
                "get_chatlist_entries" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // `DC_GCL_ARCHIVED_ONLY`: lists disjoint, so this picks a list, not a filter.
                    let archived_only = positional(1).as_u64().unwrap_or(0) & 0x01 != 0;
                    // Verified vs core: `ARCHIVED_ONLY` ignores query; plain query searches all,
                    // archived included. Fake filtering archived by query would let one-call
                    // implementation pass here, fail on device.
                    let query = positional(2).as_str().unwrap_or("").to_string();
                    let searching = !query.is_empty();
                    let entries = if archived_only {
                        state.archived_order.clone()
                    } else if searching {
                        let mut all = state.chat_order.clone();
                        all.extend(state.archived_order.iter().copied());
                        all
                    } else {
                        state.chat_order.clone()
                    };
                    // Make *ordinary* listing slow, so answer model moved on from arrives last.
                    // Requests concurrent, as core's: delays only own reply.
                    if !archived_only {
                        if let Some(delay) = std::env::var("PIIRIT_FAKE_CHATLIST_DELAY_MS")
                            .ok()
                            .and_then(|value| value.parse::<u64>().ok())
                        {
                            drop(state);
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                    }
                    // Query matched as core does, on the name a chat item reports.
                    let entries = if searching && !archived_only {
                        let needle = query.to_lowercase();
                        entries
                            .into_iter()
                            .filter(|chat| format!("chat {chat}").contains(&needle))
                            .collect()
                    } else {
                        entries
                    };
                    ok(&id, &json!(entries))
                }
                "get_chatlist_items_by_entries" => {
                    // Core builds rows slowly per batch; delay lets tests see first batch.
                    tokio::time::sleep(delay("PIIRIT_FAKE_CHATLIST_ITEMS_DELAY_MS")).await;
                    let account = account_id();
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    let mut items = serde_json::Map::new();
                    for chat in ids {
                        let fresh = u32::try_from(chat)
                            .is_ok_and(|chat| state.fresh.contains(&(account, chat)));
                        // Draft-holding chat previews draft: "Draft" prefix, DC_STATE_OUT_DRAFT.
                        // Pinned in `deltachat-jsonrpc/tests/real_server.rs`.
                        let draft = u32::try_from(chat)
                            .ok()
                            .and_then(|chat| state.drafts.get(&chat))
                            .filter(|text| !text.is_empty());
                        items.insert(
                            chat.to_string(),
                            json!({
                                "kind": "ChatListItem",
                                "name": format!("chat {chat}"),
                                "summaryText1": draft.map(|_| "Draft"),
                                "summaryText2": draft.map_or_else(
                                    || format!("last in {chat}"),
                                    Clone::clone,
                                ),
                                "summaryStatus": if draft.is_some() { 19 } else { 0 },
                                "freshMessageCounter": u32::from(fresh),
                                "isEncrypted": true,
                                "isMuted": u32::try_from(chat)
                                    .is_ok_and(|chat| state.muted.contains(&chat)),
                                // Self and device chats, when named: cover leaves them out.
                                "isSelfTalk": env_ids("PIIRIT_FAKE_SELF_TALK").contains(&chat),
                                "isDeviceTalk": env_ids("PIIRIT_FAKE_DEVICE_TALK").contains(&chat),
                                // Chat 1 pinned: ordinary list has both kinds; archived has one.
                                "isPinned": chat == 1,
                            }),
                        );
                    }
                    ok(&id, &Value::Object(items))
                }
                "search_messages" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Three args: third is chat to search (null = all). Pinned binary rejects two.
                    let needle = positional(1).as_str().unwrap_or_default().to_lowercase();
                    let within = positional(2)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok());
                    let mut hits: Vec<u32> = Vec::new();
                    for (chat, messages) in &state.chats {
                        if within.is_some_and(|wanted| wanted != *chat) {
                            continue;
                        }
                        for msg in messages {
                            let text = format!("message {msg}");
                            if !needle.is_empty() && text.contains(&needle) {
                                hits.push(*msg);
                            }
                        }
                    }
                    ok(&id, &json!(hits))
                }
                "get_message_list_items" => {
                    // Delayed like get_messages; tests with two in flight need overlap.
                    tokio::time::sleep(delay("PIIRIT_FAKE_FETCH_DELAY_MS")).await;
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    // Fourth arg asks for day markers, one before each day's first message, as
                    // core; model reads placeholder row's day off them.
                    let markers = positional(3).as_bool().unwrap_or(false);
                    let mut items: Vec<Value> = Vec::new();
                    let mut day = None;
                    for msg in state.chats.get(&chat).cloned().unwrap_or_default() {
                        if markers {
                            let start = day_start(message_timestamp(u64::from(msg)));
                            if day != Some(start) {
                                items.push(json!({"kind": "dayMarker", "timestamp": start}));
                                day = Some(start);
                            }
                        }
                        items.push(json!({"kind": "message", "msg_id": msg}));
                    }
                    ok(&id, &Value::Array(items))
                }
                // Ids of messages of up to three kinds in chat (null chat: all), oldest first,
                // as core answers and asks not re-sorted. Media pages use it.
                "get_chat_media" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok());
                    let wanted: Vec<String> = (2..=4)
                        .filter_map(|index| positional(index).as_str().map(str::to_string))
                        .collect();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let mut ids: Vec<u32> = state
                        .chats
                        .iter()
                        .filter(|(id, _)| chat.map_or(true, |chat| **id == chat))
                        .flat_map(|(_, messages)| messages.iter().copied())
                        .filter(|msg| {
                            let message = state.full_message(u64::from(*msg));
                            let kind = message
                                .get("viewType")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            wanted.iter().any(|wanted| wanted == kind)
                        })
                        .collect();
                    ids.sort_unstable();
                    ok(&id, &json!(ids))
                }
                "get_messages" => {
                    tokio::time::sleep(delay("PIIRIT_FAKE_FETCH_DELAY_MS")).await;
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    // Multi-id page as chat opens (not single-message send readback): phone is
                    // slow here with long picture chat, while own send is answered.
                    if ids.len() > 1 {
                        tokio::time::sleep(delay("PIIRIT_FAKE_PAGE_DELAY_MS")).await;
                    }
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let mut loaded = serde_json::Map::new();
                    for msg in ids {
                        loaded.insert(msg.to_string(), state.full_message(msg));
                    }
                    ok(&id, &Value::Object(loaded))
                }
                // Drafts per chat, as core: one text per chat, read back and cleared.
                "misc_set_draft" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let text = positional(2).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    state.drafts.insert(chat, text);
                    ok(&id, &Value::Null)
                }
                "remove_draft" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.drafts.remove(&chat);
                    ok(&id, &Value::Null)
                }
                "get_draft" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    // Null for none, whole message object for one, as core.
                    match state.drafts.get(&chat) {
                        Some(text) => ok(&id, &json!({"text": text, "state": 19})),
                        None => ok(&id, &Value::Null),
                    }
                }
                "misc_send_msg" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let text = positional(2).as_str().unwrap_or_default().to_string();
                    // Core picks view type from the file; echoed back for sender's row.
                    let file = positional(3);
                    let file_name = positional(4);
                    let quoted = positional(6);
                    if should_fail(&text) {
                        // Failed send reports an Error event, not only a failed call.
                        state.lock().await.events.push_back(json!({
                            "contextId": account,
                            "event": {"kind": "Error", "msg": "could not send"},
                        }));
                        err(&id, "could not send")
                    } else {
                        let msg = {
                            let mut state = state.lock().await;
                            let msg = state.add_message(account, chat);
                            state.note_quote(msg, &quoted);
                            msg
                        };
                        // Queued above: delay here puts event ahead of reply, an order core can
                        // produce, and one that duplicated a sent row.
                        tokio::time::sleep(delay("PIIRIT_FAKE_SEND_DELAY_MS")).await;
                        // Core reads file; this reads extension: enough for image vs paperclip row.
                        let extension = file.as_str().and_then(|path| {
                            std::path::Path::new(path)
                                .extension()
                                .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
                        });
                        let view_type = match extension.as_deref() {
                            _ if file.is_null() => "Text",
                            Some("png" | "jpg" | "jpeg") => "Image",
                            Some("xdc") => "Webxdc",
                            _ => "File",
                        };
                        // Remembered as `send_msg` does: refetched row must stay an app message.
                        if view_type == "Webxdc" {
                            state.lock().await.sent_files.insert(
                                msg,
                                Sent {
                                    file: file.clone(),
                                    view_type: view_type.to_string(),
                                    text: None,
                                    file_name: None,
                                },
                            );
                        }
                        ok(
                            &id,
                            &json!([
                                msg,
                                {"text": text, "fromId": 1, "timestamp": 0,
                                 "showPadlock": true, "state": 20,
                                 "file": file, "fileName": file_name,
                                 "viewType": view_type}
                            ]),
                        )
                    }
                }
                // Other send: MessageData, for view type core wouldn't pick (voice). Answers id
                // only; row fetched after, `get_messages` names asked type.
                "send_msg" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let data = positional(2);
                    let file = data.get("file").cloned().unwrap_or(Value::Null);
                    let view_type = data
                        .get("viewtype")
                        .and_then(Value::as_str)
                        .unwrap_or("Text")
                        .to_string();
                    let text = data
                        .get("text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    let file_name = data
                        .get("filename")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    if file.is_null() {
                        err(&id, "send_msg without a file is not what the app sends")
                    } else {
                        // Held back like other send: in-flight tests cover both.
                        tokio::time::sleep(delay("PIIRIT_FAKE_SEND_DELAY_MS")).await;
                        let mut state = state.lock().await;
                        let msg = state.add_message(account, chat);
                        state.sent_files.insert(
                            msg,
                            Sent {
                                file,
                                view_type,
                                text,
                                file_name,
                            },
                        );
                        state.note_quote(msg, data.get("quotedMessageId").unwrap_or(&Value::Null));
                        ok(&id, &json!(msg))
                    }
                }
                // Off-web fetch by core, not app: how an app is taken from store.
                "get_http_response" => {
                    let url = positional(1).as_str().unwrap_or_default().to_string();
                    if should_fail(&url) {
                        err(&id, "could not reach the store")
                    } else {
                        ok(
                            &id,
                            &json!({
                                "blob": base64(b"PK\x03\x04 a fake app"),
                                "mimetype": "application/octet-stream",
                                "encoding": Value::Null,
                            }),
                        )
                    }
                }
                // App's name and self-description. Summary counts updates sent, so test can
                // watch row follow chat.
                "get_webxdc_info" => {
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let sent = state
                        .lock()
                        .await
                        .webxdc_updates
                        .get(&msg)
                        .map_or(0, Vec::len);
                    ok(
                        &id,
                        &json!({
                            "name": "Checkers",
                            "icon": "icon.png",
                            "document": null,
                            "summary": if sent == 0 {
                                Value::Null
                            } else {
                                json!(format!("{sent} move(s)"))
                            },
                            "sourceCodeUrl": "https://example.org/checkers",
                            "internetAccess": false,
                            "selfAddr": "self@example.org",
                            "isAppSender": true,
                            "isBroadcast": false,
                            "sendUpdateInterval": 1000,
                            "sendUpdateMaxSize": 102_400,
                        }),
                    )
                }
                // One archive file, base64 as core encodes (STANDARD_NO_PAD). Page and icon only.
                "get_webxdc_blob" => {
                    let path = positional(2).as_str().unwrap_or_default().to_string();
                    match webxdc_file(&path) {
                        Some(bytes) => ok(&id, &json!(base64(&bytes))),
                        None => err(&id, "no such file in the webxdc"),
                    }
                }
                // Updates after serial, as one JSON *string* (core hands it over encoded).
                "get_webxdc_status_updates" => {
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let serial = positional(2)
                        .as_u64()
                        .and_then(|value| usize::try_from(value).ok())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    let updates = state.webxdc_updates.get(&msg).cloned().unwrap_or_default();
                    let after: Vec<Value> = updates.iter().skip(serial).cloned().collect();
                    ok(
                        &id,
                        &json!(serde_json::to_string(&after).unwrap_or_default()),
                    )
                }
                "send_webxdc_status_update" => {
                    let account = account_id();
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let update: Value = positional(2)
                        .as_str()
                        .and_then(|text| serde_json::from_str(text).ok())
                        .unwrap_or(Value::Null);
                    if update.is_null() {
                        err(&id, "the update is not JSON")
                    } else {
                        let mut state = state.lock().await;
                        let kept = state.webxdc_updates.entry(msg).or_default();
                        let serial = kept.len() + 1;
                        // Core hands an update back with both serials; own send is one received.
                        kept.push(json!({
                            "payload": update.get("payload").cloned().unwrap_or(Value::Null),
                            "serial": serial,
                            "max_serial": serial,
                        }));
                        state.events.push_back(json!({
                            "contextId": account,
                            "event": {
                                "kind": "WebxdcStatusUpdate",
                                "msgId": msg,
                                "statusUpdateSerial": serial,
                            },
                        }));
                        ok(&id, &Value::Null)
                    }
                }
                // Call methods, as core does (src/calls.rs upstream): every state move announced.
                "ice_servers" => ok(
                    &id,
                    &json!(
                        r#"[{"urls":["turn:127.0.0.1:3478"],"username":"fake","credential":"fake"}]"#
                    ),
                ),
                "place_outgoing_call" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let offer = positional(2).as_str().unwrap_or_default().to_string();
                    let has_video = positional(3).as_bool().unwrap_or(false);
                    let mut state = state.lock().await;
                    state.seed_chats();
                    if state.is_group(chat) {
                        err(&id, "Can only place calls in single chats")
                    } else {
                        let answer_me = offer.contains("answer-me");
                        let call = state.add_call(
                            account,
                            chat,
                            FakeCall {
                                incoming: false,
                                has_video,
                                offer,
                                accepted: false,
                                state: "Alerting",
                            },
                        );
                        // Offer asking for it answered at once, as other end's device would.
                        if answer_me {
                            if let Some(placed) = state.calls.get_mut(&call) {
                                placed.accepted = true;
                                placed.state = "Active";
                            }
                            state.call_changed(account, call);
                            state.call_event(
                                account,
                                "OutgoingCallAccepted",
                                call,
                                &json!({"accept_call_info": "v=0 fake-answer"}),
                            );
                        }
                        ok(&id, &json!(call))
                    }
                }
                "accept_incoming_call" => {
                    let account = account_id();
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    match state.calls.get_mut(&msg) {
                        Some(call) if call.incoming => {
                            call.accepted = true;
                            call.state = "Active";
                            state.call_event(
                                account,
                                "IncomingCallAccepted",
                                msg,
                                &json!({"from_this_device": true}),
                            );
                            state.call_changed(account, msg);
                            ok(&id, &Value::Null)
                        }
                        _ => err(&id, "accept_incoming_call is called with a message that is not an incoming call"),
                    }
                }
                "end_call" => {
                    let account = account_id();
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    let ended = match state.calls.get_mut(&msg) {
                        None => None,
                        Some(call) if matches!(call.state, "Alerting" | "Active") => {
                            call.state = match (call.accepted, call.incoming) {
                                (true, _) => "Completed",
                                (false, true) => "Declined",
                                (false, false) => "Canceled",
                            };
                            Some(true)
                        }
                        // Ended already: core logs it, does nothing.
                        Some(_) => Some(false),
                    };
                    match ended {
                        None => err(&id, "end_call is called with a message that is not a call"),
                        Some(moved) => {
                            if moved {
                                state.call_event(account, "CallEnded", msg, &json!({}));
                                state.call_changed(account, msg);
                                // Hidden message telling other end; app waits on it ending a call.
                                state.events.push_back(json!({
                                    "contextId": account,
                                    "event": {
                                        "kind": "SmtpMessageSent",
                                        "msg": "Message len=512 was SMTP-sent to 1 recipients.",
                                    },
                                }));
                            }
                            ok(&id, &Value::Null)
                        }
                    }
                }
                "call_info" => {
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    match state.lock().await.calls.get(&msg) {
                        Some(call) => ok(&id, &call.info()),
                        None => err(&id, "Attempting to get call state of non-call message"),
                    }
                }
                "get_next_event_batch" => {
                    // Blocks when empty, like real long poll.
                    loop {
                        let queued: Vec<Value> = state.lock().await.events.drain(..).collect();
                        if !queued.is_empty() {
                            // Held back after batch taken; before the wait it would buy nothing.
                            // Lets a test order call reply before its event; else Qt-thread
                            // race, passing on one machine, failing on another.
                            tokio::time::sleep(delay("PIIRIT_FAKE_EVENT_DELAY_MS")).await;
                            break ok(&id, &Value::Array(queued));
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    }
                }
                _ => err(&id, "method not found"),
            };

            let mut out = stdout.lock().await;
            let _ = out.write_all(response.to_string().as_bytes()).await;
            let _ = out.write_all(b"\n").await;
            let _ = out.flush().await;
        });
    }
}

fn ok(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn err(id: &Value, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}})
}
