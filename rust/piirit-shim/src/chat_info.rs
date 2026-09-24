//! What a chat is: its name, its picture and who is in it.
//!
//! One model for both kinds of chat. A group's name, picture and members
//! can be changed here. A one-to-one chat has one member, the contact, and
//! the same model reads them, so the contact page needs nothing of its own.

use std::cell::RefCell;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::contacts::contact_row;
use crate::core::connection;
use crate::json;
use crate::models::{ContactItem, ContactListModel};

/// What one load found.
struct Loaded {
    name: String,
    avatar_path: String,
    color: String,
    is_group: bool,
    can_edit: bool,
    can_send: bool,
    ephemeral_timer: u32,
    members: Vec<ContactItem>,
}

/// What to say once a change has reached the core.
enum Outcome {
    Saved,
    Renamed(String),
    Left,
}

/// A chat's name, picture and members, and the calls that change them.
///
/// ```qml
/// ChatInfo { id: chat; account_id: page.accountId; chat_id: page.chatId }
/// Repeater { model: chat.members }
/// ```
#[derive(QObject, Default)]
// `is_group`, `can_edit` and `loaded` are three facts QML binds to on its
// own; see `ChatMessages`.
#[allow(clippy::struct_excessive_bools)]
pub struct ChatInfo {
    base: qt_base_class!(trait QObject),

    /// Which account the chat belongs to. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY chat_changed),
    /// Which chat. Setting it reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY chat_changed),
    /// Emitted when the account or the chat changes.
    pub chat_changed: qt_signal!(),

    /// The chat's name, as the core holds it.
    pub name: qt_property!(QString; NOTIFY loaded_changed),
    /// Path to the chat's picture, empty when it has none.
    pub avatar_path: qt_property!(QString; NOTIFY loaded_changed),
    /// The core's per-chat colour, `#rrggbb`, for the avatar.
    pub color: qt_property!(QString; NOTIFY loaded_changed),
    /// A group rather than a one-to-one chat.
    pub is_group: qt_property!(bool; NOTIFY loaded_changed),
    /// A group this account is still in. The core refuses every change
    /// to one it has left, or to a chat that is not a group at all, so
    /// the page offers none.
    pub can_edit: qt_property!(bool; NOTIFY loaded_changed),
    /// A chat this account can write to. False for a group it has left
    /// and for a contact request, and what settings that apply to the
    /// whole chat -- disappearing messages -- are offered on.
    pub can_send: qt_property!(bool; NOTIFY loaded_changed),
    /// Seconds after which messages in this chat disappear, 0 for never.
    /// The core's `ephemeralTimer`.
    pub ephemeral_timer: qt_property!(u32; NOTIFY loaded_changed),
    /// True once a load has finished, however it went. Emitted again on
    /// every reload, since every field above changes with it.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted after every load.
    pub loaded_changed: qt_signal!(),

    /// The members, in the core's order, for a `Repeater`'s `model`. A
    /// one-to-one chat has one: the contact.
    pub members: qt_property!(RefCell<ContactListModel>; CONST),
    /// How many members there are.
    pub member_count: qt_property!(u32; READ member_count NOTIFY loaded_changed),

    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),
    /// A change reached the core. The fields reload behind it.
    pub saved: qt_signal!(),
    /// The name reached the core, and this is it.
    pub renamed: qt_signal!(name: QString),
    /// This account is no longer in the group.
    pub left: qt_signal!(),

    /// Reload everything.
    pub reload: qt_method!(fn(&mut self)),
    /// Apply one core event. Only what changes this chat is acted on.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),
    /// Whether a contact is among the members as last loaded.
    pub is_member: qt_method!(fn(&self, contact_id: u32) -> bool),

    /// Give the group a new name. Answers on `renamed`.
    pub rename: qt_method!(fn(&mut self, name: QString)),
    /// Use the image at `path` as the picture. Answers on `saved`.
    pub set_picture: qt_method!(fn(&mut self, path: QString)),
    /// Remove the picture. Answers on `saved`.
    pub clear_picture: qt_method!(fn(&mut self)),
    /// Add contacts to the group. Answers on `saved`; anyone the core
    /// refused is named on `error`, and the rest are in.
    pub add_members: qt_method!(fn(&mut self, contact_ids: QVariantList)),
    /// Remove one member. Answers on `saved`.
    pub remove_member: qt_method!(fn(&mut self, contact_id: u32)),
    /// Leave the group. Answers on `left`.
    pub leave: qt_method!(fn(&mut self)),
    /// Give a contact a name of the reader's own, or none: an empty
    /// name puts the contact back to what they call themselves. Answers
    /// on `saved`.
    pub rename_contact: qt_method!(fn(&mut self, contact_id: u32, name: QString)),
    /// Make messages in this chat disappear after `seconds`, or never for
    /// 0. Applies to every member, since the core tells them. Answers on
    /// `saved`.
    pub set_ephemeral_timer: qt_method!(fn(&mut self, seconds: u32)),

    /// Counts loads, so a slow answer to an older question cannot land on
    /// top of a newer one; see `ContactList`.
    generation: u64,
}

