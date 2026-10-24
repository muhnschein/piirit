//! Chat name, picture, members.
//!
//! One model for both kinds: group's name, picture, members editable; 1:1 chat has one member
//! (the contact), so contact page needs nothing of its own.

use std::cell::RefCell;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chatlist::mute_duration;
use crate::contacts::contact_row;
use crate::core::{connection, reconcile_rows};
use crate::json;
use crate::models::{ContactItem, ContactListModel};

/// Load result.
// Bool facts, each read into same-named property; see `ChatInfo`.
#[allow(clippy::struct_excessive_bools)]
struct Loaded {
    name: String,
    avatar_path: String,
    color: String,
    is_group: bool,
    can_edit: bool,
    can_send: bool,
    can_call: bool,
    ephemeral_timer: u32,
    is_muted: bool,
    description: String,
    can_edit_description: bool,
    members: Vec<ContactItem>,
}

/// Post-change reply.
enum Outcome {
    Saved,
    Renamed(String),
    Left,
}

/// Chat name, picture, members, calls.
///
/// ```qml
/// ChatInfo { id: chat; account_id: page.accountId; chat_id: page.chatId }
/// Repeater { model: chat.members }
/// ```
#[derive(QObject, Default)]
// `is_group`, `can_edit`, `loaded`: facts QML binds each on its own; see `ChatMessages`.
#[allow(clippy::struct_excessive_bools)]
pub struct ChatInfo {
    base: qt_base_class!(trait QObject),

    /// Account id. Set reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY chat_changed),
    /// Chat id. Set reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY chat_changed),
    /// Account/chat changed.
    pub chat_changed: qt_signal!(),

    /// Name, per core.
    pub name: qt_property!(QString; NOTIFY loaded_changed),
    /// Picture path, or empty.
    pub avatar_path: qt_property!(QString; NOTIFY loaded_changed),
    /// Per-chat colour, `#rrggbb`.
    pub color: qt_property!(QString; NOTIFY loaded_changed),
    /// Group, not 1:1.
    pub is_group: qt_property!(bool; NOTIFY loaded_changed),
    /// Group account is still in. Core refuses edits to left group or non-group: page offers none.
    pub can_edit: qt_property!(bool; NOTIFY loaded_changed),
    /// Account can write: false for left group, contact request. Gates whole-chat settings
    /// (disappearing messages).
    pub can_send: qt_property!(bool; NOTIFY loaded_changed),
    /// Call placeable: 1:1, encrypted, takes messages. Same as `ChatMessages::can_call`.
    pub can_call: qt_property!(bool; NOTIFY loaded_changed),
    /// Group/channel description, line breaks kept; empty if none. From `get_chat_description`
    /// (`get_full_chat_by_id` lacks it).
    pub description: qt_property!(QString; NOTIFY loaded_changed),
    /// Description editable: group/channel still in. Narrower than `can_edit`: core refuses other
    /// kinds (mailing list).
    pub can_edit_description: qt_property!(bool; NOTIFY loaded_changed),
    /// Seconds until messages disappear; 0 = never. Core's `ephemeralTimer`.
    pub ephemeral_timer: qt_property!(u32; NOTIFY loaded_changed),
    /// Muted now. Core's `isMuted`; no end time given, none shown.
    pub is_muted: qt_property!(bool; NOTIFY loaded_changed),
    /// True after any load, good or bad. Re-emitted per reload (all fields change with it).
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted per load.
    pub loaded_changed: qt_signal!(),

    /// Members in core's order, for `Repeater` model. 1:1: the contact.
    pub members: qt_property!(RefCell<ContactListModel>; CONST),
    /// Member count.
    pub member_count: qt_property!(u32; READ member_count NOTIFY loaded_changed),

    /// Error; core's message.
    pub error: qt_signal!(message: QString),
    /// Change saved; fields reload.
    pub saved: qt_signal!(),
    /// New name reached core.
    pub renamed: qt_signal!(name: QString),
    /// Account left group.
    pub left: qt_signal!(),

    /// Reload all.
    pub reload: qt_method!(fn(&mut self)),
    /// Apply event for this chat.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),
    /// Is member, as loaded.
    pub is_member: qt_method!(fn(&self, contact_id: u32) -> bool),

    /// Rename. Answers `renamed`.
    pub rename: qt_method!(fn(&mut self, name: QString)),
    /// Set description; empty clears. Answers `saved`.
    pub set_description: qt_method!(fn(&mut self, text: QString)),
    /// Picture from `path`. Answers `saved`.
    pub set_picture: qt_method!(fn(&mut self, path: QString)),
    /// Clear picture. Answers `saved`.
    pub clear_picture: qt_method!(fn(&mut self)),
    /// Add contacts; `saved`. Refused ones named on `error`, rest in.
    pub add_members: qt_method!(fn(&mut self, contact_ids: QVariantList)),
    /// Remove member; `saved`.
    pub remove_member: qt_method!(fn(&mut self, contact_id: u32)),
    /// Leave group; `left`.
    pub leave: qt_method!(fn(&mut self)),
    /// Local name; empty restores theirs; `saved`.
    pub rename_contact: qt_method!(fn(&mut self, contact_id: u32, name: QString)),
    /// Messages disappear after `seconds`, 0 = never; core tells every member. `saved`.
    pub set_ephemeral_timer: qt_method!(fn(&mut self, seconds: u32)),
    /// Mute `seconds`; -1 forever, 0 unmute. `saved`.
    pub set_mute_duration: qt_method!(fn(&mut self, seconds: i32)),

