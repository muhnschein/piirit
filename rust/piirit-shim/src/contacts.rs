//! Contacts; ways to start a chat.

use std::cell::RefCell;

use deltachat_jsonrpc::RpcError;
use qmetaobject::*;

use crate::core::{connection, reconcile_rows};
use crate::json;
use crate::models::{ContactItem, ContactListModel};

/// Known unblocked contacts (or blocked, per `blocked`); calls that start chats.
///
/// ```qml
/// ContactList { id: contacts; account_id: page.accountId }
/// SilicaListView { model: contacts.rows }
/// ```
#[derive(QObject, Default)]
pub struct ContactList {
    base: qt_base_class!(trait QObject),

    /// Account. Setting reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Account changed.
    pub account_changed: qt_signal!(),

    /// Filter; core matches name and address.
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Query changed.
    pub query_changed: qt_signal!(),

    /// Include own contact, last; pickers leave off.
    pub include_self: qt_property!(bool; WRITE set_include_self NOTIFY include_self_changed),
    /// `include_self` changed.
    pub include_self_changed: qt_signal!(),

    /// Blocked contacts: separate core list, not a filter (absent from `get_contacts` and
    /// pickers). Ignores `query`, `include_self`: core call takes neither.
    pub blocked: qt_property!(bool; WRITE set_blocked NOTIFY blocked_changed),
    /// `blocked` changed.
    pub blocked_changed: qt_signal!(),

    /// Rows for `SilicaListView`.
    pub rows: qt_property!(RefCell<ContactListModel>; CONST),

    /// Row count.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted after `rows` change.
    pub rows_changed: qt_signal!(),

    /// Failed; core's message.
    pub error: qt_signal!(message: QString),

    /// Reload list.
    pub reload: qt_method!(fn(&mut self)),

    /// One-to-one chat, created if needed. Answers `chat_ready`.
    pub open_chat_with: qt_method!(fn(&mut self, contact_id: u32)),

    /// Create and open group; picture after (core needs existing chat). Answers `chat_ready`.
    pub create_group:
        qt_method!(fn(&mut self, name: QString, member_ids: QVariantList, picture_path: QString)),

    /// Create and open own channel (core's broadcast); picture after, as group. No members:
    /// core refuses adding any, subscribers join by channel's invite. Answers `chat_ready`.
    pub create_channel: qt_method!(fn(&mut self, name: QString, picture_path: QString)),

    /// Follow invite (QR or `https://i.delta.chat/...`), open chat: how contacts are added, as
    /// bare address can't be encrypted to (docs/PROJECT.md); groups, channels joined same way.
    /// Answers `chat_ready`, or `relay_offered` for relay code.
    pub join_by_invite: qt_method!(fn(&mut self, qr_content: QString)),
    /// `join_by_invite` got relay code (`dcaccount:`, `dclogin:`): add to this profile, as
    /// reference clients since 2.61. `relay`: core's relay name or address.
    pub relay_offered: qt_signal!(qr_content: QString, relay: QString),

    /// Group rows: own contact first (if listed), then `contact_ids` in pick order; unlisted
    /// skipped. Maps, not model filter: hiding built a row and avatar per contact every pick.
    pub picked_rows: qt_method!(fn(&self, contact_ids: QVariantList) -> QVariantList),

    /// Per account: nothing they send arrives, no picker offers them.
    pub block: qt_method!(fn(&mut self, contact_id: u32)),

    /// Hear from contact again.
    pub unblock: qt_method!(fn(&mut self, contact_id: u32)),

    /// Block applied; list re-asked. `blocked`: which way.
    pub blocking_applied: qt_signal!(contact_id: u32, blocked: bool),

    /// Feed `core_event`; other accounts ignored. Blocks (any device) arrive as
    /// `ContactsChanged`.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Fetch own invite. Answers `invite_ready`.
    pub fetch_invite: qt_method!(fn(&mut self)),
    /// Fetch group's or channel's invite: how subscribers join a channel. Answers
    /// `invite_ready`.
    pub fetch_chat_invite: qt_method!(fn(&mut self, chat_id: u32)),
    /// Account's invite link.
    pub invite_ready: qt_signal!(link: QString),

    /// Chat ready to show.
    pub chat_ready: qt_signal!(chat_id: u32),

