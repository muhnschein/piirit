//! A `deltachat-rpc-server` double that records what it was asked.
//!
//! Every request is appended as one JSON line to `PIIRIT_FAKE_JOURNAL`,
//! so tests can assert which calls a UI action makes, in what order, with
//! what parameters. `get_next_event_batch` is left out: the client polls it
//! in a loop and would bury the sequence.
//!
//! Behaviour is keyed on input rather than an environment switch, so one
//! process can drive success and failure: a QR payload or address
//! containing `fail` is rejected.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;

use chrono::{Local, TimeZone};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// `DC_CONTACT_ID_SELF`: the account's own contact.
const SELF: u32 = 1;

/// The config key the core answers the largest recommended attachment
/// with, and the real core's own answer to it: `(30 - 1) MiB * 3/4`,
/// which is 30 MiB of chatmail relay less a megabyte of headers and less
/// what base64 adds. Computed there and here, never stored.
const RECOMMENDED_SIZE_KEY: &str = "sys.msgsize_max_recommended";
const RECOMMENDED_SIZE: u64 = (30 - 1) * 1024 * 1024 / 4 * 3;

/// What to answer `sys.msgsize_max_recommended` with.
///
/// `PIIRIT_FAKE_MSGSIZE_MAX` lowers it, so a test about a file the
/// relay will not take can write a file of a few kilobytes rather than
/// twenty-odd megabytes.
fn recommended_size() -> String {
    std::env::var("PIIRIT_FAKE_MSGSIZE_MAX")
        .ok()
        .filter(|value| value.parse::<u64>().is_ok())
        .unwrap_or_else(|| RECOMMENDED_SIZE.to_string())
}

/// One account, as `get_all_accounts` reports it.
struct Account {
    id: u32,
    configured: bool,
}

/// One call, as the real core keeps it on the call's message.
struct FakeCall {
    /// It came in, rather than being placed here.
    incoming: bool,
    has_video: bool,
    /// The caller's SDP, as `call_info` hands it back.
    offer: String,
    /// Answered, at either end.
    accepted: bool,
    /// The core's `CallState` kind, which `call_info` reports.
    state: &'static str,
}

impl FakeCall {
    /// The text the real core writes onto the call's message, in its own
    /// English: what a row that does not read the state would show.
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

    /// `call_info`, as the real core serialises it: camelCase, the state
    /// tagged by `kind`, and a duration only on a completed call.
    fn info(&self) -> Value {
        let state = if self.state == "Completed" {
            json!({"kind": "Completed", "duration": 185})
        } else {
            json!({"kind": self.state})
        };
        json!({"sdpOffer": self.offer, "hasVideo": self.has_video, "state": state})
    }
}

/// What one `send_msg` handed over: the fields of its `MessageData` the
/// row has to carry back.
struct Sent {
    file: Value,
    view_type: String,
    text: Option<String>,
    file_name: Option<String>,
}

#[derive(Default)]
struct State {
    accounts: Vec<Account>,
    /// The highest account id handed out so far. The real core never
    /// hands one out twice, even once the account has gone.
    last_account: u32,
    /// The profile the app last said it was showing. The real core keeps
    /// it on disk, selects a newly added account itself, and falls back
    /// to the first that is left when the selected one is removed.
    selected: Option<u32>,
    /// Events waiting to be handed out by `get_next_event_batch`.
    events: VecDeque<Value>,
    /// Message ids per chat, oldest first. Seeded on first use.
    chats: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Chat ids in display order, most recent first.
    chat_order: Vec<u32>,
    /// The archived list, which is disjoint from `chat_order`.
    archived_order: Vec<u32>,
    next_message_id: u32,
    /// Contact id -> address. Seeded with two known contacts.
    contacts: std::collections::BTreeMap<u32, String>,
    next_chat_id: u32,
    /// Members of each group, by chat. A chat with an entry here is a
    /// group; the seeded one has the account itself in it, as the real
    /// core lists it.
    group_members: std::collections::BTreeMap<u32, Vec<u32>>,
    /// Group names given after creation, by chat.
    group_names: std::collections::BTreeMap<u32, String>,
    /// Group pictures, by chat. The real core keeps the path of its own
    /// copy; this keeps the path it was given.
    group_images: std::collections::BTreeMap<u32, String>,
    /// Groups this account has left. The real core refuses every change
    /// to one of these.
    left_groups: std::collections::BTreeSet<u32>,
    /// Chats deleted here. The real core cannot load one any more, and
    /// says so to whatever asks for it by id.
    deleted_chats: std::collections::BTreeSet<u32>,
    /// Accounts whose transport call has not been answered yet. The
    /// real core allows one ongoing process per account and refuses a
    /// second; see `add_transport_from_qr`.
    configuring: std::collections::BTreeSet<u32>,
    /// Accounts whose ongoing process was asked to stop.
    stopped: std::collections::BTreeSet<u32>,
    /// Seconds after which a chat's messages disappear, by chat.
    timers: std::collections::BTreeMap<u32, u32>,
    /// Config values per account, so a set can be read back.
    config: std::collections::BTreeMap<(u32, String), String>,
    /// The unsent text each chat is holding. The core keeps drafts, so a
    /// fake standing in for it has to as well.
    drafts: std::collections::BTreeMap<u32, String>,
    /// Names given to contacts here, by contact. The real core shows one
    /// in place of what the contact calls themselves, and an empty name
    /// puts theirs back.
    contact_names: std::collections::BTreeMap<u32, String>,
    /// Contacts this account has blocked. The real core keeps them out of
    /// `get_contacts` whatever the flags say and lists them through
    /// `get_blocked_contacts` alone; so does this.
    blocked: std::collections::BTreeSet<u32>,
    /// Messages whose remainder was asked for; see `downloadState`.
    downloaded: std::collections::BTreeSet<u32>,
    /// Chats with an unread message on them, by account: marked unread
    /// from the list, and cleared when the chat is noticed. The real
    /// core counts fresh messages; this counts a chat as having one.
    fresh: std::collections::BTreeSet<(u32, u32)>,
    /// Reactions by message, then by the contact who sent them. The real
    /// core keeps one list per contact, and the list this account sends
    /// replaces whatever it had there before.
    reactions: std::collections::BTreeMap<u32, std::collections::BTreeMap<u32, Vec<String>>>,
    /// What a `send_msg` handed over, by message, so `get_messages` can
    /// say what was sent.
    sent_files: std::collections::BTreeMap<u32, Sent>,
    /// Status updates per webxdc instance, in the order they were sent.
    /// The real core numbers them from 1 and hands out everything after
    /// the serial it is asked from; so does this.
    webxdc_updates: std::collections::BTreeMap<u32, Vec<Value>>,
    /// Texts changed after sending, by message: what `send_edit_request`
    /// leaves behind, and what marks a message edited.
    edits: std::collections::BTreeMap<u32, String>,
    /// Chats muted from the list. The real core keeps a duration; this
    /// keeps whether.
    muted: std::collections::BTreeSet<u32>,
    /// The message each sent message quotes, when it quotes one.
    quotes: std::collections::BTreeMap<u32, u32>,
    /// Messages sent from here, in the order they went. Read as the
    /// account's own when a test asks for that; see `full_message`.
    sent: std::collections::BTreeSet<u32>,
    /// Calls, by their message. Placed here, or rung in under
    /// `PIIRIT_FAKE_INCOMING_CALL_MS`.
    calls: std::collections::BTreeMap<u32, FakeCall>,
    /// Each account's transports, in the order they were added -- the
    /// real core's `transports` table, which `list_transports` reads
    /// oldest first. The one with the account's own address on it is
    /// `configured_addr` in `config`, as it is there.
    transports: std::collections::BTreeMap<u32, Vec<String>>,
}

