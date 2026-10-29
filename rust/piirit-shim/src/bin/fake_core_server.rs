//! Fake `deltachat-rpc-server`: journals each request as a JSON line to `PIIRIT_FAKE_JOURNAL`
//! (tests assert calls, order, params). Keyed on input, not env, so one process serves success
//! and failure: QR payload or address containing `fail` is rejected.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;

use chrono::{Local, TimeZone};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// `DC_CONTACT_ID_SELF`: own contact.
const SELF: u32 = 1;

/// Max recommended attachment: key, and core's answer `(30 - 1) MiB * 3/4` (30 MiB relay,
/// less 1 MiB headers, less base64). Computed, not stored.
const RECOMMENDED_SIZE_KEY: &str = "sys.msgsize_max_recommended";
const RECOMMENDED_SIZE: u64 = (30 - 1) * 1024 * 1024 / 4 * 3;

/// Own address key core 2.66 dropped; read or written, refused (pinned in `real_server.rs`).
const REMOVED_KEY: &str = "configured_addr";

/// `PIIRIT_FAKE_MSGSIZE_MAX` lowers it: oversize tests write KiB, not ~20 MiB.
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

/// Call, as core keeps it on its message.
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
    /// Core's text, for rows not reading state.
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

    /// `call_info` as core: `camelCase`, state tagged `kind`, duration once completed.
    fn info(&self) -> Value {
        let state = if self.state == "Completed" {
            json!({"kind": "Completed", "duration": 185})
        } else {
            json!({"kind": self.state})
        };
        json!({"sdpOffer": self.offer, "hasVideo": self.has_video, "state": state})
    }
}

/// `send_msg` `MessageData` fields row must carry back.
struct Sent {
    file: Value,
    view_type: String,
    text: Option<String>,
    file_name: Option<String>,
}

#[derive(Default)]
struct State {
    accounts: Vec<Account>,
    /// Highest account id given out; never reused.
    last_account: u32,
    /// Profile app last said it shows. Core keeps it on disk, selects new account itself, falls
    /// back to first left on removal.
    selected: Option<u32>,
    /// Events for `get_next_event_batch`.
    events: VecDeque<Value>,
    /// Message ids per chat, oldest first; seeded on first use.
    chats: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Chat ids in display order, newest first.
    chat_order: Vec<u32>,
    /// Archived chats; disjoint from `chat_order`.
    archived_order: Vec<u32>,
    next_message_id: u32,
    /// Contact id -> address; two seeded.
    contacts: std::collections::BTreeMap<u32, String>,
    next_chat_id: u32,
    /// Group members by chat (entry = group); seeded one includes self, as core.
    group_members: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Group names set after creation.
    group_names: std::collections::BTreeMap<u32, String>,
    /// Group descriptions; `PIIRIT_FAKE_DESCRIPTION` sets seeded one.
    descriptions: std::collections::BTreeMap<u32, String>,
    /// Group pictures: given path, not core's copy.
    group_images: std::collections::BTreeMap<u32, String>,
    /// Left groups: core refuses all changes.
    left_groups: std::collections::BTreeSet<u32>,
    /// Deleted chats: core can't load.
    deleted_chats: std::collections::BTreeSet<u32>,
    /// Contact requests accepted here; requests are `PIIRIT_FAKE_REQUESTS` chats.
    accepted_requests: std::collections::BTreeSet<u32>,
    /// Channels joined here by invite; seeded ones are `PIIRIT_FAKE_CHANNELS` chats.
    joined_channels: std::collections::BTreeSet<u32>,
    /// Channels made here (core's `OutBroadcast`); seeded ones are `PIIRIT_FAKE_OWN_CHANNELS`.
    own_channels: std::collections::BTreeSet<u32>,
    /// Accounts with unanswered transport call: core allows one, refuses second.
    configuring: std::collections::BTreeSet<u32>,
    /// Accounts asked to stop ongoing process.
    stopped: std::collections::BTreeSet<u32>,
    /// IO stopped, not restarted: reports not connected, no relays.
    io_stopped: bool,
    /// Seconds until chat messages vanish.
    timers: std::collections::BTreeMap<u32, u32>,
    /// Config values per account.
    config: std::collections::BTreeMap<(u32, String), String>,
    /// Unsent draft text per chat.
    drafts: std::collections::BTreeMap<u32, String>,
    /// Contact nicknames; empty restores theirs.
    contact_names: std::collections::BTreeMap<u32, String>,
    /// Blocked: out of `get_contacts` whatever flags; only `get_blocked_contacts` lists them.
    blocked: std::collections::BTreeSet<u32>,
    /// Messages whose remainder was requested; see `downloadState`.
    downloaded: std::collections::BTreeSet<u32>,
    /// Chats marked unread from list, per account; cleared when noticed. Flag only, no count.
    fresh: std::collections::BTreeSet<(u32, u32)>,
    /// Reactions by message, then contact (one list each); own send replaces own list.
    reactions: std::collections::BTreeMap<u32, std::collections::BTreeMap<u32, Vec<String>>>,
    /// What `send_msg` handed over, by message.
    sent_files: std::collections::BTreeMap<u32, Sent>,
    /// Webxdc updates per instance, send order, serial from 1; all after asked serial, as core.
    webxdc_updates: std::collections::BTreeMap<u32, Vec<Value>>,
    /// Text edits by message; marks edited.
    edits: std::collections::BTreeMap<u32, String>,
    /// Muted chats: flag, not duration.
    muted: std::collections::BTreeSet<u32>,
    /// Message each sent message quotes.
    quotes: std::collections::BTreeMap<u32, u32>,
    /// Sent here, send order. Own only if test asks; see `full_message`.
    sent: std::collections::BTreeSet<u32>,
    /// Calls by message; placed here or via `PIIRIT_FAKE_INCOMING_CALL_MS`.
    calls: std::collections::BTreeMap<u32, FakeCall>,
    /// Transports per account, add order (core's `transports` table, `list_transports`).
    /// Own address = first's, as core 2.66+ (own contact's address).
    transports: std::collections::BTreeMap<u32, Vec<String>>,
    /// Chats getting reader's location live, per account.
    sharing: std::collections::BTreeSet<(u32, u32)>,
    /// Of those, ones asked for under a minute: run out unannounced by next position, as a
    /// core whose `ChatModified` got lost.
    expiring: std::collections::BTreeSet<(u32, u32)>,
    /// Points sent here once, by message: latitude, longitude.
    points: std::collections::BTreeMap<u32, (f64, f64)>,
    /// Map app (webxdc integration) per account: its message.
    integrations: std::collections::BTreeMap<u32, u32>,
    /// Map app's chat (0: all), as core's `WebxdcIntegrateFor`.
    integrated_for: std::collections::BTreeMap<u32, u32>,
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

