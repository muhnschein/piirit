//! Contacts, and the ways a chat gets started.
//!
//! Until this existed the app could only show conversations that arrived on
//! their own: nothing created one.

use std::cell::RefCell;

use deltachat_jsonrpc::RpcError;
use qmetaobject::*;

use crate::core::{connection, reconcile_rows};
use crate::json;
use crate::models::{ContactItem, ContactListModel};

/// Known, unblocked contacts, and the calls that turn one into a chat.
///
/// With `blocked` set it lists the account's blocked contacts instead,
/// which is the other list the core keeps and the one the blocked
/// contacts page shows.
///
/// ```qml
/// ContactList { id: contacts; account_id: page.accountId }
/// SilicaListView { model: contacts.rows }
/// ```
#[derive(QObject, Default)]
pub struct ContactList {
    base: qt_base_class!(trait QObject),

    /// Whose contacts these are. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Emitted when the account changes.
    pub account_changed: qt_signal!(),

    /// Filter, matched by the core against name and address. Setting it
    /// reloads.
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Emitted when the query changes.
    pub query_changed: qt_signal!(),

    /// List the account's own contact too, after everyone else. Off by
    /// default: a picker offers other people, and the one page that
    /// draws the reader among the members is the one that asks.
    pub include_self: qt_property!(bool; WRITE set_include_self NOTIFY include_self_changed),
    /// Emitted when `include_self` changes.
    pub include_self_changed: qt_signal!(),

    /// List the contacts this account has blocked rather than the ones
    /// it can write to. The core keeps the two apart -- a blocked
    /// contact is in neither `get_contacts` nor any picker -- so this is
    /// a different list, not a filter over the one above. `query` and
    /// `include_self` say nothing here: the core's call takes neither,
    /// and a blocked list is short enough to read.
    pub blocked: qt_property!(bool; WRITE set_blocked NOTIFY blocked_changed),
    /// Emitted when `blocked` changes.
    pub blocked_changed: qt_signal!(),

    /// The rows, for a `SilicaListView`'s `model`.
    pub rows: qt_property!(RefCell<ContactListModel>; CONST),

    /// How many rows there are.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Reload the list.
    pub reload: qt_method!(fn(&mut self)),

    /// Open the one-to-one chat with a contact, creating it if needed.
    /// Answers on `chat_ready`.
    pub open_chat_with: qt_method!(fn(&mut self, contact_id: u32)),

    /// Create a group with the given name, members and picture, and open
    /// it. The picture is a path, or empty for none: the core takes one
    /// only on a chat that exists, so it is set once the group does.
    /// Answers on `chat_ready`.
    pub create_group:
        qt_method!(fn(&mut self, name: QString, member_ids: QVariantList, picture_path: QString)),

    /// Follow an invite -- a scanned QR payload or a pasted
    /// `https://i.delta.chat/...` link -- and open the chat it leads to.
    /// This is how a Delta Chat contact is normally added: an address alone
    /// cannot be encrypted to (docs/PROJECT.md). Answers on `chat_ready`.
    pub join_by_invite: qt_method!(fn(&mut self, qr_content: QString)),

    /// The rows a group being put together draws: this account's own
    /// contact first, when the list holds it, then the contacts named
    /// by `contact_ids`, in the order they were picked. An id the list
    /// does not hold is left out.
    ///
    /// A list of maps rather than a filter over the model: a page that
    /// drew the members by hiding everybody else built a row -- and an
    /// avatar, and the two effects behind it -- for every contact the
    /// reader has, twice over, and built them again on every pick. What
    /// it cost to open that page was the size of an address book that
    /// had nothing to do with the group.
    pub picked_rows: qt_method!(fn(&self, contact_ids: QVariantList) -> QVariantList),

    /// Stop hearing from a contact: nothing they send arrives, and they
    /// are not offered by anything that picks a contact. Blocking is the
    /// account's, as the core keeps it -- one profile's block list says
    /// nothing about another's.
    pub block: qt_method!(fn(&mut self, contact_id: u32)),

    /// Hear from a contact again.
    pub unblock: qt_method!(fn(&mut self, contact_id: u32)),

    /// A block was applied, and this list has been asked again. `blocked`
    /// is which way it went, for a page that says so.
    pub blocking_applied: qt_signal!(contact_id: u32, blocked: bool),

