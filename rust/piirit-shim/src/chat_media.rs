//! The pictures, sounds, files, apps or calls of one chat: what the pages
//! behind the tiles on a contact's or a group's page list.
//!
//! The core keeps the index. `get_chat_media` answers with the ids of
//! every message of up to three kinds in a chat, oldest first, and its
//! own note says not to sort them again -- so the list is turned round,
//! newest first being what a gallery shows, and nothing else is done to
//! its order. Three kinds is the core's limit for one call, and that is
//! what shapes the four pages: the reference clients' four tabs each fit
//! in one.
//!
//! The messages behind the ids are read the way the conversation reads
//! its own, in pages of fifty through `get_messages`, but from the top
//! down without waiting to be asked: every row stands as a placeholder
//! from the moment the ids are in, so the page knows how many there are,
//! and the first screen is filled before the rest have been read. A chat
//! with a thousand pictures costs twenty calls in the background and
//! nothing the reader waits on. What has been read is kept across a
//! reload, so a picture arriving while the page is open is one row
//! fetched and nothing moved.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chat::{act_on_message, fetch_messages};
use crate::core::connection;
use crate::json;
use crate::models::{MessageListItem, MessageListModel};

/// How many messages one `get_messages` asks for: the conversation's own
/// page.
const PAGE: usize = 50;

/// The view types a kind lists, in the shape `get_chat_media` takes: one
/// that has to be named and two that need not.
///
/// A sticker is a picture too and is not here: the gallery's three slots
/// are taken by what the reference clients put in theirs. A shared
/// contact is with the files, where the reference clients have no place
/// for it, since a card somebody sent is otherwise findable nowhere.
fn view_types(kind: &str) -> Option<(&'static str, Option<&'static str>, Option<&'static str>)> {
    match kind {
        "gallery" => Some(("Image", Some("Gif"), Some("Video"))),
        "audio" => Some(("Audio", Some("Voice"), None)),
        "files" => Some(("File", Some("Vcard"), None)),
        "apps" => Some(("Webxdc", None, None)),
        "calls" => Some(("Call", None, None)),
        _ => None,
    }
}

/// One step of a load, in the order they arrive.
enum Step {
    /// The ids are in, newest first: one row per id from here on.
    Ids(Vec<u32>),
    /// One page of messages, to fill their rows in.
    Rows(Vec<MessageListItem>),
    /// Every page is in.
    Done,
    /// Something failed. The message is the core's own.
    Failed(String),
}

/// The messages of one kind in one chat, newest first.
///
/// ```qml
/// ChatMedia { id: media; account_id: 1; chat_id: 7; kind: "gallery" }
/// SilicaGridView { model: media.rows }
/// ```
#[derive(QObject, Default)]
pub struct ChatMedia {
    base: qt_base_class!(trait QObject),

    /// Which account the chat belongs to. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Which chat. Setting it reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY source_changed),
    /// Which kind of message: `gallery` for pictures and videos, `audio`
    /// for voice messages and music, `files` for what was sent as a file
    /// and for shared contacts, `apps` for webxdc apps, `calls` for
    /// calls. Setting it reloads.
    pub kind: qt_property!(QString; WRITE set_kind NOTIFY source_changed),
    /// Emitted when the account, the chat or the kind changes.
    pub source_changed: qt_signal!(),

    /// The rows, newest first, for a view's `model`. A row is a
    /// placeholder -- its id and nothing else -- until its page has been
    /// read; the row's `loaded` says which.
    pub rows: qt_property!(RefCell<MessageListModel>; CONST),
    /// How many rows there are.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// True once the ids are in, however that went: from here `count`
    /// says how many there are, and a page with nothing on it can say
    /// so. The rows may still be filling.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// True from a load being asked for until its last page is in.
    pub loading: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted when `loaded` or `loading` changes.
    pub loaded_changed: qt_signal!(),

    /// Something failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Read the chat's list again, keeping the rows already read.
    pub reload: qt_method!(fn(&mut self)),
    /// Delete a message, here and on the mail server: what the
    /// conversation's row menu does, offered on a media page's row too.
    pub delete_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Delete a message for everyone in the chat, not only here: the
    /// same choice the conversation offers, since a picture on this page
    /// is a message in that chat.
    pub delete_message_for_all: qt_method!(fn(&mut self, message_id: u32)),
    /// Apply one core event. Only what changes this chat is acted on.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Counts loads, so a page of an older load cannot land in the rows
    /// of a newer one; see `ChatInfo`.
    generation: u64,
}