    /// Keep new transport once, as core (second = second address, not change of first).
    fn add_transport(&mut self, account: u32, addr: &str) {
        let list = self.transports.entry(account).or_default();
        if !list.iter().any(|known| known == addr) {
            list.push(addr.to_string());
        }
        self.transports_modified(account);
    }

    /// Configured profiles from `PIIRIT_FAKE_ACCOUNTS` (unset in onboarding tests), seeded before
    /// first request so early profile asks find transports. `PIIRIT_FAKE_OLDER_RELAY` seeds relay
    /// set up before `account<N>@example.org`; core 2.66+ makes it own address.
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
            list.push(format!("account{account}@example.org"));
        }
    }

    /// Core's event on account transport change.
    fn transports_modified(&mut self, account: u32) {
        self.events.push_back(json!({
            "contextId": account,
            "event": {"kind": "TransportsModified"},
        }));
    }

    /// Accounts as core: configured carries profile (name, picture, colour), no address (dropped
    /// in 2.61); unconfigured id only.
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

    /// Two seeded chats: paging, adding one more.
    fn seed_chats(&mut self) {
        if self.chats.is_empty() {
            // Long chat for paging, on request. Ids from 1, newest highest; seeded quote, picture
            // keep ids.
            let long = std::env::var("PIIRIT_FAKE_LONG_CHAT")
                .ok()
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|count| *count > 2);
            self.chats.insert(
                1,
                long.map_or_else(|| vec![1, 2], |count| (1..=count).collect()),
            );
            self.chats.insert(2, vec![10]);
            // Ada's point sent once, then her live sharing's start notice.
            if std::env::var_os("PIIRIT_FAKE_LOCATIONS").is_some() {
                self.chats.insert(1, vec![1, 2, 30, 31]);
            }
            // Other media kinds on request: file, voice, app, video.
            if std::env::var_os("PIIRIT_FAKE_MEDIA").is_some() {
                self.chats.insert(2, vec![10, 12, 13, 14, 15]);
            }
            // Not yet downloaded too (`pre_message`), plus plain text. 16 sent before 14 and 15
            // (conversation order, not id): gallery must merge by it.
            if std::env::var_os("PIIRIT_FAKE_PRE_MESSAGES").is_some() {
                self.chats
                    .insert(2, vec![10, 12, 13, 16, 14, 15, 17, 18, 19, 20]);
            }
            self.chat_order = vec![1, 2];
            // Extra empty chats from 201, on request.
            let many = std::env::var("PIIRIT_FAKE_MANY_CHATS")
                .ok()
                .and_then(|count| count.parse::<u32>().ok())
                .unwrap_or(0);
            for chat in 201..201u32.saturating_add(many) {
                self.chats.insert(chat, Vec::new());
                self.chat_order.push(chat);
            }
            // Chat 3 archived: else both lists alike.
            self.chats.insert(3, vec![30]);
            // Empty archive: page hides search.
            self.archived_order = if std::env::var("PIIRIT_FAKE_NO_ARCHIVED").is_ok() {
                Vec::new()
            } else {
                vec![3]
            };
            // Above seeded ids: no collision.
            self.next_message_id = long.unwrap_or(0).max(100);
            self.contacts.insert(10, "ada@example.org".to_string());
            self.contacts.insert(11, "grace@example.org".to_string());
            // Chat 2 group: self + one contact; test adds other, removes this one.
            self.group_members.insert(2, vec![SELF, 10]);
            self.next_chat_id = 500;
            // Received replies, on request: `PIIRIT_FAKE_QUOTES=120:1,119:10` (message:quoted).
            if let Ok(pairs) = std::env::var("PIIRIT_FAKE_QUOTES") {
                for pair in pairs.split(',') {
                    let mut ids = pair.split(':').filter_map(|id| id.parse::<u32>().ok());
                    if let (Some(message), Some(quoted)) = (ids.next(), ids.next()) {
                        self.quotes.insert(message, quoted);
                    }
                }
            }
            // Others' reaction on first message, on request: non-own chip, ours counts up.
            if std::env::var_os("PIIRIT_FAKE_REACTED").is_some() {
                self.reactions
                    .entry(1)
                    .or_default()
                    .insert(10, vec!["👍".to_string()]);
            }
        }
    }

    /// Reactions as core: counted, most frequent first; null if none. Pinned in
    /// `deltachat-jsonrpc/tests/real_server.rs`.
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

    /// Contact as `get_contacts`: own unlisted, askable by id (members). Ada has bio. Names:
    /// `authName` theirs, `name` given here, `displayName` `name` else `authName`. Every 2.62 key,
    /// unseen values (`lastSeen` 0, "Old"; own "Normal"). Picture only via
    /// `PIIRIT_FAKE_CONTACT_PICTURE` (Ada, core-style path). Own address = first relay's;
    /// `me@example.org` without one (core 2.66 refuses there; chat tests seed no relay).
    fn contact_object(&self, account: u32, contact: u32) -> Option<Value> {
        let own = self.transports.get(&account).and_then(|list| list.first());
        let address = if contact == SELF {
            own.map_or("me@example.org", String::as_str)
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

    /// Unaccepted contact request: named in `PIIRIT_FAKE_REQUESTS`, not accepted since.
    fn is_request(&self, chat: u32) -> bool {
        env_ids("PIIRIT_FAKE_REQUESTS").contains(&u64::from(chat))
            && !self.accepted_requests.contains(&chat)
    }

    /// Channel read here (core's `InBroadcast`): joined, or named in `PIIRIT_FAKE_CHANNELS`.
    fn is_channel(&self, chat: u32) -> bool {
        self.joined_channels.contains(&chat)
            || env_ids("PIIRIT_FAKE_CHANNELS").contains(&u64::from(chat))
    }

    /// Own channel: made here, or named in `PIIRIT_FAKE_OWN_CHANNELS`.
    fn is_own_channel(&self, chat: u32) -> bool {
        self.own_channels.contains(&chat)
            || env_ids("PIIRIT_FAKE_OWN_CHANNELS").contains(&u64::from(chat))
    }

    /// Core's `chatType`.
    fn chat_type(&self, chat: u32) -> &'static str {
        if self.is_channel(chat) {
            "InBroadcast"
        } else if self.is_own_channel(chat) {
            "OutBroadcast"
        } else if self.is_group(chat) {
            "Group"
        } else {
            "Single"
        }
    }

    /// Announce name, picture, member change, as core.
    fn chat_modified(&mut self, account_id: u32, chat: u32) {
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ChatModified", "chatId": chat},
        }));
    }

    /// Message's chat; search results need it.
    fn chat_of(&self, message_id: u32) -> u32 {
        self.chats
            .iter()
            .find(|(_, messages)| messages.contains(&message_id))
            .map_or(0, |(chat, _)| *chat)
    }

    /// One message: seeded shape plus chat, sent data, edit, quote, download hold; shared by
    /// `get_message`/`get_messages` so they agree. Sent-here is own only under
    /// `PIIRIT_FAKE_SELF_SENT`: suite relies on sends returning as someone else's.
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
        if self.points.contains_key(&id) {
            message["hasLocation"] = json!(true);
            message["fromId"] = json!(SELF);
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
            // Core's `WithMessage` quote: id, chat, text, author.
            message["quote"] = json!({
                "kind": "WithMessage",
                "messageId": quoted,
                "chatId": self.chat_of(*quoted),
                "text": wordy(u64::from(*quoted)),
                "authorDisplayName": "Ada Lovelace",
            });
        }
        // Download-limit hold until remainder requested.
        let held_back = std::env::var("PIIRIT_FAKE_HELD_BACK_MSG")
            .ok()
            .and_then(|value| value.parse::<u64>().ok());
        let fetched = self.downloaded.contains(&id);
        message["downloadState"] = json!(if held_back == Some(msg) && !fetched {
            "Available"
        } else {
            "Done"
        });
        if std::env::var_os("PIIRIT_FAKE_PRE_MESSAGES").is_some() {
            pre_message(&mut message, msg, fetched);
        }
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

    /// Append message to chat, announce (send or arrival).
    fn add_message(&mut self, account_id: u32, chat_id: u32) -> u32 {
        self.seed_chats();
        self.next_message_id += 1;
        let id = self.next_message_id;
        self.chats.entry(chat_id).or_default().push(id);
        // To top: list reorders.
        self.chat_order.retain(|chat| *chat != chat_id);
        self.chat_order.insert(0, chat_id);
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "IncomingMsg", "chatId": chat_id, "msgId": id},
        }));
        // Core follows in same ms (pinned: `MsgDelivered`, `ChatlistItemChanged`), plus
        // `ChatlistChanged` if order changed. Model announcing only on first event's refresh never
        // announced: next event staled it.
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ChatlistItemChanged", "chatId": chat_id},
        }));
        id
    }

    /// Put message one before end, announce. Core sorts arrival below newest *seen*, no further:
    /// one dated before unread tail lands among it (late group message, sync).
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

    /// Call's message in chat, as core (placed or incoming). `MsgsChanged`: ringing call is not
    /// `IncomingMsg`.
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

    /// Call state move: `MsgsChanged` for its message.
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

    /// Queue imex progress: 0 failure, 1000 done.
    fn imex(&mut self, account_id: u32, progress: u32) {
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ImexProgress", "progress": progress},
        }));
    }

    /// Configure account; permille progress, then 1000.
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