    /// Feed a `core_event` in. Events for other accounts are ignored.
    ///
    /// Blocking from another device lands here as a `ContactsChanged`,
    /// and so does the core's own answer to a block made on this one.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Fetch this account's own invite, the one to hand out. Answers on
    /// `invite_ready`.
    pub fetch_invite: qt_method!(fn(&mut self)),
    /// This account's invite link.
    pub invite_ready: qt_signal!(link: QString),

    /// A chat is ready to be shown.
    pub chat_ready: qt_signal!(chat_id: u32),

    /// Counts loads, so a slow answer to an old query cannot land on top of
    /// a newer one. Typing "anna" starts four of these and they are not
    /// answered in the order they were asked.
    generation: u64,
}

/// How an answer from the core lands in the rows.
#[derive(Clone, Copy)]
enum Fill {
    /// Another list -- another account, query or kind -- so the rows are
    /// replaced, and the view starts again from its top.
    Replace,
    /// The same list read again, after the core said it changed or a page
    /// asked: only the rows that changed are touched, so the view stays
    /// where the reader left it. Replacing them would take it back to its
    /// top, away from whatever the reader had scrolled to.
    InPlace,
}

impl ContactList {
    /// How many rows there are.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.load(Fill::Replace);
        }
    }

    /// Set the filter and reload if it changed.
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.load(Fill::Replace);
        }
    }

    /// List the account's own contact as well, or stop doing so.
    pub fn set_include_self(&mut self, include_self: bool) {
        if self.include_self != include_self {
            self.include_self = include_self;
            self.include_self_changed();
            self.load(Fill::Replace);
        }
    }

    /// Show the blocked contacts, or the ones that can be written to.
    pub fn set_blocked(&mut self, blocked: bool) {
        if self.blocked != blocked {
            self.blocked = blocked;
            self.blocked_changed();
            self.load(Fill::Replace);
        }
    }

    /// Reload the list.
    pub fn reload(&mut self) {
        self.load(Fill::InPlace);
    }

    /// Ask the core for the list, and put what it answers in `rows`.
    fn load(&mut self, fill: Fill) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let query = self.query.to_string();
        // listFlags: 0 is known, unblocked contacts; DC_GCL_ADD_SELF (2)
        // puts the account's own contact at the end of them.
        let list_flags: u32 = if self.include_self { 2 } else { 0 };
        let blocked = self.blocked;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<ContactItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answered after something newer was asked: these are results
            // for a query the reader has already typed past.
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
            // Two calls rather than a flag on one: the core lists the
            // blocked contacts through a method of its own, which takes
            // neither the flags nor the query.
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

    /// Stop hearing from a contact.
    pub fn block(&mut self, contact_id: u32) {
        self.set_blocking(contact_id, true);
    }

    /// Hear from a contact again.
    pub fn unblock(&mut self, contact_id: u32) {
        self.set_blocking(contact_id, false);
    }

    /// Block or unblock, then read the list back.
    ///
    /// Read back rather than edited in place: a block takes the contact
    /// out of one of these lists and puts it into the other, and which
    /// list this one is deciding that is the core's business, not a row
    /// removal guessed at here.
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
        // A contact added, renamed, blocked or unblocked -- from here or
        // from another device -- and the overflow that says events were
        // dropped, which is answered by reading everything again.
        if matches!(
            kind.to_string().as_str(),
            "ContactsChanged" | "EventChannelOverflow"
        ) {
            self.reload();
        }
    }

    /// The reader's own row, then the picked ones. See the declaration.
    pub fn picked_rows(&self, contact_ids: QVariantList) -> QVariantList {
        let rows = self.rows.borrow();
        let mut picked = QVariantList::default();
        if let Some(own) = rows.iter().find(|item| item.is_self) {
            picked.push(QVariant::from(row_map(own)));
        }
        // Read back the way `create_group` reads the same list: what
        // QML hands over is a list of variants, not of numbers.
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

    /// Open the one-to-one chat with a contact.
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

    /// Create a group, add the given members, and give it its picture.
    pub fn create_group(&mut self, name: QString, member_ids: QVariantList, picture_path: QString) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(u32, Vec<String>), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok((chat_id, refused)) => {
                    // Said first, then opened anyway: the group is real,
                    // and leaving the reader on the picker with an error is
                    // how one ends up stranded with no way back to it.
                    if !refused.is_empty() {
                        this.borrow().error(
                            format!(
                                "the group was made, but some people could not be added ({})",
                                refused.join("; ")
                            )
                            .into(),
                        );
                    }
                    this.borrow().chat_ready(chat_id);
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        let name = name.to_string();
        let members: Vec<u32> = member_ids
            .into_iter()
            .filter_map(|value| i32::from_qvariant(value.clone()))
            .filter_map(|value| u32::try_from(value).ok())
            .collect();
        let picture = picture_path.to_string();
        let picture = if picture.is_empty() {
            None
        } else {
            Some(crate::chat::local_path(&picture))
        };
        runtime.spawn(async move {
            let result = async {
                // Encrypted, of key-contacts, which is what the reference
                // client's "New Group" makes. It is the method that decides
                // that -- `create_group_chat_unencrypted` is the other one.
                // The third argument is upstream's deprecated `protect`,
                // which its own docs say to pass `false`; it is bound as
                // `_protect` there and read by nothing.
                let chat_id: u32 = rpc
                    .call("create_group_chat", (account_id, name, false))
                    .await
                    .map_err(|err| err.to_string())?;
                // Every member attempted, and the chat handed back either
                // way: it exists on the core from the call above, so
                // failing out of here left a half-built group the reader
                // was never shown and could not find.
                let mut refused = Vec::new();
                for member in members {
                    if let Err(err) = rpc
                        .call::<_, ()>("add_contact_to_chat", (account_id, chat_id, member))
                        .await
                    {
                        refused.push(format!("{member}: {err}"));
                    }
                }
                // The picture last, on the group that now exists. A
                // refusal here is reported like a member's: the group is
                // still made and still opened.
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

    /// Follow an invite and open the chat it leads to.
    pub fn join_by_invite(&mut self, qr_content: QString) {
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.chat_callback();

        let qr_content = qr_content.to_string();
        runtime.spawn(async move {
            let result = async {
                // Ask the core what the payload is before acting on it: it
                // knows the formats, and guessing at them here would be the
                // protocol work docs/PROJECT.md rules out.
                //
                // A payload the core will not read is not an invite either,
                // and is said so in the same words. Since 2.61 that is how
                // it answers a fingerprint code with no invite in it and a
                // key it knows no one by, which had a kind of its own
                // before. A lost connection is not an answer, and is passed
                // on as it is.
                let qr: serde_json::Value = rpc
                    .call("check_qr", (account_id, qr_content.clone()))
                    .await
                    .map_err(|err| match err {
                        RpcError::Remote(refusal) => not_an_invite(&refusal.message),
                        err => err.to_string(),
                    })?;
                let kind = json::str_at(&qr, "kind");
                if !matches!(kind, "askVerifyContact" | "askVerifyGroup") {
                    return Err(not_an_invite(kind));
                }
                // Returns as soon as the chat exists; the handshake itself
                // finishes in the background.
                rpc.call::<_, u32>("secure_join", (account_id, qr_content))
                    .await
                    .map_err(|err| err.to_string())
            }
            .await;
            done(result);
        });
    }

    /// Fetch this account's own invite link.
    pub fn fetch_invite(&mut self) {
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
            // A null chat gives the account's own contact invite; a chat id
            // would give that group's.
            let result = rpc
                .call::<_, String>(
                    "get_chat_securejoin_qr_code",
                    (account_id, Option::<u32>::None),
                )
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// The shared completion path of the three chat-opening methods.
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

/// Why a payload was not followed: it is not an invite. The core's own
/// word on it goes in brackets -- the kind it named, or why it would not
/// read it.
fn not_an_invite(what: &str) -> String {
    format!("that link is not a contact or group invite ({what})")
}

/// `DC_CONTACT_ID_SELF`: the account's own contact. Never listed by
/// `get_contacts`, but a member of every group the account is in.
pub(crate) const SELF_CONTACT_ID: u32 = 1;

/// One row from the core's contact object.
/// One contact as a map QML reads properties off, with the same names
/// the model's roles have -- so a row drawn from this and a row drawn
/// from the model are written the same way.
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
        // `color` is pinned by the integration test, which checks it on a
        // message's sender -- the same shape as a contact. The picture key
        // is not pinned, so a rename upstream shows up as a contact
        // falling back to its initial rather than as a failure.
        color: json::text(contact, "color"),
        avatar_path: json::text(contact, "profileImage"),
    }
}