impl State {
    /// A config value, as the real core would read it back.
    fn config(&self, account: u32, key: &str) -> Option<String> {
        self.config.get(&(account, key.to_string())).cloned()
    }

    /// The account's transports, oldest first.
    fn transports_for(&self, account: u32) -> Vec<String> {
        self.transports.get(&account).cloned().unwrap_or_default()
    }

    /// Keep a transport the account was just given, as the real core
    /// does: appended, once, and the account's own address if it had
    /// none yet -- a second transport is a second address to be reached
    /// at, not a change of the first.
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

    /// Profiles a test wants there from the start, already configured,
    /// named in `PIIRIT_FAKE_ACCOUNTS`. The onboarding tests leave it
    /// unset and start from none. Seeded before the first request is
    /// answered, so that a page asking about a profile before the
    /// account list has been read finds its transports there.
    ///
    /// A profile set up twice has a relay besides the one its own
    /// address (`configured_addr`) is on: what `PIIRIT_FAKE_OLDER_RELAY`
    /// seeds, as the first transport the core lists.
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

    /// Queue the event the real core sends when an account's transports
    /// change.
    fn transports_modified(&mut self, account: u32) {
        self.events.push_back(json!({
            "contextId": account,
            "event": {"kind": "TransportsModified"},
        }));
    }

    /// The accounts, shaped as the real core shapes them: a configured
    /// one carries its profile -- name, picture, colour -- but no
    /// address, which the real core dropped in 2.61, and an unconfigured
    /// one is an id and nothing else.
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