/// Start time, journal clock.
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn journal(method: &str, params: &Value) {
    let Ok(path) = std::env::var("PIIRIT_FAKE_JOURNAL") else {
        return;
    };
    // Polling noise would bury sequence.
    if method == "get_next_event_batch" {
        return;
    }
    // Ms since start: call-overlap tests can't use Qt timer (too coarse, two probes at once).
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

/// Message text, short by default (tests assert it). `PIIRIT_FAKE_WORDY` wraps: height then
/// tracks draw width, only way to resize a laid-out row (view holding place).
fn wordy(msg: u64) -> String {
    let text = format!("message {msg}");
    if std::env::var_os("PIIRIT_FAKE_WORDY").is_none() {
        return text;
    }
    format!("{text}, and then a good deal more of it, long enough that where it wraps depends on how wide the row is drawn")
}

/// Send time: 2023-11-14T22:13:20Z, a day later, for day separator. Own fn: placeholder row
/// takes day from marker, filled row from message; must agree.
fn message_timestamp(msg: u64) -> i64 {
    if msg == 1 {
        1_700_000_000
    } else {
        1_700_090_000
    }
}

/// Local midnight of `timestamp`'s day, as core's markers. Checked in three zones.
fn day_start(timestamp: i64) -> i64 {
    let Some(when) = Local.timestamp_opt(timestamp, 0).single() else {
        return timestamp.div_euclid(86_400) * 86_400;
    };
    when.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| midnight.and_local_timezone(Local).earliest())
        .map_or(timestamp, |midnight| midnight.timestamp())
}

/// Fake archive: page, script, icon. Script fetched by absolute dir path, as bundlers write
/// (half the store); host serving under prefix answers nothing.
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

/// Fake map app: tile address as upstream's `index.js` writes it; other app's never touched.
fn map_file(path: &str) -> Option<Vec<u8>> {
    match path {
        "index.html" => Some(
            b"<html><head></head><body><script src=\"index.js\"></script></body></html>".to_vec(),
        ),
        "index.js" => {
            Some(b"let url = 'https://tile.openstreetmap.org/{z}/{x}/{y}.png';\n".to_vec())
        }
        "notes.txt" => Some(b"https://tile.openstreetmap.org/ stays\n".to_vec()),
        _ => None,
    }
}

/// Standard base64, unpadded, as core encodes blobs.
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

/// Message shaped like core's. Seeded 1 quotes, 2 unread, 10 image: one fetch covers view.
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
        // Seeded and test-added messages unread: arrivals worth covering.
        "state": if msg == 2 || msg > 100 { 10 } else { 16 },
        // Real chat's first message: core's "end-to-end encrypted" notice, which day heading was
        // reported drawn over.
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
    // `PIIRIT_FAKE_LOCATIONS`: point sent once, live sharing's start notice. Ada's both.
    let locations = std::env::var_os("PIIRIT_FAKE_LOCATIONS").is_some();
    if locations && msg == 30 {
        message["text"] = json!(
            "https://www.openstreetmap.org/?mlat=60.10000&mlon=24.90000#map=16/60.10000/24.90000"
        );
        message["hasLocation"] = json!(true);
    }
    if locations && msg == 31 {
        message["text"] = json!("Location streaming enabled by Ada Lovelace.");
        message["isInfo"] = json!(true);
        message["systemMessageType"] = json!("LocationStreamingEnabled");
    }
    // One of each other media kind, plus video second picture: in group under
    // `PIIRIT_FAKE_MEDIA`, by id always. Sizes/types as core.
    match msg {
        12 => {
            message["viewType"] = json!("File");
            // Under hash, as core keeps received file; sender's name is `fileName`.
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
            // Ms, as sender writes: row shows length before play.
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
            // Ms, as sender writes: viewer shows it till player has file.
            message["duration"] = json!(7_000);
        }
        _ => {}
    }
    // Cut by sender's core: ends in marker, whole only via `get_message_html`. In no chat:
    // whole-message page asks by id.
    if msg == 11 {
        message["text"] = json!(format!("{LONG_MESSAGE_HEAD}\n[...]"));
        message["hasHtml"] = json!(true);
    }
    message
}