impl ChatMedia {
    /// How many rows there are.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set the account and start over if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.start_over();
        }
    }

    /// Set the chat and start over if it changed.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.source_changed();
            self.start_over();
        }
    }

    /// Set the kind and start over if it changed.
    pub fn set_kind(&mut self, kind: QString) {
        if self.kind.to_string() != kind.to_string() {
            self.kind = kind;
            self.source_changed();
            self.start_over();
        }
    }

    /// Forget every row and read the chat's list afresh: what is held is
    /// another chat's, another kind's, or -- after an overflow -- not to
    /// be trusted.
    fn start_over(&mut self) {
        self.rows.borrow_mut().reset_data(Vec::new());
        self.rows_changed();
        if self.loaded {
            self.loaded = false;
            self.loaded_changed();
        }
        self.reload();
    }

    /// Read the chat's list again, keeping the rows already read: only
    /// the messages not yet here are fetched, so an event that changed
    /// nothing costs one call and moves nothing.
    pub fn reload(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let kind = self.kind.to_string();
        if account_id == 0 || chat_id == 0 || kind.is_empty() {
            return;
        }
        let Some((first, second, third)) = view_types(&kind) else {
            self.error(format!("no such kind of media: {kind}").into());
            return;
        };
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.set_loading(true);
        let known: BTreeSet<u32> = self
            .rows
            .borrow()
            .iter()
            .filter(|row| row.loaded)
            .map(|row| row.message_id)
            .collect();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let step = queued_callback(move |step: Step| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().take(step);
        });

        runtime.spawn(async move {
            let ids = match fetch_ids(&rpc, account_id, chat_id, first, second, third).await {
                Ok(ids) => ids,
                Err(err) => {
                    step(Step::Failed(err));
                    return;
                }
            };
            let wanted: Vec<u32> = ids
                .iter()
                .copied()
                .filter(|id| !known.contains(id))
                .collect();
            step(Step::Ids(ids));
            for page in wanted.chunks(PAGE) {
                match fetch_messages(&rpc, account_id, page).await {
                    Ok(items) => step(Step::Rows(items)),
                    Err(err) => {
                        step(Step::Failed(err));
                        return;
                    }
                }
            }
            step(Step::Done);
        });
    }

    /// Take one step of a load.
    fn take(&mut self, step: Step) {
        match step {
            Step::Ids(ids) => self.place(&ids),
            Step::Rows(items) => {
                if self.fill(items) > 0 {
                    self.rows_changed();
                }
            }
            Step::Done => self.set_loading(false),
            Step::Failed(err) => {
                // Loaded in the sense that matters: the wait is over.
                self.set_loading(false);
                self.set_loaded(true);
                self.error(err.into());
            }
        }
    }

    /// The ids came back: one row per id, in the order they came,
    /// keeping every row already read. Nothing is touched when the list
    /// is as it was, so the view keeps its place.
    fn place(&mut self, ids: &[u32]) {
        let same = {
            let rows = self.rows.borrow();
            rows.iter().count() == ids.len()
                && rows.iter().zip(ids).all(|(row, id)| row.message_id == *id)
        };
        if !same {
            let kept: BTreeMap<u32, MessageListItem> = self
                .rows
                .borrow()
                .iter()
                .filter(|row| row.loaded)
                .map(|row| (row.message_id, row.clone()))
                .collect();
            let rows: Vec<MessageListItem> = ids
                .iter()
                .map(|id| kept.get(id).cloned().unwrap_or_else(|| placeholder(*id)))
                .collect();
            self.rows.borrow_mut().reset_data(rows);
            self.rows_changed();
        }
        self.set_loaded(true);
    }

    /// Fill fetched rows in place, by id, and say how many had a row to
    /// land in. By id rather than by index, as the conversation does: the
    /// list can have changed while the page was being read.
    fn fill(&mut self, items: Vec<MessageListItem>) -> usize {
        let mut rows = self.rows.borrow_mut();
        let index_of: BTreeMap<u32, usize> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.message_id, index))
            .collect();
        let mut landed = 0;
        for item in items {
            if let Some(index) = index_of.get(&item.message_id).copied() {
                rows.change_line(index, item);
                landed += 1;
            }
        }
        landed
    }

    /// Re-read one message and replace its row: the rest of a message
    /// the download limit held back landing, or an edit from elsewhere.
    fn refresh_one(&mut self, message_id: u32) {
        let account_id = self.account_id;
        if !self
            .rows
            .borrow()
            .iter()
            .any(|row| row.message_id == message_id)
        {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let generation = self.generation;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            if let Ok(items) = result {
                if this.borrow_mut().fill(items) > 0 {
                    this.borrow().rows_changed();
                }
            }
        });
        runtime.spawn(async move {
            done(fetch_messages(&rpc, account_id, &[message_id]).await);
        });
    }

    /// Delete a message, here and on the mail server.
    pub fn delete_message(&mut self, message_id: u32) {
        self.remove("delete_messages", message_id);
    }

    /// Delete a message for everyone in the chat, not only here: the
    /// core's `delete_messages_for_all`. Offered for the same messages
    /// the conversation offers it for, and refused by the core for the
    /// rest -- see `ChatMessages::delete_message_for_all`.
    pub fn delete_message_for_all(&mut self, message_id: u32) {
        self.remove("delete_messages_for_all", message_id);
    }

    /// Ask the core to delete this message, one way or the other, and
    /// read the list again once it has.
    ///
    /// The core announces the deletion and the list is read again on
    /// that; the read here is so the row goes the moment the core has
    /// agreed rather than a turn later, and a refusal is reported like
    /// any other failure. Both reads land on the same list, and the
    /// second moves nothing. No core is nothing to delete from: every
    /// row here came out of one.
    fn remove(&mut self, method: &'static str, message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if let Err(err) = result {
                this.borrow().error(err.into());
                return;
            }
            this.borrow_mut().reload();
        });
        runtime.spawn(async move {
            done(act_on_message(&rpc, method, account_id, message_id).await);
        });
    }

    /// Apply one core event.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if context_id != self.account_id || self.chat_id == 0 {
            return;
        }
        let kind = kind.to_string();
        let Some(payload) = json::chat_event(&payload_json.to_string(), self.chat_id) else {
            return;
        };
        match kind.as_str() {
            // Something arrived or went: the list is read again, and
            // only what is new in it is fetched.
            "IncomingMsg" | "MsgDeleted" => self.reload(),
            // The same, and the one message the event names read again:
            // a download the limit held back lands as this event with
            // nothing else different, and the row it fills is one
            // already here.
            "MsgsChanged" => {
                self.reload();
                if let Some(message_id) = json::u32_opt(&payload, "msgId").filter(|id| *id != 0) {
                    self.refresh_one(message_id);
                }
            }
            // The core dropped events it could not queue, so what is
            // held may already be wrong in ways no later event will
            // mention. Start again rather than patch.
            "EventChannelOverflow" => self.start_over(),
            _ => {}
        }
    }

    /// Say whether a load is under way, when that changes.
    fn set_loading(&mut self, loading: bool) {
        if self.loading != loading {
            self.loading = loading;
            self.loaded_changed();
        }
    }

    /// Say whether the ids are in, when that changes.
    fn set_loaded(&mut self, loaded: bool) {
        if self.loaded != loaded {
            self.loaded = loaded;
            self.loaded_changed();
        }
    }
}