    /// Two chats with a couple of messages each, so a test can watch a
    /// model load them and then take in one more.
    fn seed_chats(&mut self) {
        if self.chats.is_empty() {
            // A chat long enough to page through, when a test asks for
            // one. Ids count up from 1, so the newest is the highest and
            // the seeded quote and picture keep the ids they always had.
            let long = std::env::var("PIIRIT_FAKE_LONG_CHAT")
                .ok()
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|count| *count > 2);
            self.chats.insert(
                1,
                long.map_or_else(|| vec![1, 2], |count| (1..=count).collect()),
            );
            self.chats.insert(2, vec![10]);
            // The rest of what a chat can hold, in the group, when a test
            // asks for it: a file, a voice message, an app and a video
            // beside the picture, one of each kind the media pages list.
            if std::env::var_os("PIIRIT_FAKE_MEDIA").is_some() {
                self.chats.insert(2, vec![10, 12, 13, 14, 15]);
            }
            self.chat_order = vec![1, 2];
            // Chat 3 is archived, and appears in no ordinary listing.
            // Without it, a model asking for the archived list and a model
            // asking for the ordinary one are indistinguishable, and a
            // test cannot tell which answer it got.
            self.chats.insert(3, vec![30]);
            // An empty archive is its own case: the page hides its search
            // field when there is nothing to search, and with a chat
            // always present no test could see that.
            self.archived_order = if std::env::var("PIIRIT_FAKE_NO_ARCHIVED").is_ok() {
                Vec::new()
            } else {
                vec![3]
            };
            // Above whatever the seeded chat used, so a message added
            // while a test runs cannot collide with one already in it.
            self.next_message_id = long.unwrap_or(0).max(100);
            self.contacts.insert(10, "ada@example.org".to_string());
            self.contacts.insert(11, "grace@example.org".to_string());
            // Chat 2 is the group: the account itself and one contact, so
            // a test can add the other and remove this one.
            self.group_members.insert(2, vec![SELF, 10]);
            self.next_chat_id = 500;
            // Someone else's reaction on the first message, when a test
            // asks for one: what a chip that is not ours looks like, and
            // what our own on top of it counts up to.
            if std::env::var_os("PIIRIT_FAKE_REACTED").is_some() {
                self.reactions
                    .entry(1)
                    .or_default()
                    .insert(10, vec!["👍".to_string()]);
            }
        }
    }

    /// A message's reactions as the real core shapes them: counted and
    /// sorted, most frequent first, beside the per-contact lists -- or
    /// null when nobody has reacted. Pinned in
    /// deltachat-jsonrpc/tests/real_server.rs.
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

    /// Whether a chat is a group: the created ones, and the seeded one.
    fn is_group(&self, chat: u32) -> bool {
        self.group_members.contains_key(&chat)
    }

    /// A chat's name: what it was renamed to, else the seeded one.
    fn chat_name(&self, chat: u32) -> String {
        self.group_names
            .get(&chat)
            .cloned()
            .unwrap_or_else(|| format!("chat {chat}"))
    }

    /// One contact, shaped as `get_contacts` shapes them. The account's
    /// own contact is not in the list but can be asked for by id, which
    /// is how a chat's members come back. Ada has written a line about
    /// herself, so a contact page has one to show.
    ///
    /// The three names are the real core's: `authName` is what the
    /// contact calls themselves, `name` what was given to them here, and
    /// `displayName` the second when there is one, else the first.
    ///
    /// Every key the 2.62 core sends is here, whether the app reads it or
    /// not, with the values for a contact nobody has seen lately.
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
            "profileImage": null,
            "lastSeen": 0,
            "freshness": "Normal",
        }))
    }

    /// Announce a change to a chat's name, picture or members, as the real
    /// core does after each of those calls.
    fn chat_modified(&mut self, account_id: u32, chat: u32) {
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ChatModified", "chatId": chat},
        }));
    }

    /// Which chat a message is in. The real core carries this on the
    /// message object; a search result is unusable without it.
    fn chat_of(&self, message_id: u32) -> u32 {
        self.chats
            .iter()
            .find(|(_, messages)| messages.contains(&message_id))
            .map_or(0, |(chat, _)| *chat)
    }

    /// Append a message to a chat and announce it, the way a send or an
    /// incoming message does.
    /// One message with everything the fake knows about it laid over the
    /// seeded shape: which chat it is in, what was sent with it, what its
    /// text was changed to, what it quotes, and whether the download
    /// limit is holding it back. What `get_message` and `get_messages`
    /// both answer with, so the two cannot disagree.
    ///
    /// A message sent from here reads as the account's own only under
    /// `PIIRIT_FAKE_SELF_SENT`: the rest of the suite leans on a sent
    /// message coming back as somebody else's, which is how a message
    /// can be made to arrive without a network.
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
            // The real core's `WithMessage` quote: the quoted message's
            // id beside its text and author.
            message["quote"] = json!({
                "kind": "WithMessage",
                "messageId": quoted,
                "text": wordy(u64::from(*quoted)),
                "authorDisplayName": "Ada Lovelace",
            });
        }
        // One message the download limit held back, when a test names
        // it, until its remainder is asked for.
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

    /// Remember what a message sent from here quoted, if anything.
    fn note_quote(&mut self, msg: u32, quoted: &Value) {
        self.sent.insert(msg);
        if let Some(quoted) = quoted.as_u64().and_then(|value| u32::try_from(value).ok()) {
            if quoted != 0 {
                self.quotes.insert(msg, quoted);
            }
        }
    }

    fn add_message(&mut self, account_id: u32, chat_id: u32) -> u32 {
        self.seed_chats();
        self.next_message_id += 1;
        let id = self.next_message_id;
        self.chats.entry(chat_id).or_default().push(id);
        // A message moves its chat to the top, which is what makes a chat
        // list reorder rather than merely change.
        self.chat_order.retain(|chat| *chat != chat_id);
        self.chat_order.insert(0, chat_id);
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "IncomingMsg", "chatId": chat_id, "msgId": id},
        }));
        // The real core follows a message with this within the same
        // millisecond (pinned with the vendored server: `MsgDelivered`,
        // then `ChatlistItemChanged` for the chat), and with a
        // `ChatlistChanged` besides when the order changed. A model that
        // announced only on the refresh the first event started never
        // announced at all, since the one behind it made that answer
        // stale before it landed.
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ChatlistItemChanged", "chatId": chat_id},
        }));
        id
    }

    /// Put a message into a chat one place before its end, and announce
    /// it. The real core sorts a received message below the newest *seen*
    /// message and no further, so one whose Date is earlier than the
    /// unread messages already at the end lands among them rather than
    /// after them -- a late message in a busy group, or an older one
    /// synced from another device.
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

    /// Put a call's message into a chat, as the real core does for a call
    /// placed here and one that comes in: a message like any other, which
    /// `MsgsChanged` announces -- a ringing call is not an `IncomingMsg`.
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

    /// Announce that a call's state moved, as the real core does with
    /// every move: `MsgsChanged` for its message.
    fn call_changed(&mut self, account_id: u32, message_id: u32) {
        let chat_id = self.chat_of(message_id);
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "MsgsChanged", "chatId": chat_id, "msgId": message_id},
        }));
    }

    /// One of the four call events, in the real core's own `snake_case`.
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

    /// Queue one import/export progress event, as the core does while a
    /// backup is read or written: 0 is a failure, 1000 is done.
    fn imex(&mut self, account_id: u32, progress: u32) {
        self.events.push_back(json!({
            "contextId": account_id,
            "event": {"kind": "ImexProgress", "progress": progress},
        }));
    }

    /// Configure an account and queue the progress events the core emits:
    /// permille steps, then 1000 for done.
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

/// When the server came up, for the journal's clock.
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn journal(method: &str, params: &Value) {
    let Ok(path) = std::env::var("PIIRIT_FAKE_JOURNAL") else {
        return;
    };
    // Polling noise would bury the sequence.
    if method == "get_next_event_batch" {
        return;
    }
    // When it arrived, in milliseconds since the server came up: a test
    // that asks whether two calls overlapped cannot read that off a Qt
    // timer of its own, which is coarse enough to fire two probes at once.
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
        // One write, not `writeln!`'s two: requests are handled
        // concurrently, and the newline landing separately tore lines into
        // each other.
        let _ = file.write_all(line.as_bytes());
    }
}