impl ChatInfo {
    /// How many members there are.
    pub fn member_count(&self) -> u32 {
        u32::try_from(self.members.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.chat_changed();
            self.reload();
        }
    }

    /// Set the chat and reload if it changed.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.loaded = false;
            self.loaded_changed();
            self.chat_changed();
            self.reload();
        }
    }

    /// Whether a contact is among the members as last loaded.
    pub fn is_member(&self, contact_id: u32) -> bool {
        self.members
            .borrow()
            .iter()
            .any(|row| row.contact_id == contact_id)
    }

    /// Reload everything.
    pub fn reload(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Loaded, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(found) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.name = found.name.into();
                        this_mut.avatar_path = found.avatar_path.into();
                        this_mut.color = found.color.into();
                        this_mut.is_group = found.is_group;
                        this_mut.can_edit = found.can_edit;
                        this_mut.can_send = found.can_send;
                        this_mut.ephemeral_timer = found.ephemeral_timer;
                        this_mut.members.borrow_mut().reset_data(found.members);
                        this_mut.loaded = true;
                    }
                    this.borrow().loaded_changed();
                }
                Err(err) => {
                    // Loaded in the sense that matters: the wait is over.
                    this.borrow_mut().loaded = true;
                    this.borrow().loaded_changed();
                    this.borrow().error(err.into());
                }
            }
        });

        runtime.spawn(async move {
            done(fetch(&rpc, account_id, chat_id).await);
        });
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        let kind = kind.to_string();
        match kind.as_str() {
            // A member's name or picture is the contact's, and the core
            // does not say whose changed.
            "ContactsChanged" | "EventChannelOverflow" => self.reload(),
            // Renaming, a new picture, and anyone joining or leaving --
            // from this device or another -- and the timer, which any
            // member can set.
            "ChatModified" | "ChatEphemeralTimerModified" => {
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
                if json::u32_opt(&payload, "chatId") == Some(self.chat_id) {
                    self.reload();
                }
            }
            _ => {}
        }
    }

    /// Give the group a new name.
    pub fn rename(&mut self, name: QString) {
        let name = name.to_string().trim().to_string();
        // The core refuses an empty name with "Invalid name", which says
        // nothing about what to do instead.
        if name.is_empty() {
            self.error(QString::from("A group needs a name"));
            return;
        }
        // Retyping the name it already has is not a change.
        if name == self.name.to_string() {
            return;
        }
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Renamed(name.clone()));

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("set_chat_name", (account_id, chat_id, name))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Use the image at `path` as the picture.
    pub fn set_picture(&mut self, path: QString) {
        // A picker hands back a path, or a URL; the core wants a path.
        let path = crate::chat::local_path(&path.to_string());
        if path.is_empty() {
            return;
        }
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            // The core copies the file into its own blob directory and
            // holds that path afterwards, which is why a save reloads.
            let result = rpc
                .call::<_, ()>("set_chat_profile_image", (account_id, chat_id, Some(path)))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Remove the picture.
    pub fn clear_picture(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            // Null clears, as with the profile picture.
            let result = rpc
                .call::<_, ()>(
                    "set_chat_profile_image",
                    (account_id, chat_id, Option::<String>::None),
                )
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Add contacts to the group.
    pub fn add_members(&mut self, contact_ids: QVariantList) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |refused: Vec<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Reloaded either way: whoever the core took is in.
            this.borrow_mut().reload();
            if refused.is_empty() {
                this.borrow().saved();
            } else {
                this.borrow().error(
                    format!("some people could not be added ({})", refused.join("; ")).into(),
                );
            }
        });

        let members: Vec<u32> = contact_ids
            .into_iter()
            .filter_map(|value| i32::from_qvariant(value.clone()))
            .filter_map(|value| u32::try_from(value).ok())
            .collect();
        runtime.spawn(async move {
            // Every member attempted, as create_group does: one the core
            // refuses -- an address contact, in an encrypted group -- must
            // not stop the ones after it.
            let mut refused = Vec::new();
            for member in members {
                if let Err(err) = rpc
                    .call::<_, ()>("add_contact_to_chat", (account_id, chat_id, member))
                    .await
                {
                    refused.push(format!("{member}: {err}"));
                }
            }
            done(refused);
        });
    }

    /// Remove one member.
    pub fn remove_member(&mut self, contact_id: u32) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(
                    "remove_contact_from_chat",
                    (account_id, chat_id, contact_id),
                )
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Make messages in this chat disappear after `seconds`.
    pub fn set_ephemeral_timer(&mut self, seconds: u32) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("set_chat_ephemeral_timer", (account_id, chat_id, seconds))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Give a contact a name of the reader's own, or none.
    pub fn rename_contact(&mut self, contact_id: u32, name: QString) {
        let name = name.to_string().trim().to_string();
        let account_id = self.account_id;
        if account_id == 0 || contact_id == 0 {
            return;
        }
        // The name already held, whichever way: nothing to write.
        let unchanged = self
            .members
            .borrow()
            .iter()
            .any(|row| row.contact_id == contact_id && row.name.to_string() == name);
        if unchanged {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            // An empty name is the core's own way of saying "theirs
            // again": pinned against the real core in
            // deltachat-jsonrpc/tests/real_server.rs.
            let result = rpc
                .call::<_, ()>("change_contact_name", (account_id, contact_id, name))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Leave the group.
    pub fn leave(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Left);

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("leave_group", (account_id, chat_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Report a change, and read back what the core actually kept: it
    /// rewrites a picture into its own blob directory, and it is the one
    /// that knows who is in the group now.
    fn stored_callback(&self, outcome: Outcome) -> impl Fn(Result<(), String>) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => {
                    this.borrow_mut().reload();
                    match &outcome {
                        Outcome::Saved => this.borrow().saved(),
                        Outcome::Renamed(name) => this.borrow().renamed(name.as_str().into()),
                        Outcome::Left => this.borrow().left(),
                    }
                }
                Err(err) => this.borrow().error(err.into()),
            }
        })
    }
}