/// A row that knows its id and nothing else yet.
fn placeholder(message_id: u32) -> MessageListItem {
    MessageListItem {
        message_id,
        loaded: false,
        ..MessageListItem::default()
    }
}

/// The ids of every message of these kinds in the chat, newest first.
///
/// The core answers oldest first and asks not to be re-sorted; turned
/// round is not sorted, and newest first is what a gallery shows.
async fn fetch_ids(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
    first: &str,
    second: Option<&str>,
    third: Option<&str>,
) -> Result<Vec<u32>, String> {
    let mut ids: Vec<u32> = rpc
        .call(
            "get_chat_media",
            (account_id, Some(chat_id), first, second, third),
        )
        .await
        .map_err(|err| err.to_string())?;
    ids.reverse();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::view_types;

    #[test]
    fn every_kind_names_what_the_core_can_take_in_one_call() {
        assert_eq!(
            view_types("gallery"),
            Some(("Image", Some("Gif"), Some("Video")))
        );
        assert_eq!(view_types("audio"), Some(("Audio", Some("Voice"), None)));
        assert_eq!(view_types("files"), Some(("File", Some("Vcard"), None)));
        assert_eq!(view_types("apps"), Some(("Webxdc", None, None)));
        assert_eq!(view_types("calls"), Some(("Call", None, None)));
        assert_eq!(view_types("stickers"), None);
        assert_eq!(view_types(""), None);
    }
}