/// Large attachment as core 2.x keeps it until fetched (`PIIRIT_FAKE_PRE_MESSAGES`): Text
/// pre-message, name, size, dimensions (sender's), core's " [Image – 2.4 MiB]" after text; no
/// file. Fetched: real kind, file, same id. 16 picture, 17 video, 18 PDF, 19 PNG sent as file
/// (no size, as core never sizes files). 17's fetch never lands: stays `InProgress`, as core says
/// while fetching.
fn pre_message(message: &mut Value, msg: u64, fetched: bool) {
    let (kind, name, bytes, said, mime) = match msg {
        16 => (
            "Image",
            "beach.jpg",
            2_516_582,
            "Image – 2.4 MiB",
            "image/jpeg",
        ),
        17 => ("Video", "trip.mp4", 8_388_608, "Video – 8 MiB", "video/mp4"),
        18 => (
            "File",
            "report.pdf",
            3_145_728,
            "report.pdf – 3 MiB",
            "application/pdf",
        ),
        19 => (
            "File",
            "scan.png",
            1_048_576,
            "scan.png – 1 MiB",
            "image/png",
        ),
        _ => return,
    };
    message["fileName"] = json!(name);
    message["fileBytes"] = json!(bytes);
    if msg == 16 {
        message["dimensionsWidth"] = json!(800);
        message["dimensionsHeight"] = json!(600);
    }
    if msg == 17 {
        message["duration"] = json!(12_000);
    }
    if fetched && msg != 17 {
        message["viewType"] = json!(kind);
        message["text"] = json!("");
        message["file"] = json!(format!("/tmp/piirit-fake/{name}"));
        message["fileMime"] = json!(mime);
        message["downloadState"] = json!("Done");
    } else {
        message["viewType"] = json!("Text");
        message["text"] = json!(format!(" [{said}]"));
        message["file"] = Value::Null;
        message["downloadState"] = json!(if fetched { "InProgress" } else { "Available" });
    }
}

/// Start of long message: fits in `text`.
const LONG_MESSAGE_HEAD: &str = "# Groceries";

/// Whole message HTML as pinned `deltachat-rpc-server` writes. Newlines matter: head on own
/// lines, newline -> `<br/>` *plus newline*, so counting both double-spaces; one-line fixture
/// would hide that bug.
const LONG_MESSAGE_HTML: &str = "<!DOCTYPE html>\n\
     <html><head>\n\
     <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\" />\n\
     <meta name=\"color-scheme\" content=\"light dark\" />\n\
     </head><body dir=\"auto\" style=\"unicode-bidi: plaintext\">\n\
     Groceries<br/>\nmilk<br/>\nbread<br/>\nand a &amp; sign<br/>\n\
     </body></html>\n";

/// Inputs standing for unreachable server.
fn should_fail(value: &str) -> bool {
    value.contains("fail")
}

/// Chat ids in `var`, comma-separated: test marks seeded chats self/device, no fixture each.
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

/// Ask relay in `qr` for account. Instant unless `slow`: after `PIIRIT_FAKE_SLOW_MS` (3 s
/// default), as down relay holds core. Pending = configuring: second transport call refused
/// (core's words); `stop_ongoing_process` fails it unless `deaf` (stop too late).
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
    // Account's name on payload's relay: rows tell apart.
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

/// `check_qr`: invite (contact, channel), relay code, other (shim's distinctions). Relay as core
/// reads it: `dcaccount:` domain, `dclogin:` address.
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
    // Channel link names channel in `b=`, as core writes it.
    if content.contains("i.delta.chat") && content.contains("&b=") {
        return json!({"kind": "askJoinBroadcast", "name": "News"});
    }
    let kind = if content.contains("i.delta.chat") || content.starts_with("OPENPGP4FPR:") {
        "askVerifyContact"
    } else if content.starts_with("DCBACKUP2:") {
        // Device offering itself; `toonew` = newer backup.
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

/// Relay `init_transports` lands on. Slow if `PIIRIT_FAKE_AUTO_RELAY` (no payload to say).
fn automatic_relay() -> String {
    std::env::var("PIIRIT_FAKE_AUTO_RELAY").unwrap_or_else(|_| "first.auto.example".to_string())
}

/// Take profile from device or file: `ImexProgress`, then configured. `fail` refuses; `slow`
/// waits (answers `stop_ongoing_process` unless `deaf`).
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
    // Backup carries address: same backup twice = same address, keyed on import argument.
    let addr = backup_addr(from);
    state.add_transport(account, &addr);
    state.configure(account);
    ok(id, &Value::Null)
}