/// The chat and its members, in two calls: `get_full_chat_by_id` names the
/// members by id only, and `get_contacts_by_ids` answers with the contacts
/// keyed by id -- as strings, JSON having no other kind of key -- so the
/// order is put back from the id list.
async fn fetch(rpc: &RpcClient, account_id: u32, chat_id: u32) -> Result<Loaded, String> {
    let chat: serde_json::Value = rpc
        .call("get_full_chat_by_id", (account_id, chat_id))
        .await
        .map_err(|err| err.to_string())?;
    let ids: Vec<u32> = chat
        .get("contactIds")
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(serde_json::Value::as_u64)
                .filter_map(|id| u32::try_from(id).ok())
                .collect()
        })
        .unwrap_or_default();
    let contacts: serde_json::Value = if ids.is_empty() {
        serde_json::Value::Null
    } else {
        rpc.call("get_contacts_by_ids", (account_id, ids.clone()))
            .await
            .map_err(|err| err.to_string())?
    };
    let members = ids
        .iter()
        .filter_map(|id| contacts.get(id.to_string()))
        .map(contact_row)
        .collect();
    // "Single" is the one-to-one chat; everything else -- Group, and the
    // broadcast and mailing-list kinds -- has members worth listing.
    let is_group = !matches!(json::str_at(&chat, "chatType"), "Single" | "");
    Ok(Loaded {
        name: json::str_at(&chat, "name").to_string(),
        avatar_path: json::str_at(&chat, "profileImage").to_string(),
        color: json::str_at(&chat, "color").to_string(),
        is_group,
        // Pinned against the real core: after leaving, `selfInGroup` and
        // `canSend` both go false and every edit is refused.
        can_edit: is_group && json::flag(&chat, "selfInGroup"),
        can_send: json::flag(&chat, "canSend"),
        ephemeral_timer: json::u32_at(&chat, "ephemeralTimer"),
        members,
    })
}