    /// Stale-load guard. See `ContactList`.
    generation: u64,
}

impl ChatInfo {
    /// Member count.
    pub fn member_count(&self) -> u32 {
        u32::try_from(self.members.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set account; reload if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.chat_changed();
            self.reload();
        }
    }

    /// Set chat; reload if changed.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.loaded = false;
            self.loaded_changed();
            self.chat_changed();
            self.reload();
        }
    }

    /// Is member, as loaded.
    pub fn is_member(&self, contact_id: u32) -> bool {
        self.members
            .borrow()
            .iter()
            .any(|row| row.contact_id == contact_id)
    }

    /// Reload all.
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
                        this_mut.can_call = found.can_call;
                        this_mut.ephemeral_timer = found.ephemeral_timer;
                        this_mut.is_muted = found.is_muted;
                        this_mut.description = found.description.into();
                        this_mut.can_edit_description = found.can_edit_description;
                        // In place: runs on every contact change in account, mostly no member
                        // change; reset rebuilt every row.
                        reconcile_rows(&mut this_mut.members.borrow_mut(), found.members, |item| {
                            item.contact_id
                        });
                        this_mut.loaded = true;
                    }
                    this.borrow().loaded_changed();
                }
                Err(err) => {
                    // Loaded where it matters: wait over.
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

    /// Apply core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        let kind = kind.to_string();
        match kind.as_str() {
            // Contact's name/picture; core omits whose.
            "ContactsChanged" | "EventChannelOverflow" => self.reload(),
            // Rename, picture, join/leave (any device), timer (any member), mute.
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

    /// Rename group.
    pub fn rename(&mut self, name: QString) {
        let name = name.to_string().trim().to_string();
        // Core's "Invalid name" gives no hint: check here.
        if name.is_empty() {
            self.error(QString::from("A group needs a name"));
            return;
        }
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

    /// Set description, or clear.
    pub fn set_description(&mut self, text: QString) {
        // Core trims too; trim so stray newline is no change.
        let text = text.to_string().trim().to_string();
        if text == self.description.to_string() {
            return;
        }
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            // Empty clears. Promoted group/channel: members told, not local note.
            let result = rpc
                .call::<_, ()>("set_chat_description", (account_id, chat_id, text))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Set picture from `path`.
    pub fn set_picture(&mut self, path: QString) {
        // Picker: path or URL; core wants path.
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
            // Core copies to blob dir: reload.
            let result = rpc
                .call::<_, ()>("set_chat_profile_image", (account_id, chat_id, Some(path)))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Clear picture.
    pub fn clear_picture(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            // Null clears, like profile.
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

    /// Add contacts.
    pub fn add_members(&mut self, contact_ids: QVariantList) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |refused: Vec<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Reload either way: taken members in.
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
            // Try all, as create_group: one refusal (address contact in encrypted group) mustn't
            // stop rest.
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

    /// Remove member.
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

    /// Messages disappear after `seconds`.
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

    /// Mute `seconds`; -1 forever, 0 unmute.
    pub fn set_mute_duration(&mut self, seconds: i32) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback(Outcome::Saved);

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(
                    "set_chat_mute_duration",
                    (account_id, chat_id, mute_duration(seconds)),
                )
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Local name, or clear.
    pub fn rename_contact(&mut self, contact_id: u32, name: QString) {
        let name = name.to_string().trim().to_string();
        let account_id = self.account_id;
        if account_id == 0 || contact_id == 0 {
            return;
        }
        // Same name: nothing to write.
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
            // Empty = theirs again. Pinned: deltachat-jsonrpc/tests/real_server.rs.
            let result = rpc
                .call::<_, ()>("change_contact_name", (account_id, contact_id, name))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Leave group.
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

    /// Report change, reread what core kept (picture rewritten into blob dir, current members).
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

/// Chat and members.
///
/// `get_full_chat_by_id` names members by id; `get_contacts_by_ids` keys by id string (JSON),
/// so order restored from id list.
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
    // "Single" = 1:1. Others have members.
    let is_group = !matches!(json::str_at(&chat, "chatType"), "Single" | "");
    let can_send = json::flag(&chat, "canSend");
    // Only these kinds have one; core refuses others. Failed read = empty, not lost page.
    let chat_type = json::str_at(&chat, "chatType");
    let has_description = matches!(chat_type, "Group" | "OutBroadcast" | "InBroadcast");
    let description: String = if has_description {
        rpc.call("get_chat_description", (account_id, chat_id))
            .await
            .unwrap_or_default()
    } else {
        String::new()
    };
    let self_in_group = json::flag(&chat, "selfInGroup");
    Ok(Loaded {
        name: json::str_at(&chat, "name").to_string(),
        avatar_path: json::str_at(&chat, "profileImage").to_string(),
        color: json::str_at(&chat, "color").to_string(),
        is_group,
        // Pinned vs core: after leave, `selfInGroup` and `canSend` false, edits refused.
        can_edit: is_group && self_in_group,
        can_send,
        can_call: crate::chat::takes_calls(&chat, is_group, can_send),
        ephemeral_timer: json::u32_at(&chat, "ephemeralTimer"),
        is_muted: json::flag(&chat, "isMuted"),
        description,
        can_edit_description: matches!(chat_type, "Group" | "OutBroadcast") && self_in_group,
        members,
    })
}