/// One message, shaped like the real core's. Seeded message 1 quotes,
/// 2 is unread, and 10 carries an image, so one fetch covers the cases the
/// conversation view has to render.
/// What a message says.
///
/// Short by default, because most tests assert on it. `PIIRIT_FAKE_WORDY`
/// makes it long enough to wrap, which is what a real conversation looks
/// like to a view: a row's height then depends on how wide it is drawn, and
/// changes when that changes. Nothing else here can make a laid-out row
/// change size, and a view that cannot be made to shift cannot be shown to
/// hold its place.
fn wordy(msg: u64) -> String {
    let text = format!("message {msg}");
    if std::env::var_os("PIIRIT_FAKE_WORDY").is_none() {
        return text;
    }
    format!("{text}, and then a good deal more of it, long enough that where it wraps depends on how wide the row is drawn")
}

/// When a message was sent.
///
/// 2023-11-14T22:13:20Z and a day later, so a day separator has something to
/// separate. Its own function because the day markers below have to agree
/// with it: a placeholder row takes its day from the marker and a filled-in
/// row from the message, and the two disagreeing is the heading changing
/// under the reader.
fn message_timestamp(msg: u64) -> i64 {
    if msg == 1 {
        1_700_000_000
    } else {
        1_700_090_000
    }
}

/// Local midnight starting the day `timestamp` falls in, which is what the
/// real core gives as a day marker -- checked against it in three zones.
fn day_start(timestamp: i64) -> i64 {
    let Some(when) = Local.timestamp_opt(timestamp, 0).single() else {
        return timestamp.div_euclid(86_400) * 86_400;
    };
    when.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| midnight.and_local_timezone(Local).earliest())
        .map_or(timestamp, |midnight| midnight.timestamp())
}

/// What the fake archive holds: a page, the script it asks for, and an
/// icon. Anything else is not in this webxdc.
///
/// The page asks for its script the way a bundler writes one -- an
/// absolute path, into a directory -- because that is what half the
/// apps in the store do and what a host serving them under a prefix
/// answers nothing to.
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

/// Standard base64 without padding, which is what the core encodes a
/// blob as. Written out rather than depended on: this is a test double.
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
        // One seeded message is unread, and so is anything added while
        // the test runs: an arrival is the case worth covering.
        "state": if msg == 2 || msg > 100 { 10 } else { 16 },
        // The first message of a real chat is the core's own "messages are
        // end-to-end encrypted" notice, which is the row the day heading
        // was reported drawn on top of.
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
    // One of each other kind the media pages list, and a second picture
    // that is a video: in the group under PIIRIT_FAKE_MEDIA, and by
    // id always. The sizes and types are what the real core reports for
    // a file it has copied.
    match msg {
        12 => {
            message["viewType"] = json!("File");
            // Under its hash, as the real core keeps a received file;
            // the sender's name for it is `fileName` and nowhere else.
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
    // A message the sending core had to cut: what is here ends in the
    // core's own marker, and the whole of it is only behind
    // `get_message_html`. In no chat -- what reads it asks for it by id,
    // the way the page that shows one whole does.
    if msg == 11 {
        message["text"] = json!(format!("{LONG_MESSAGE_HEAD}\n[...]"));
        message["hasHtml"] = json!(true);
    }
    message
}

/// The beginning of the long message, which is all that fits in `text`.
const LONG_MESSAGE_HEAD: &str = "# Groceries";

/// The whole of it, as the core gives it out: an HTML part, written the
/// way the pinned `deltachat-rpc-server` writes one.
///
/// The newlines matter and are why they are here. The core's own
/// template puts its head on lines of its own, and turns each newline of
/// the message into `<br/>` *followed by a newline* -- so a reader that
/// counts both gets a blank line between every line of the message. Written
/// out on one line here, that double-spacing would reach the phone with
/// nothing in the suite to notice it.
const LONG_MESSAGE_HTML: &str = "<!DOCTYPE html>\n\
     <html><head>\n\
     <meta http-equiv=\"Content-Type\" content=\"text/html; charset=utf-8\" />\n\
     <meta name=\"color-scheme\" content=\"light dark\" />\n\
     </head><body dir=\"auto\" style=\"unicode-bidi: plaintext\">\n\
     Groceries<br/>\nmilk<br/>\nbread<br/>\nand a &amp; sign<br/>\n\
     </body></html>\n";

/// True for the inputs that stand in for "the server cannot be reached".
fn should_fail(value: &str) -> bool {
    value.contains("fail")
}

/// Chat ids named in `var`, comma-separated. Lets a test mark which of
/// the seeded chats stand for something -- the chat with oneself, the
/// device chat -- without a fixture for each.
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

/// Ask the relay named in `qr` for an account.
///
/// Instant, unless the relay is a slow one: a payload containing `slow`
/// is answered only after `PIIRIT_FAKE_SLOW_MS` (three seconds by
/// default), the way a relay that is down holds the real core for as
/// long as its connection attempts take. While it is pending the account
/// is configuring, and the real core's two rules about that are kept:
/// a second transport call on the same account is refused, in the real
/// core's words, and `stop_ongoing_process` makes the pending call end
/// in failure -- unless the payload also says `deaf`, for a relay that
/// answers after all, which is what a stop that came too late looks
/// like.
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
    // The address the relay minted: this account's name on the relay
    // the payload names, which is enough for a row to be told apart by.
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

/// Take a profile over, from another device or from a file: the import
/// the real core runs, reported the way it reports one -- `ImexProgress`
/// events as it goes, then the account configured.
///
/// Keyed on what it is handed, like the transport call: `fail` refuses,
/// `slow` waits (and answers a `stop_ongoing_process` that arrives
/// meanwhile, unless it is `deaf`).
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
    // A backup carries the profile it was written from, address and
    // all, so the same backup imported twice is the same address twice.
    // Keyed on what the import was handed, which is the one thing two
    // imports of the same profile have in common here.
    let addr = backup_addr(from);
    state
        .config
        .insert((account, "configured_addr".to_string()), addr.clone());
    state.add_transport(account, &addr);
    state.configure(account);
    ok(id, &Value::Null)
}