/// One relay's connectivity block as core writes (pinned in
/// `deltachat-jsonrpc/tests/real_server.rs`): domain bold, mailbox on bar. Distinct figures
/// per relay catch page reading wrong bar.
fn transport_block(addr: &str) -> String {
    let domain = addr.rsplit('@').next().unwrap_or(addr);
    // Older relay: reached, not lately: yellow.
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

/// Backup's address from path or code: same thing, same address.
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

/// Export as core's `export_backup`: `ImexProgress`, `.tar` in folder. `fail` refuses;
/// `slow` waits (answers `stop_ongoing_process` unless `deaf`).
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
    // Core's name: date, number, address. Page finds the one new file.
    let written = std::path::Path::new(folder).join(format!("delta-chat-2026-01-01-{account}.tar"));
    // Off runtime threads, as shim: `std::fs` blocks.
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

/// Offer profile to second device, as `provide_backup`: open until taken or stopped.
/// `ImexProgress` 1000 taken, 0 stopped; unread code emits nothing. By account:
/// `PIIRIT_FAKE_PROVIDE_FAIL` refuses, `PIIRIT_FAKE_TAKEN` taken `PIIRIT_FAKE_SLOW_MS` later,
/// `PIIRIT_FAKE_STALLED` arrives then leaves; others wait for stop.
async fn provide_backup(state: &Arc<Mutex<State>>, id: &Value, account: u32) -> Value {
    if env_ids("PIIRIT_FAKE_PROVIDE_FAIL").contains(&u64::from(account)) {
        return err(id, "backup could not be offered");
    }
    if !state.lock().await.configuring.insert(account) {
        return err(id, "There is already another ongoing process running.");
    }
    let stalls = env_ids("PIIRIT_FAKE_STALLED").contains(&u64::from(account));
    let taken = if stalls || env_ids("PIIRIT_FAKE_TAKEN").contains(&u64::from(account)) {
        // Device arrives half a wait in: transfer has a middle (page bar, mid-transfer test).
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
        // `Ok` like core, stopped or device left; only progress (0 vs 1000) differs.
        return ok(id, &Value::Null);
    }
    state.imex(account, 1000);
    ok(id, &Value::Null)
}

/// Wait for `stop_ongoing_process`, as provider holds call. Capped at 30 s.
async fn wait_for_stop(state: &Arc<Mutex<State>>, account: u32) {
    for _ in 0..600 {
        if state.lock().await.stopped.contains(&account) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Code as `get_backup_qr`: `DCBACKUP5:` (pinned transfer version), opaque but to core; named
/// for account so profiles differ. Core blocks until ready (fails after 1 min); this waits for
/// start. `PIIRIT_FAKE_NO_QR` fails (blank page).
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

/// Reply delay in ms from `var`: fixes reply order.
fn delay(var: &str) -> std::time::Duration {
    delay_or(var, 0)
}

fn main() {
    // Not `#[tokio::main]`: needs `macros` (proc-macro crate app build lacks).
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

/// Pictures test dropped in `path` (see `PIIRIT_FAKE_ARRIVALS`), each a new message in chat 2.
fn land_pictures(state: &mut State, path: &std::path::Path) {
    let Ok(arrivals) = std::fs::read_to_string(path) else {
        return;
    };
    let _ = std::fs::remove_file(path);
    for arrival in arrivals.lines().filter(|line| !line.trim().is_empty()) {
        let id = state.add_message(1, 2);
        state.sent_files.insert(
            id,
            Sent {
                file: json!(format!("/tmp/piirit-fake/arrived-{id}.jpg")),
                view_type: "Image".to_string(),
                text: None,
                file_name: None,
            },
        );
        if arrival.trim() == "quiet" {
            // `add_message`'s two.
            state.events.pop_back();
            state.events.pop_back();
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn serve() {
    // Seeded before first request for early profile asks.
    let mut seeded = State::default();
    seeded.seed_accounts();
    let state = Arc::new(Mutex::new(seeded));
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

    // Payload-free events of asked kinds, queued first (core can't emit on demand): tests the
    // unread-kind cost behind `HANDLED_EVENT_KINDS`.
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
    // Server dies under client.
    if let Ok(after) = std::env::var("PIIRIT_FAKE_EXIT_AFTER_MS") {
        if let Ok(millis) = after.parse() {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                std::process::exit(0);
            });
        }
    }
    // Mid-chat sort: no app call does it.
    if let Ok(after) = std::env::var("PIIRIT_FAKE_LATE_ARRIVAL_MS") {
        if let Ok(millis) = after.parse() {
            let state = state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                state.lock().await.add_late_message(1, 1);
            });
        }
    }
    // Pictures arriving in gallery group (chat 2) when test says: `PIIRIT_FAKE_ARRIVALS` = file
    // test drops, a line per picture (`quiet`: events lost, core's overflow case; only reread
    // finds it). Landed before next request read: on test's clock, not this process's.
    let arrivals = std::env::var_os("PIIRIT_FAKE_ARRIVALS").map(std::path::PathBuf::from);
    // Call ringing after delay: first chat's peer, audio, offer page would answer. Core sends
    // `IncomingCall` and `MsgsChanged`.
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
    // One request handled late: test drops `PIIRIT_FAKE_SLOW_ONCE` file with text (`"method"`,
    // `"value"`); next request holding it takes it (here, in order read), waits 1 s, then
    // journaled and done. Later ones overtake it.
    let slow_once = std::env::var_os("PIIRIT_FAKE_SLOW_ONCE").map(std::path::PathBuf::from);
    // Server dies on next request once test drops `PIIRIT_FAKE_EXIT_ONCE` file; replacement lives.
    let exit_once = std::env::var_os("PIIRIT_FAKE_EXIT_ONCE").map(std::path::PathBuf::from);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(path) = &arrivals {
            land_pictures(&mut *state.lock().await, path);
        }
        if let Some(path) = &exit_once {
            if tokio::fs::remove_file(path).await.is_ok() {
                std::process::exit(0);
            }
        }
        let mut slow = false;
        if let Some(path) = &slow_once {
            if let Ok(text) = tokio::fs::read_to_string(path).await {
                slow = line.contains(text.trim()) && tokio::fs::remove_file(path).await.is_ok();
            }
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
            if slow {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
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
                    // Reseeded when empty: deleting all restores starting ones.
                    if state.accounts.is_empty() {
                        state.seed_accounts();
                    }
                    let list = state.account_list();
                    ok(&id, &list)
                }
                "get_all_account_ids" => {
                    let mut state = state.lock().await;
                    if state.accounts.is_empty() {
                        state.seed_accounts();
                    }
                    let ids: Vec<u32> = state.accounts.iter().map(|account| account.id).collect();
                    ok(&id, &json!(ids))
                }
                // Live location, as core: chat on or off, `ChatModified` either way.
                "send_locations_to_chat" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let seconds = positional(2).as_i64().unwrap_or_default();
                    let mut state = state.lock().await;
                    if seconds > 0 {
                        state.sharing.insert((account, chat));
                    } else {
                        state.sharing.remove(&(account, chat));
                    }
                    if (1..60).contains(&seconds) {
                        state.expiring.insert((account, chat));
                    }
                    state.events.push_back(json!({
                        "contextId": account,
                        "event": {"kind": "ChatModified", "chatId": chat},
                    }));
                    ok(&id, &Value::Null)
                }
                "is_sending_locations" => {
                    let account = account_id();
                    let sharing = state.lock().await.sharing.iter().any(|(on, _)| *on == account);
                    ok(&id, &json!(sharing))
                }
                "is_sending_locations_to_chat" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let sharing = state.lock().await.sharing.contains(&(account, chat));
                    ok(&id, &json!(sharing))
                }
                // Stored only while some chat shares; answers whether any does.
                "set_location" => {
                    let mut state = state.lock().await;
                    let expired = std::mem::take(&mut state.expiring);
                    state.sharing.retain(|on| !expired.contains(on));
                    let accounts: std::collections::BTreeSet<u32> =
                        state.sharing.iter().map(|(account, _)| *account).collect();
                    for account in &accounts {
                        state.events.push_back(json!({
                            "contextId": account,
                            "event": {"kind": "LocationChanged", "contactId": SELF},
                        }));
                        // Core tells map app too (`emit_location_changed`).
                        if let Some(map) = state.integrations.get(account).copied() {
                            state.events.push_back(json!({
                                "contextId": account,
                                "event": {"kind": "WebxdcStatusUpdate", "msgId": map,
                                          "statusUpdateSerial": 0},
                            }));
                        }
                    }
                    ok(&id, &json!(!accounts.is_empty()))
                }
                // Core copies file in: unreadable or no zip refused. New app, new message.
                "set_webxdc_integration" => {
                    let account = account_id();
                    let path = positional(1).as_str().unwrap_or_default().to_string();
                    match std::fs::read(&path) {
                        Ok(bytes) if bytes.starts_with(b"PK\x03\x04") => {
                            let mut state = state.lock().await;
                            state.next_message_id += 1;
                            let app = state.next_message_id;
                            state.integrations.insert(account, app);
                            ok(&id, &Value::Null)
                        }
                        _ => err(&id, "not a webxdc file"),
                    }
                }
                // Account's map app, pointed at chat; null if none.
                "init_webxdc_integration" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    match state.integrations.get(&account).copied() {
                        Some(app) => {
                            state.integrated_for.insert(app, chat);
                            ok(&id, &json!(app))
                        }
                        None => ok(&id, &Value::Null),
                    }
                }
                // Newest first. Ada: streamed point after her one-off (message 30); own one-offs.
                "get_locations" => {
                    let contact = positional(2).as_u64().unwrap_or_default();
                    let state = state.lock().await;
                    let mut found = Vec::new();
                    if contact == 10 {
                        found.push(json!({"latitude": 60.2, "longitude": 25.0, "accuracy": 12.0,
                                          "timestamp": 1_700_090_600, "contactId": 10,
                                          "msgId": 0, "isIndependent": false}));
                        found.push(json!({"latitude": 60.1, "longitude": 24.9, "accuracy": 0.0,
                                          "timestamp": 1_700_090_000, "contactId": 10,
                                          "msgId": 30, "isIndependent": true}));
                    }
                    if contact == u64::from(SELF) {
                        for (msg, (latitude, longitude)) in state.points.iter().rev() {
                            found.push(json!({"latitude": latitude, "longitude": longitude,
                                              "accuracy": 0.0, "timestamp": 1_700_090_000,
                                              "contactId": SELF, "msgId": msg,
                                              "isIndependent": true}));
                        }
                    }
                    ok(&id, &json!(found))
                }
                "remove_account" => {
                    let mut state = state.lock().await;
                    let gone = positional(0).as_u64().unwrap_or(0);
                    let gone = u32::try_from(gone).unwrap_or(0);
                    state.accounts.retain(|account| account.id != gone);
                    // Core deletes account dir with transports, config.
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
                    // Null clears, as core; "" is a value. Core refuses "" for `selfavatar`: app
                    // sends null.
                    match positional(2).as_str() {
                        _ if key == REMOVED_KEY => err(&id, &unknown_key(&key)),
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
                    // `sys.` keys computed, not stored.
                    let default = (key == RECOMMENDED_SIZE_KEY).then(recommended_size);
                    if key == REMOVED_KEY {
                        err(&id, &unknown_key(&key))
                    } else {
                        let value = state
                            .config
                            .get(&(account, key))
                            .cloned()
                            .or(default)
                            .map_or(Value::Null, Value::String);
                        ok(&id, &value)
                    }
                }
                // Read/unread from list. Core announces both (badges follow); profiles page counts
                // `get_fresh_msgs`.
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
                    // Core (`markfresh_chat`) freshens newest incoming, announces chat changed (no
                    // message) and list item changed.
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
                // Read position: last unseen run, newest back to first seen, as core; from
                // `message_object` states so fake agrees with itself.
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
                            // Outgoing: neither.
                            _ => {}
                        }
                    }
                    ok(&id, &first.map_or(Value::Null, |msg| json!(msg)))
                }
                "get_fresh_msgs" => {
                    let account = account_id();
                    // Held back on request: events arrive while count read.
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
                // Forgotten; tests read journal.
                | "set_stock_strings" => ok(&id, &Value::Null),
                "start_io_for_all_accounts" | "stop_io_for_all_accounts" => {
                    state.lock().await.io_stopped = method == "stop_io_for_all_accounts";
                    ok(&id, &Value::Null)
                }
                "stop_ongoing_process" => {
                    state.lock().await.stopped.insert(account_id());
                    ok(&id, &Value::Null)
                }
                // Core picks relays: first answering, second once up (background). Keyed on
                // account.
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
                // Core's import, keyed on argument: code or file path.
                "get_backup" | "import_backup" => {
                    let from = positional(1).as_str().unwrap_or_default().to_string();
                    import_into(&state, &id, account_id(), &from).await
                }
                // `get_backup` other end: this phone serves profile.
                "provide_backup" => provide_backup(&state, &id, account_id()).await,
                "get_backup_qr" => backup_qr(&state, &id, account_id()).await,
                // Other way: written to folder.
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
                        // Malformed EnteredLoginParam: invalid-params, as core.
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
                // Oldest first, `EnteredLoginParam` each (app reads `addr`).
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
                // Last or unknown transport refused (core's words). Own address moves with order.
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
                    // `DC_GCL_ADD_SELF`: self last, only unsearched (pinned in real_server.rs).
                    if flags & 0x02 != 0 && query.is_empty() {
                        ids.push(SELF);
                    }
                    let contacts: Vec<Value> = ids
                        .iter()
                        .filter_map(|contact| state.contact_object(account_id(), *contact))
                        .collect();
                    ok(&id, &Value::Array(contacts))
                }
                // Core refusing blocked list, accounts test names.
                "get_blocked_contacts"
                    if env_ids("PIIRIT_FAKE_BLOCKED_FAIL").contains(&u64::from(account_id())) =>
                {
                    err(&id, "the block list could not be read")
                }
                // No `get_contacts` flag brings blocked back.
                "get_blocked_contacts" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let ids: Vec<u32> = state.blocked.iter().copied().collect();
                    let contacts: Vec<Value> = ids
                        .iter()
                        .filter_map(|contact| state.contact_object(account_id(), *contact))
                        .collect();
                    ok(&id, &Value::Array(contacts))
                }
                // Answer nothing, announce change.
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
                // Profile page connectivity: band, web-view report with quota bar (pinned in
                // real_server.rs). IO stopped: not connected, no relay.
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
                    // Every transport, oldest first, own quota: older relay's bar first.
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
                // As `get_messages`: reader asks before download-limited rest.
                "get_message" => {
                    let msg = positional(1).as_u64().unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    ok(&id, &state.full_message(msg))
                }
                // Only seeded cut message has whole; core: null for whole ones.
                "get_message_html" => {
                    let msg = positional(1).as_u64().unwrap_or_default();
                    if msg == 11 {
                        ok(&id, &json!(LONG_MESSAGE_HTML))
                    } else {
                        ok(&id, &Value::Null)
                    }
                }
                // Core fetches, announces; instant here.
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
                // Own reactions, whole list; empty removes. Core's event; answers hidden carrier
                // message id (no chat list).
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
                // Join and 1:1: fresh chat at top. Channel link (`qr_kind`): read-only channel.
                "create_chat_by_contact_id" | "secure_join" => {
                    let channel = method.as_str() == "secure_join"
                        && qr_kind(positional(1).as_str().unwrap_or_default())["kind"]
                            == "askJoinBroadcast";
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.next_chat_id += 1;
                    let chat = state.next_chat_id;
                    state.chats.insert(chat, Vec::new());
                    state.chat_order.insert(0, chat);
                    if channel {
                        state.joined_channels.insert(chat);
                    }
                    ok(&id, &json!(chat))
                }
                // Own channel: named, no members; core refuses adding any.
                "create_broadcast" => {
                    let name = positional(1).as_str().unwrap_or_default().to_string();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.next_chat_id += 1;
                    let chat = state.next_chat_id;
                    state.chats.insert(chat, Vec::new());
                    state.chat_order.insert(0, chat);
                    state.own_channels.insert(chat);
                    state.group_names.insert(chat, name);
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
                    // Core's words: subscribers join by invite only.
                    if state.is_own_channel(chat) {
                        err(
                            &id,
                            &format!("Chat#{chat} is not a group where one can add members"),
                        )
                    } else {
                        let members = state.group_members.entry(chat).or_default();
                        if !members.contains(&contact) {
                            members.push(contact);
                        }
                        state.chat_modified(account, chat);
                        ok(&id, &Value::Null)
                    }
                }
                // `ChatInfo` shape: member ids; contacts via `get_contacts_by_ids`, keyed by id
                // string.
                "get_full_chat_by_id" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let is_group = state.is_group(chat);
                    // Channel subscriber: no writing, as left group.
                    let left = state.left_groups.contains(&chat) || state.is_channel(chat);
                    // Own channel: subscribers only, none yet; reader not among them, as core.
                    let members = if is_group {
                        state.group_members.get(&chat).cloned().unwrap_or_default()
                    } else if state.is_own_channel(chat) {
                        Vec::new()
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
                                "chatType": state.chat_type(chat),
                                "name": state.chat_name(chat),
                                "profileImage": state.group_images.get(&chat),
                                "color": "#0071c7",
                                "contactIds": members,
                                // Both false once left, as core; 1:1 no group to be "in".
                                "selfInGroup": is_group && !left,
                                "canSend": !left,
                                // All chats here chatmail.
                                "isEncrypted": true,
                                "isSelfTalk": false,
                                "isDeviceChat": false,
                                "ephemeralTimer": state.timers.get(&chat).copied().unwrap_or(0),
                            }),
                        )
                    }
                }
                // Member ids only: header count. As `get_full_chat_by_id`.
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
                        if let Some(object) = state.contact_object(account_id(), contact) {
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
                    // Core refuses empty name, or chat not a group account is in.
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
                // As core: trimmed; empty clears; refused unless in group.
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
                    // Removing non-member: no error to core either.
                    if let Some(members) = state.group_members.get_mut(&chat) {
                        members.retain(|member| *member != contact);
                    }
                    if contact == SELF {
                        state.left_groups.insert(chat);
                    }
                    state.chat_modified(account, chat);
                    ok(&id, &Value::Null)
                }
                // Checked vs core: any chat; own event carries timer.
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
                // Fingerprint code, no invite secrets: no kind since 2.61. Core refuses if no
                // contact has key: here always.
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
                // Own invite, or own channel's (names channel in `b=`, as core).
                "get_chat_securejoin_qr_code" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok());
                    let state = state.lock().await;
                    match chat {
                        Some(chat) if state.is_own_channel(chat) => ok(
                            &id,
                            &json!(format!(
                                "https://i.delta.chat/#ABCDEF&a=me%40example.org&n=Me&b={}",
                                state.chat_name(chat)
                            )),
                        ),
                        _ => ok(
                            &id,
                            &json!("https://i.delta.chat/#ABCDEF&a=me%40example.org&n=Me"),
                        ),
                    }
                }
                // Here, or everywhere (deletion request as own message). No delivery here: same to
                // chat.
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
                    // Core announces deletion; model reloads.
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
                            "chatType": state.chat_type(chat),
                            "name": state.chat_name(chat),
                            "isEncrypted": true,
                            "isMuted": state.muted.contains(&chat),
                            "isContactRequest": state.is_request(chat),
                        }),
                    )
                }
                // Request becomes ordinary chat; core announces it.
                "accept_chat" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    state.accepted_requests.insert(chat);
                    state.chat_modified(account, chat);
                    ok(&id, &Value::Null)
                }
                // Sender blocked: core hides chat, as deleted.
                "block_chat" => {
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
                // Writable: not left group nor unaccepted request, as `get_full_chat_by_id`.
                "can_send" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    ok(
                        &id,
                        &json!(
                            !state.left_groups.contains(&chat)
                                && !state.is_request(chat)
                                && !state.is_channel(chat)
                        ),
                    )
                }
                // Edit own text. Core refuses empty and others' (menu offers neither): only empty
                // refused, `fail` fails like send. Announced as core: model re-reads row.
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
                // Messages a deletion period would take: older, all chats; seeded: all.
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
                // Core keeps duration, announces `ChatModified`; list reads `isMuted`.
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
                // One contact by id, self too: colour lookup.
                "get_contact" => {
                    let contact = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    match state.contact_object(account_id(), contact) {
                        Some(object) => ok(&id, &object),
                        None => err(&id, "contact not found"),
                    }
                }
                "get_chatlist_entries" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // `DC_GCL_ARCHIVED_ONLY`: disjoint lists: picks list, not filter.
                    let archived_only = positional(1).as_u64().unwrap_or(0) & 0x01 != 0;
                    // Checked vs core: `ARCHIVED_ONLY` ignores query; plain query searches all
                    // incl. archived. Filtering archived here would pass one-call implementation
                    // that fails on device.
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
                    // Slow *ordinary* list: stale answer lands last. Concurrent as core; delays own
                    // reply.
                    if !archived_only {
                        if let Some(delay) = std::env::var("PIIRIT_FAKE_CHATLIST_DELAY_MS")
                            .ok()
                            .and_then(|value| value.parse::<u64>().ok())
                        {
                            drop(state);
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                    }
                    // Matched as core: chat item's name.
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
                    // Core builds rows slowly per batch; delay shows first.
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
                        // Draft chat previews draft: "Draft" prefix, DC_STATE_OUT_DRAFT (pinned in
                        // real_server.rs).
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
                                // Self and device chats when named; cover omits.
                                "isSelfTalk": env_ids("PIIRIT_FAKE_SELF_TALK").contains(&chat),
                                "isDeviceTalk": env_ids("PIIRIT_FAKE_DEVICE_TALK").contains(&chat),
                                // Chat 1 pinned: ordinary list has both kinds; archived one.
                                "isPinned": chat == 1,
                                "isContactRequest": u32::try_from(chat)
                                    .is_ok_and(|chat| state.is_request(chat)),
                            }),
                        );
                    }
                    ok(&id, &Value::Object(items))
                }
                "search_messages" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Third arg: chat to search (null = all); pinned binary rejects two.
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
                    // Delayed like get_messages: two in flight.
                    tokio::time::sleep(delay("PIIRIT_FAKE_FETCH_DELAY_MS")).await;
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    // Fourth arg: day markers before each day's first message, as core; placeholder
                    // reads day.
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
                // Ids of up to three kinds (null chat: all), oldest first, as core (don't re-sort).
                // Media pages.
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
                    // Multi-id page as chat opens, not send readback: slow on phone with long
                    // picture chat.
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
                // One draft per chat, read back, cleared, as core.
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
                    // Null for none, else message object, as core.
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
                    // Core picks view type from file; echoed to sender's row.
                    let file = positional(3);
                    let file_name = positional(4);
                    let point = positional(5);
                    let quoted = positional(6);
                    if should_fail(&text) {
                        // Failed send also reports Error event.
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
                            if let (Some(latitude), Some(longitude)) = (
                                point.get(0).and_then(Value::as_f64),
                                point.get(1).and_then(Value::as_f64),
                            ) {
                                state.points.insert(msg, (latitude, longitude));
                            }
                            msg
                        };
                        // Queued above: delay puts event before reply (core can); it duplicated a
                        // sent row.
                        tokio::time::sleep(delay("PIIRIT_FAKE_SEND_DELAY_MS")).await;
                        // Extension, not content: image vs paperclip.
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
                        // As `send_msg`: refetched row stays app.
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
                                 "viewType": view_type,
                                 "hasLocation": point.is_array()}
                            ]),
                        )
                    }
                }
                // MessageData send for type core wouldn't pick (voice). Answers id; `get_messages`
                // names it.
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
                        // Held back like other send: in-flight tests.
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
                // Core fetches off web, not app: store install.
                "get_http_response" => {
                    let url = positional(1).as_str().unwrap_or_default().to_string();
                    // Map tiles: `7/…` answered by server's error page, as a blocked one.
                    if let Some(tile) = url.strip_prefix("https://tile.openstreetmap.org/") {
                        let (body, mimetype) = if tile.starts_with("7/") {
                            (b"<html>blocked</html>".to_vec(), "text/html")
                        } else {
                            (format!("\u{89}PNG tile {tile}").into_bytes(), "image/png")
                        };
                        ok(
                            &id,
                            &json!({"blob": base64(&body), "mimetype": mimetype,
                                    "encoding": Value::Null}),
                        )
                    } else if should_fail(&url) {
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
                // Name, self-description; summary counts sent updates: row follows.
                "get_webxdc_info" => {
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let state_now = state.lock().await;
                    let sent = state_now.webxdc_updates.get(&msg).map_or(0, Vec::len);
                    let map = state_now.integrations.values().any(|app| *app == msg);
                    drop(state_now);
                    let answer = if map {
                        // Integration: core lets it online.
                        json!({
                            "name": "Maps", "icon": "", "document": null, "summary": null,
                            "sourceCodeUrl": "https://github.com/deltachat/maps",
                            "internetAccess": true, "selfAddr": "self@example.org",
                            "isAppSender": true, "isBroadcast": false,
                            "sendUpdateInterval": 1000, "sendUpdateMaxSize": 102_400,
                        })
                    } else {
                        json!({
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
                        })
                    };
                    ok(&id, &answer)
                }
                // One archive file, base64 STANDARD_NO_PAD as core; page, icon only.
                "get_webxdc_blob" => {
                    let msg = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let path = positional(2).as_str().unwrap_or_default().to_string();
                    let map = state.lock().await.integrations.values().any(|app| *app == msg);
                    let file = if map { map_file(&path) } else { webxdc_file(&path) };
                    match file {
                        Some(bytes) => ok(&id, &json!(base64(&bytes))),
                        None => err(&id, "no such file in the webxdc"),
                    }
                }
                // Updates after serial as one JSON *string* (core sends encoded).
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
                        // Core returns update with both serials; own send counts received.
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
                // Call methods as core (upstream src/calls.rs): every state move announced.
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
                        // Offer asking for it answered at once, as peer would.
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
                        // Ended already: core logs, no-op.
                        Some(_) => Some(false),
                    };
                    match ended {
                        None => err(&id, "end_call is called with a message that is not a call"),
                        Some(moved) => {
                            if moved {
                                state.call_event(account, "CallEnded", msg, &json!({}));
                                state.call_changed(account, msg);
                                // Hidden message to peer; app waits on it to end call.
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
                    // Blocks when empty, like long poll.
                    loop {
                        let queued: Vec<Value> = state.lock().await.events.drain(..).collect();
                        if !queued.is_empty() {
                            // Held back after batch taken (before wait buys nothing): call reply
                            // then precedes its event; else Qt-thread race, machine-dependent.
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

/// Core's words for config key it lacks.
fn unknown_key(key: &str) -> String {
    format!("unknown key {key:?}: Matching variant not found")
}

fn err(id: &Value, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}})
}