    /// Counts loads: out-of-order stale answer must not land.
    generation: u64,
}

/// How an answer lands in `rows`.
#[derive(Clone, Copy)]
enum Fill {
    /// New list: rows replaced, view to top.
    Replace,
    /// Same list re-read: changed rows only, scroll kept.
    InPlace,
}

impl ContactList {
    /// Row count.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set account; reload if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.load(Fill::Replace);
        }
    }

    /// Set filter; reload if changed.
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.load(Fill::Replace);
        }
    }

    /// Include own contact, or not.
    pub fn set_include_self(&mut self, include_self: bool) {
        if self.include_self != include_self {
            self.include_self = include_self;
            self.include_self_changed();
            self.load(Fill::Replace);
        }
    }

    /// Blocked or writable contacts.
    pub fn set_blocked(&mut self, blocked: bool) {
        if self.blocked != blocked {
            self.blocked = blocked;
            self.blocked_changed();
            self.load(Fill::Replace);
        }
    }

    /// Reload list.
    pub fn reload(&mut self) {
        self.load(Fill::InPlace);
    }

    /// Ask core for list; answer into `rows`.
    fn load(&mut self, fill: Fill) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let query = self.query.to_string();
        // listFlags: 0 known unblocked; DC_GCL_ADD_SELF (2) adds own contact last.
        let list_flags: u32 = if self.include_self { 2 } else { 0 };
        let blocked = self.blocked;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ContactItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Stale: answered after newer query asked.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(items) => {
                    {
                        let this_ref = this.borrow();
                        let mut rows = this_ref.rows.borrow_mut();
                        match fill {
                            Fill::Replace => rows.reset_data(items),
                            Fill::InPlace => {
                                reconcile_rows(&mut rows, items, |item| item.contact_id);
                            }
                        }
                    }
                    this.borrow().rows_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            // Blocked list has own core method; no flags or query.
            let contacts = if blocked {
                rpc.call::<_, Vec<serde_json::Value>>("get_blocked_contacts", (account_id,))
                    .await
            } else {
                let query = if query.is_empty() { None } else { Some(query) };
                rpc.call::<_, Vec<serde_json::Value>>(
                    "get_contacts",
                    (account_id, list_flags, query),
                )
                .await
            };
            let result = contacts
                .map(|contacts| contacts.iter().map(contact_row).collect())
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Stop hearing from contact.
    pub fn block(&mut self, contact_id: u32) {
        self.set_blocking(contact_id, true);
    }

    /// Hear from contact again.
    pub fn unblock(&mut self, contact_id: u32) {
        self.set_blocking(contact_id, false);
    }

    /// Block or unblock, re-read list.
    fn set_blocking(&mut self, contact_id: u32, blocked: bool) {
        let account_id = self.account_id;
        if account_id == 0 || contact_id == 0 {
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
                Ok(()) => {
                    this.borrow_mut().reload();
                    this.borrow().blocking_applied(contact_id, blocked);
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let method = if blocked {
            "block_contact"
        } else {
            "unblock_contact"
        };
        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(method, (account_id, contact_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, _payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        // Added, renamed, (un)blocked, or overflow: re-read all.
        if matches!(
            kind.to_string().as_str(),
            "ContactsChanged" | "EventChannelOverflow"
        ) {
            self.reload();
        }
    }

    /// Own row, then picked ones.
    pub fn picked_rows(&self, contact_ids: QVariantList) -> QVariantList {
        let rows = self.rows.borrow();
        let mut picked = QVariantList::default();
        if let Some(own) = rows.iter().find(|item| item.is_self) {
            picked.push(QVariant::from(row_map(own)));
        }
        // Read back as `create_group` does: QML passes variants.
        let wanted = contact_ids
            .into_iter()
            .filter_map(|value| i32::from_qvariant(value.clone()))
            .filter_map(|value| u32::try_from(value).ok());
        for contact_id in wanted {
            if let Some(item) = rows
                .iter()
                .find(|item| item.contact_id == contact_id && !item.is_self)
            {
                picked.push(QVariant::from(row_map(item)));
            }
        }
        picked
    }

    /// One-to-one chat with contact.
    pub fn open_chat_with(&mut self, contact_id: u32) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.chat_callback();

        runtime.spawn(async move {
            let result = rpc
                .call::<_, u32>("create_chat_by_contact_id", (account_id, contact_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Create group, add members, set picture.
    pub fn create_group(&mut self, name: QString, member_ids: QVariantList, picture_path: QString) {
        let members: Vec<u32> = member_ids
            .into_iter()
            .filter_map(|value| i32::from_qvariant(value.clone()))
            .filter_map(|value| u32::try_from(value).ok())
            .collect();
        self.create(Made::Group(members), name, picture_path);
    }

    /// Create channel, set picture.
    pub fn create_channel(&mut self, name: QString, picture_path: QString) {
        self.create(Made::Channel, name, picture_path);
    }

    /// Create group or channel, add members, set picture; open it even if some refused.
    fn create(&mut self, made: Made, name: QString, picture_path: QString) {
        let account_id = self.account_id;
        // Channel's only possible refusal: picture.
        let partly = match made {
            Made::Group(_) => "the group was made, but some people could not be added",
            Made::Channel => "the channel was made, but its picture could not be set",
        };
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(u32, Vec<String>), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok((chat_id, refused)) => {
                    // Report, then open: chat exists; error on picker strands reader.
                    if !refused.is_empty() {
                        this.borrow()
                            .error(format!("{partly} ({})", refused.join("; ")).into());
                    }
                    this.borrow().chat_ready(chat_id);
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let name = name.to_string();
        let picture = picture_path.to_string();
        let picture = if picture.is_empty() {
            None
        } else {
            Some(crate::chat::local_path(&picture))
        };
        runtime.spawn(async move {
            let result = async {
                let (chat_id, members) = match made {
                    // Encrypted key-contact group, as reference "New Group". Third arg:
                    // deprecated unread `protect`, upstream says `false`.
                    Made::Group(members) => (
                        rpc.call::<_, u32>("create_group_chat", (account_id, name, false))
                            .await
                            .map_err(|err| err.to_string())?,
                        members,
                    ),
                    // Reference "New Channel".
                    Made::Channel => (
                        rpc.call::<_, u32>("create_broadcast", (account_id, name))
                            .await
                            .map_err(|err| err.to_string())?,
                        Vec::new(),
                    ),
                };
                // Try every member; chat returned anyway.
                let mut refused = Vec::new();
                for member in members {
                    if let Err(err) = rpc
                        .call::<_, ()>("add_contact_to_chat", (account_id, chat_id, member))
                        .await
                    {
                        refused.push(format!("{member}: {err}"));
                    }
                }
                // Picture last, on existing chat; refusal reported like member's.
                if let Some(path) = picture {
                    if let Err(err) = rpc
                        .call::<_, ()>("set_chat_profile_image", (account_id, chat_id, Some(path)))
                        .await
                    {
                        refused.push(format!("picture: {err}"));
                    }
                }
                Ok::<_, String>((chat_id, refused))
            }
            .await;
            done(result);
        });
    }

    /// Follow invite and open chat.
    pub fn join_by_invite(&mut self, qr_content: QString) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Followed, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(Followed::Chat(chat_id)) => this.borrow().chat_ready(chat_id),
                Ok(Followed::Relay(qr, relay)) => {
                    this.borrow().relay_offered(qr.into(), relay.into());
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let qr_content = qr_content.to_string();
        runtime.spawn(async move {
            let result = async {
                // Core decides payload kind; guessing is protocol work (docs/PROJECT.md).
                // Unreadable: "not an invite" (since 2.61 incl. fingerprint without invite,
                // unknown key). Lost connection passed on.
                let qr: serde_json::Value = rpc
                    .call("check_qr", (account_id, qr_content.clone()))
                    .await
                    .map_err(|err| match err {
                        RpcError::Remote(refusal) => not_an_invite(&refusal.message),
                        err => err.to_string(),
                    })?;
                let kind = json::str_at(&qr, "kind");
                // Relay code goes to page, which asks before adding.
                let relay = match kind {
                    "account" => Some(json::str_at(&qr, "domain")),
                    "login" => Some(json::str_at(&qr, "address")),
                    _ => None,
                };
                if let Some(relay) = relay {
                    return Ok(Followed::Relay(qr_content, relay.to_string()));
                }
                // Channel: only way in, owner can't add subscribers.
                if !matches!(
                    kind,
                    "askVerifyContact" | "askVerifyGroup" | "askJoinBroadcast"
                ) {
                    return Err(not_an_invite(kind));
                }
                // Returns once chat exists; handshake in background.
                rpc.call::<_, u32>("secure_join", (account_id, qr_content))
                    .await
                    .map(Followed::Chat)
                    .map_err(|err| err.to_string())
            }
            .await;
            done(result);
        });
    }

    /// Fetch own invite link.
    pub fn fetch_invite(&mut self) {
        self.fetch_invite_for(None);
    }

    /// Fetch group's or channel's invite link.
    pub fn fetch_chat_invite(&mut self, chat_id: u32) {
        self.fetch_invite_for(Some(chat_id));
    }

    /// Invite link: own (`None`) or chat's.
    fn fetch_invite_for(&mut self, chat_id: Option<u32>) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(link) => this.borrow().invite_ready(link.into()),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            // Null chat: own invite. Chat id: that group's or channel's.
            let result = rpc
                .call::<_, String>("get_chat_securejoin_qr_code", (account_id, chat_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Completion shared by the three chat-opening methods.
    fn chat_callback(&self) -> impl Fn(Result<u32, String>) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: Result<u32, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(chat_id) => this.borrow().chat_ready(chat_id),
                Err(err) => this.borrow().error(err.into()),
            }
        })
    }
}

/// What `ContactList::create` makes.
enum Made {
    /// Group with these members besides self.
    Group(Vec<u32>),
    /// Own channel; no members.
    Channel,
}

/// Scan result: chat, or relay.
enum Followed {
    /// Chat the invite opened.
    Chat(u32),
    /// Code and its relay.
    Relay(String, String),
}

/// Payload not followed: not an invite. Core's word in brackets.
fn not_an_invite(what: &str) -> String {
    format!("that link is not a contact, group or channel invite ({what})")
}

/// `DC_CONTACT_ID_SELF`: own contact. Not in `get_contacts`, in every group.
pub(crate) const SELF_CONTACT_ID: u32 = 1;

/// Contact as QML map keyed by role names, so both row kinds read alike.
fn row_map(item: &ContactItem) -> QVariantMap {
    let mut row = QVariantMap::default();
    row.insert("contact_id".into(), QVariant::from(item.contact_id));
    row.insert("display_name".into(), QVariant::from(&item.display_name));
    row.insert("address".into(), QVariant::from(&item.address));
    row.insert("is_key_contact".into(), QVariant::from(item.is_key_contact));
    row.insert("is_self".into(), QVariant::from(item.is_self));
    row.insert("color".into(), QVariant::from(&item.color));
    row.insert("avatar_path".into(), QVariant::from(&item.avatar_path));
    row
}

/// One row from core's contact object.
pub(crate) fn contact_row(contact: &serde_json::Value) -> ContactItem {
    let address = json::str_at(contact, "address");
    let display_name = match json::str_at(contact, "displayName") {
        "" => address,
        name => name,
    };
    let contact_id = json::u32_at(contact, "id");
    ContactItem {
        contact_id,
        display_name: display_name.into(),
        name: json::text(contact, "name"),
        auth_name: json::text(contact, "authName"),
        address: address.into(),
        is_key_contact: json::flag(contact, "isKeyContact"),
        is_self: contact_id == SELF_CONTACT_ID,
        status: json::text(contact, "status"),
        // `color` pinned by integration test; picture key isn't (rename -> initial).
        color: json::text(contact, "color"),
        avatar_path: json::text(contact, "profileImage"),
        // Since 2.61, in place of `wasSeenRecently`.
        seen_long_ago: json::str_at(contact, "freshness") == "Old",
    }
}

#[cfg(test)]
mod tests {
    use super::contact_row;
    use serde_json::json;

    /// Only core's "Old" means not seen for long.
    #[test]
    fn only_old_freshness_is_seen_long_ago() {
        let seen = |freshness: serde_json::Value| {
            contact_row(&json!({"id": 10, "address": "ada@example.org", "freshness": freshness}))
                .seen_long_ago
        };
        assert!(seen(json!("Old")));
        assert!(!seen(json!("Normal")));
        assert!(!seen(json!("RecentlySeen")));
        assert!(!seen(serde_json::Value::Null));
    }
}