/// One relay's block of the connectivity report, the shape the real core
/// writes (pinned in deltachat-jsonrpc/tests/real_server.rs): the domain
/// in bold, then the mailbox on a bar. Each relay's figure is its own,
/// so a page that reads another relay's bar can be seen to.
fn transport_block(addr: &str) -> String {
    let domain = addr.rsplit('@').next().unwrap_or(addr);
    // The older relay is the one the profile has drifted from: still
    // reached, but not lately, which the real core draws in yellow with
    // its own words; the rest are connected.
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

/// The address a backup carries, made up from what names it: the file
/// path or the code the other device showed. Two imports of the same
/// thing get the same address, which is what a duplicate is.
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

/// Write a profile out, the way the real core's `export_backup` does:
/// `ImexProgress` events while it runs, and a `.tar` in the folder it was
/// handed, which the core names itself.
///
/// Keyed on the folder, like the import above: `fail` refuses, `slow`
/// waits (and answers a `stop_ongoing_process` that arrives meanwhile,
/// unless it is `deaf`).
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
    // The name the real core builds: the date, a number and the address.
    // A page finds the file by what appeared in the folder, so what
    // matters is that exactly one did.
    let written = std::path::Path::new(folder).join(format!("delta-chat-2026-01-01-{account}.tar"));
    // Off the runtime's threads, as the shim does its own filesystem
    // work: `std::fs` stops the thread it is called on, and this double
    // is a server that goes on answering while it writes.
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

/// Offer a profile to a second device, the way the real core's
/// `provide_backup` does: the call stays open until a device has taken
/// the profile or the provider is stopped, and reports itself in
/// `ImexProgress` events -- 1000 once a device has it, 0 when it is
/// stopped. Only the transfer is reported, so a code sitting on screen
/// with nobody reading it emits nothing at all.
///
/// Keyed on the account rather than on a string, because the call
/// carries nothing else: `PIIRIT_FAKE_PROVIDE_FAIL` names the accounts
/// it refuses outright, `PIIRIT_FAKE_TAKEN` the ones a device turns up
/// for, `PIIRIT_FAKE_SLOW_MS` later, and `PIIRIT_FAKE_STALLED` the ones
/// a device turns up for and then goes away again. Every other account
/// waits to be stopped, which is a code left up on a page.
async fn provide_backup(state: &Arc<Mutex<State>>, id: &Value, account: u32) -> Value {
    if env_ids("PIIRIT_FAKE_PROVIDE_FAIL").contains(&u64::from(account)) {
        return err(id, "backup could not be offered");
    }
    if !state.lock().await.configuring.insert(account) {
        return err(id, "There is already another ongoing process running.");
    }
    let stalls = env_ids("PIIRIT_FAKE_STALLED").contains(&u64::from(account));
    let taken = if stalls || env_ids("PIIRIT_FAKE_TAKEN").contains(&u64::from(account)) {
        // A device turns up half a wait in, and the transfer has a
        // middle: the bar the page draws has to have something to draw
        // before the 1000 that ends it, and a test reading the page
        // mid-transfer needs room on both sides of that.
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
        // `Ok`, like the real core: a provider that was stopped, and one
        // whose device went away, both end with the provider having
        // finished and nothing to say about it. Only the progress above
        // tells them from the 1000 below.
        return ok(id, &Value::Null);
    }
    state.imex(account, 1000);
    ok(id, &Value::Null)
}

/// Wait for a `stop_ongoing_process` on this account.
///
/// The real provider holds its call open for as long as the code is up,
/// so this one does too rather than answering a page that is still
/// showing a code. Bounded at half a minute so that a test which never
/// stops it ends by itself instead of holding a request open until the
/// process is killed.
async fn wait_for_stop(state: &Arc<Mutex<State>>, account: u32) {
    for _ in 0..600 {
        if state.lock().await.stopped.contains(&account) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// The code a running provider shows, as the real core's
/// `get_backup_qr` answers it: a `DCBACKUP5:` payload -- the transfer
/// version the pinned core speaks -- that means nothing to anything but
/// the core at the other end. Named for the account, so two profiles
/// offered in turn do not show one code.
///
/// The real call blocks until the provider is ready and fails after a
/// minute rather than deadlocking; this one waits for the provider to
/// have started, in much less than a minute, and fails the accounts
/// named in `PIIRIT_FAKE_NO_QR` outright -- which is a provider running
/// behind a page with nothing on it to read.
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

/// A reply delay in milliseconds, from `var`, or `default` when unset.
fn delay_or(var: &str, default: u64) -> std::time::Duration {
    std::time::Duration::from_millis(
        std::env::var(var)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(default),
    )
}

/// A reply delay in milliseconds, from `var`. Lets a test fix the order in
/// which two replies land.
fn delay(var: &str) -> std::time::Duration {
    delay_or(var, 0)
}

fn main() {
    // By hand rather than `#[tokio::main]`, which is the `macros` feature
    // and a proc-macro crate the app's own build does not need.
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
    // Seeded before the first request: a page asking about a profile
    // before anything has read the account list finds it there.
    let mut seeded = State::default();
    seeded.seed_accounts();
    let state = Arc::new(Mutex::new(seeded));
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));

    // Events of whatever kind is asked for, queued before anything else
    // and with no payload beyond the kind. The real core cannot be made to
    // say a particular thing on demand, and what an event of an unread kind
    // costs the app is exactly what `HANDLED_EVENT_KINDS` is about, so a
    // test names the kinds it wants to see arrive.
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
    // Stands in for the server dying under the client.
    if let Ok(after) = std::env::var("PIIRIT_FAKE_EXIT_AFTER_MS") {
        if let Ok(millis) = after.parse() {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                std::process::exit(0);
            });
        }
    }
    // A message that sorts into the middle of the first chat rather than
    // at its end, this long after the server starts. What a test of the
    // message model needs to see the core do, and cannot ask for over
    // the wire: nothing the app calls puts a message anywhere but last.
    if let Ok(after) = std::env::var("PIIRIT_FAKE_LATE_ARRIVAL_MS") {
        if let Ok(millis) = after.parse() {
            let state = state.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                state.lock().await.add_late_message(1, 1);
            });
        }
    }
    // A call ringing in, this long after the server starts: from the
    // first chat's other end, audio only, with an offer the page would
    // answer. The real core says `IncomingCall` for it, and `MsgsChanged`
    // for its message.
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
                    // Seeded again when there is nothing left: a test
                    // that deletes every profile and asks again gets the
                    // ones it started with, as it always has.
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
                    // The real core deletes the account's directory, and
                    // its transports and config with it.
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
                    // Null clears, as the real core does -- and an empty
                    // string is a value, not a clear. `selfavatar` refuses
                    // one outright there, which is why the app has to send
                    // null rather than "".
                    match positional(2).as_str() {
                        // The profile's own address. The real core takes
                        // only an address it has a transport for, in
                        // these words, and since 2.61 even on an account
                        // with none: writing one is no longer a way of
                        // making a transport. Nor does it clear one.
                        // Both pinned offline against the pinned binary.
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
                    // The `sys.` keys are the core's own answers rather
                    // than anything stored: it computes this one, and a
                    // test that wants another number sets it.
                    let default = (key == RECOMMENDED_SIZE_KEY).then(recommended_size);
                    let value = state
                        .config
                        .get(&(account, key))
                        .cloned()
                        .or(default)
                        .map_or(Value::Null, Value::String);
                    ok(&id, &value)
                }
                // Read and unread, as the list marks them. The real core
                // announces both, and the announcements are what every
                // badge follows; `get_fresh_msgs` is what the profiles
                // page counts.
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
                    // The real core (chat.rs, `markfresh_chat`) puts the
                    // newest incoming message back to fresh and announces
                    // the chat changed, with no message named, and its
                    // list item changed.
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
                // Where the reader left off: the last run of messages
                // that are not seen, as the real core reckons it --
                // walking back from the newest and stopping at the first
                // seen one. Read off the same message states
                // `message_object` gives out, so the fake cannot disagree
                // with itself.
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
                | "start_io_for_all_accounts"
                | "stop_io_for_all_accounts"
                | "maybe_network"
                | "markseen_msgs"
                | "set_chat_visibility"
                | "resend_messages" => ok(&id, &Value::Null),
                "stop_ongoing_process" => {
                    state.lock().await.stopped.insert(account_id());
                    ok(&id, &Value::Null)
                }
                "add_transport_from_qr" => {
                    let qr = positional(1).as_str().unwrap_or_default().to_string();
                    add_transport_from_qr(&state, &id, account_id(), &qr).await
                }
                // Both are the core's import, and both are keyed on
                // what they are handed: the code the other device shows,
                // or the path of a backup file.
                "get_backup" | "import_backup" => {
                    let from = positional(1).as_str().unwrap_or_default().to_string();
                    import_into(&state, &id, account_id(), &from).await
                }
                // The other end of `get_backup`: this phone holding
                // the profile and offering it to a device that reads
                // the code it shows.
                "provide_backup" => provide_backup(&state, &id, account_id()).await,
                "get_backup_qr" => backup_qr(&state, &id, account_id()).await,
                // The other direction: one profile written to a folder.
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
                        // Matches the real core: a malformed
                        // EnteredLoginParam is an invalid-params error.
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
                // The account's transports, oldest first, in the shape
                // the real core lists them: an `EnteredLoginParam` each,
                // of which the app reads `addr`.
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
                // One transport fewer. The real core refuses to remove
                // the last one and one it does not have, in these words,
                // and answers the removal of the one with the account's
                // own address on it by electing the newest of the rest
                // -- upstream's `delete_transport` and
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
                    // DC_GCL_ADD_SELF puts the account's own contact last,
                    // and only when nothing is being searched for -- as
                    // the real core does, pinned in real_server.rs.
                    if flags & 0x02 != 0 && query.is_empty() {
                        ids.push(SELF);
                    }
                    let contacts: Vec<Value> = ids
                        .iter()
                        .filter_map(|contact| state.contact_object(*contact))
                        .collect();
                    ok(&id, &Value::Array(contacts))
                }
                // A name of the reader's own for a contact; empty puts the
                // contact's own back. Announced the way the real core
                // announces any contact change.
                // The other contact list the core keeps: the blocked
                // ones, which no flag on `get_contacts` brings back.
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
                // Blocking and letting back in. Both answer with nothing
                // and announce the change, as the real core does.
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
                // The connection and the mailbox, as the profile page
                // reads them: a band, and a report written for a web view
                // with the quota on a bar in it -- the shape the real core
                // writes, pinned in deltachat-jsonrpc/tests/real_server.rs.
                "get_connectivity" => ok(&id, &json!(4000)),
                "get_connectivity_html" => {
                    // The core reports every transport, oldest first,
                    // each with its own quota. A profile set up twice
                    // has the older relay's bar before its own. Each
                    // relay's figure is its own, so a page that reads
                    // the wrong bar can be seen to.
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
                // The rest of a message held back by the download limit.
                // One message, as `get_messages` gives them out. What
                // the reader page asks for before it asks whether there
                // is more of it.
                "get_message" => {
                    let msg = positional(1).as_u64().unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    ok(&id, &state.full_message(msg))
                }
                // The whole of a message the sending core cut. Only the
                // one seeded as cut has one; every other message is
                // already whole, and the core answers null for those.
                "get_message_html" => {
                    let msg = positional(1).as_u64().unwrap_or_default();
                    if msg == 11 {
                        ok(&id, &json!(LONG_MESSAGE_HTML))
                    } else {
                        ok(&id, &Value::Null)
                    }
                }
                // The real core fetches it and announces the message
                // changed; here the fetch is instant.
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
                // This account's reactions on a message, the whole list
                // at once: an empty one takes them off. Announced with the
                // event the real core sends, and answered with the id of
                // the hidden message that carries the reaction -- which
                // is not in any chat's list.
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
                // A join and a one-to-one both end in a fresh chat at the top.
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
                // A chat as ChatInfo reads it. Members
                // are ids; the contacts behind them come from
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
                    // The real core's words for a chat that is not there.
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
                                // Both false once the account has left, as
                                // the real core answers; a one-to-one chat
                                // is not a group the account is "in" at all.
                                "selfInGroup": is_group && !left,
                                "canSend": !left,
                                "ephemeralTimer": state.timers.get(&chat).copied().unwrap_or(0),
                            }),
                        )
                    }
                }
                // Who is in a chat, ids only: what a conversation header
                // counts. The same members `get_full_chat_by_id` names.
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
                    // The real core's two refusals, in its words: an empty
                    // name, and a chat that is not a group it is in.
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
                "set_chat_profile_image" => {
                    let account = account_id();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Null clears, as the real core does.
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
                    // Removing someone who is not in the group is not an
                    // error to the real core either.
                    if let Some(members) = state.group_members.get_mut(&chat) {
                        members.retain(|member| *member != contact);
                    }
                    if contact == SELF {
                        state.left_groups.insert(chat);
                    }
                    state.chat_modified(account, chat);
                    ok(&id, &Value::Null)
                }
                // Verified against the real core: accepted on any chat,
                // and announced with its own event carrying the timer.
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
                // A fingerprint code with neither of an invite's two
                // secrets in it has had no kind since 2.61: the core looks
                // for a contact with that key and refuses the code when
                // there is none, which here is every time.
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
                    // Enough to tell an invite from anything else, which is
                    // the only distinction the shim makes.
                    let kind = if content.contains("i.delta.chat")
                        || content.starts_with("OPENPGP4FPR:")
                    {
                        "askVerifyContact"
                    } else if content.starts_with("dcaccount:") || content.starts_with("DCACCOUNT:")
                    {
                        "account"
                    } else if content.starts_with("DCBACKUP2:") {
                        // What a device offering itself as a first
                        // device shows; `toonew` in it stands in for a
                        // backup from a newer Delta Chat than this core.
                        if content.contains("toonew") {
                            "backupTooNew"
                        } else {
                            "backup2"
                        }
                    } else {
                        "text"
                    };
                    ok(&id, &json!({"kind": kind}))
                }
                "get_chat_securejoin_qr_code" => ok(
                    &id,
                    &json!("https://i.delta.chat/#ABCDEF&a=me%40example.org&n=Me"),
                ),
                // Both ways a message goes: only here, or here and
                // at every other end. The second sends a deletion
                // request out as a message of its own; nothing in
                // this server delivers anything, so what it does to
                // the chat is what the first does.
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
                    // The core announces a deletion; the model reloads on it.
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
                    // Chat 2 is the group, so a test has both kinds.
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
                // Whether the account can write into a chat: not into a
                // group it has left, as `get_full_chat_by_id` says too.
                "can_send" => {
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    ok(&id, &json!(!state.left_groups.contains(&chat)))
                }
                // Replace the text of a message sent from here. The real
                // core refuses an empty text and somebody else's message;
                // the menu offers neither, so only the first is refused
                // here, and a text asking to fail fails the way a send
                // does. The change is announced as the real core announces
                // it, so a model re-reads the row.
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
                // How many messages a deletion period would take now: every
                // message in every chat older than it, which with the seeded
                // timestamps is all of them for any period a page offers.
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
                // Mute a chat, or unmute it. The real core keeps a duration
                // and announces the change as a `ChatModified`; the list
                // reads `isMuted` back off the row.
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
                // One contact by id, the account's own included: the
                // profile page asks for its colour this way.
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
                    // DC_GCL_ARCHIVED_ONLY. The two listings are disjoint,
                    // so this is which list, not a filter on one.
                    let archived_only = positional(1).as_u64().unwrap_or(0) & 0x01 != 0;
                    // Verified against the real core: with ARCHIVED_ONLY
                    // set it never looks at the query, and a plain query
                    // searches every chat *including* archived ones. A
                    // fake that filtered the archived list by the query
                    // would let a one-call implementation pass here and
                    // fail on a device.
                    let query = positional(2).as_str().unwrap_or("").to_string();
                    let searching = !query.is_empty();
                    let entries = if archived_only {
                        // The query is deliberately not consulted.
                        state.archived_order.clone()
                    } else if searching {
                        // A plain query reaches archived chats too.
                        let mut all = state.chat_order.clone();
                        all.extend(state.archived_order.iter().copied());
                        all
                    } else {
                        state.chat_order.clone()
                    };
                    // Lets a test make the *ordinary* listing the slow one,
                    // so an answer to a question the model has already
                    // moved on from arrives last. Requests are handled
                    // concurrently here, as the real core's are, so this
                    // delays only its own reply.
                    if !archived_only {
                        if let Some(delay) = std::env::var("PIIRIT_FAKE_CHATLIST_DELAY_MS")
                            .ok()
                            .and_then(|value| value.parse::<u64>().ok())
                        {
                            drop(state);
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                    }
                    // The core matches the query itself; so does this, on
                    // the same name a chat item reports.
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
                    let account = account_id();
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    let state = state.lock().await;
                    let mut items = serde_json::Map::new();
                    for chat in ids {
                        let fresh = u32::try_from(chat)
                            .ok()
                            .is_some_and(|chat| state.fresh.contains(&(account, chat)));
                        // A chat holding a draft previews the draft, and
                        // names it in the prefix the row shows in front:
                        // "Draft", and DC_STATE_OUT_DRAFT for the state.
                        // The real core does this itself, which is pinned
                        // in deltachat-jsonrpc/tests/real_server.rs.
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
                                // The chat with oneself and the core's
                                // device chat, when a test names them:
                                // what the cover leaves out of its grid.
                                "isSelfTalk": env_ids("PIIRIT_FAKE_SELF_TALK").contains(&chat),
                                "isDeviceTalk": env_ids("PIIRIT_FAKE_DEVICE_TALK").contains(&chat),
                                // Chat 1 is pinned, so the ordinary list
                                // has both kinds in it and a test can see
                                // the two headings. The archived list
                                // holds one unpinned chat, which is the
                                // other case: one kind, no headings.
                                "isPinned": chat == 1,
                            }),
                        );
                    }
                    ok(&id, &Value::Object(items))
                }
                "search_messages" => {
                    let mut state = state.lock().await;
                    state.seed_chats();
                    // Three arguments; the third is a chat to search
                    // within, and null means every chat. Verified against
                    // the pinned binary: passing two is rejected.
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
                    // Delayed with the same knob as get_messages: a real
                    // core takes time over both, and a test that wants two
                    // of these in the air at once needs them to overlap.
                    tokio::time::sleep(delay("PIIRIT_FAKE_FETCH_DELAY_MS")).await;
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let chat = positional(1)
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or_default();
                    // The fourth argument asks for day markers. The real
                    // core interleaves one before each day's first message,
                    // and the model reads a placeholder row's day off them.
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
                // The ids of every message of up to three kinds in one
                // chat -- or, for a null chat, in every chat -- oldest
                // first, as the real core answers and asks not to have
                // re-sorted. What the media pages are built on.
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
                    // One call for many ids: the point of the batch.
                    let ids: Vec<u64> = positional(1)
                        .as_array()
                        .map(|array| array.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    let mut state = state.lock().await;
                    state.seed_chats();
                    let mut loaded = serde_json::Map::new();
                    for msg in ids {
                        loaded.insert(msg.to_string(), state.full_message(msg));
                    }
                    ok(&id, &Value::Object(loaded))
                }
                // Drafts, which the core keeps per chat. Enough of them to
                // drive the page: one text per chat, read back and cleared.
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
                    // Null for none and a whole message object for one, as
                    // the real core answers.
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
                    // The core takes one file and decides the message's
                    // view type from it. Echoed back so the row the sender
                    // sees carries the attachment, as the real one does.
                    let file = positional(3);
                    let file_name = positional(4);
                    let quoted = positional(6);
                    if should_fail(&text) {
                        // The real core reports a failed send as an Error
                        // event, not only as a failed call.
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
                        // The event is queued above, so a delay here puts it
                        // ahead of this call's own reply -- the ordering the
                        // real core can produce, and the one that duplicated a
                        // sent row.
                        tokio::time::sleep(delay("PIIRIT_FAKE_SEND_DELAY_MS")).await;
                        // The real core reads the file; this reads the
                        // extension, which is enough to tell an image row
                        // from a paperclip one.
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
                        // Remembered for the same reason `send_msg` does:
                        // the row is fetched again after it is sent, and
                        // a message that arrived as an app has to still
                        // be one then.
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
                // The other send: a MessageData object, which is how a view
                // type the core would not pick itself -- a voice message
                // -- is asked for. Answers with the id alone; the row is
                // fetched afterwards, and `get_messages` names the type
                // the send asked for.
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
                        // Held back the way the other send is, so a test
                        // about a send in flight covers both of them.
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
                // Anything off the web, fetched by the core rather than by
                // the app: how an app is taken from the store.
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
                // What the app is called and what it says about itself.
                // The summary counts the updates sent to it, so a test
                // can watch a row follow the chat.
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
                // One file out of the archive, base64 as the real core
                // encodes it (STANDARD_NO_PAD). Two files exist here: the
                // page and its icon.
                "get_webxdc_blob" => {
                    let path = positional(2).as_str().unwrap_or_default().to_string();
                    match webxdc_file(&path) {
                        Some(bytes) => ok(&id, &json!(base64(&bytes))),
                        None => err(&id, "no such file in the webxdc"),
                    }
                }
                // Everything after the serial asked for, as one JSON
                // *string*: the real core hands the array over already
                // encoded.
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
                        // The core hands an update back with the two
                        // serials on it, and the app's own send is one of
                        // the updates it then receives.
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
                // The call methods, kept to what the real core does with
                // each (src/calls.rs upstream): a call is a message, and
                // every move of its state is announced.
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
                        // An offer that asks for it is answered at once,
                        // as the other end's device would.
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
                        // Ended already: the real core says so in its log
                        // and does nothing.
                        Some(_) => Some(false),
                    };
                    match ended {
                        None => err(&id, "end_call is called with a message that is not a call"),
                        Some(moved) => {
                            if moved {
                                state.call_event(account, "CallEnded", msg, &json!({}));
                                state.call_changed(account, msg);
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
                    // Blocks when empty, like the real long poll.
                    loop {
                        let queued: Vec<Value> = state.lock().await.events.drain(..).collect();
                        if !queued.is_empty() {
                            // Held back after the batch is taken, not
                            // before the wait: a delay ahead of the loop
                            // would be spent while the queue was still
                            // empty and buy nothing. This is what lets a
                            // test say that a call's own reply is dealt
                            // with before the event the same call
                            // produced -- otherwise which of the two wins
                            // is a race between queued callbacks on the
                            // Qt thread, and a test that quietly depends
                            // on one of them passes on one machine and
                            // fails on another.
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
