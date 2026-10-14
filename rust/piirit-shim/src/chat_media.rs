//! Pictures, sounds, files, apps, calls of a chat: pages behind contact/group page tiles.
//!
//! Core keeps the index: `get_chat_media` gives ids of up to three kinds, oldest first, and
//! its note says not to re-sort. Only reversed (newest first, as gallery). Three kinds per
//! call is core's limit; reference clients' four tabs each fit one, hence four pages.
//!
//! Messages read like conversation's (`get_messages`, pages of fifty), but top down without
//! waiting to be asked: placeholder rows from ids on (count known), first screen filled
//! before rest. Thousand pictures = twenty background calls, no wait. Read rows kept across
//! reload: new picture = one row fetched, nothing moved.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chat::{act_on_message, fetch_messages};
use crate::core::connection;
use crate::json;
use crate::models::{MessageListItem, MessageListModel};

/// Messages per `get_messages`, as conversation.
const PAGE: usize = 50;

/// View types per kind, as `get_chat_media` takes: one required, two optional.
///
/// No stickers: gallery's three slots hold what reference clients put there. Shared
/// contacts in files (reference clients have no place; else findable nowhere).
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

/// One load step, in arrival order.
enum Step {
    /// Ids in, newest first: one row each.
    Ids(Vec<u32>),
    /// Page of messages to fill rows.
    Rows(Vec<MessageListItem>),
    /// All pages in.
    Done,
    /// Failure; core's message.
    Failed(String),
}

/// One kind's messages in a chat, newest first.
///
/// ```qml
/// ChatMedia { id: media; account_id: 1; chat_id: 7; kind: "gallery" }
/// SilicaGridView { model: media.rows }
/// ```
#[derive(QObject, Default)]
pub struct ChatMedia {
    base: qt_base_class!(trait QObject),

    /// Account id. Set reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Chat id. Set reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY source_changed),
    /// `gallery` pics/video, `audio` voice/music, `files` incl. contacts,
    /// `apps` webxdc, `calls`. Set reloads.
    pub kind: qt_property!(QString; WRITE set_kind NOTIFY source_changed),
    /// Account, chat or kind changed.
    pub source_changed: qt_signal!(),

    /// Rows, newest first, for `model`. Placeholder (id only) until page read; row's
    /// `loaded` says which.
    pub rows: qt_property!(RefCell<MessageListModel>; CONST),
    /// Row count.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted on any `rows` change.
    pub rows_changed: qt_signal!(),

    /// True once ids in, success or not: `count` now valid, empty page can say so. Rows
    /// may still fill.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// True from load start until last page.
    pub loading: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted on `loaded`/`loading` change.
    pub loaded_changed: qt_signal!(),

    /// Failure; core's message.
    pub error: qt_signal!(message: QString),

    /// Reload list, keeping read rows.
    pub reload: qt_method!(fn(&mut self)),
    /// Delete here and on server, as conversation menu does.
    pub delete_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Delete for all in chat, as conversation offers.
    pub delete_message_for_all: qt_method!(fn(&mut self, message_id: u32)),
    /// Apply core event; this chat only.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Load counter: older load's page can't land in newer's rows; see `ChatInfo`.
    ///
    /// Shared with load, read before each page: stops once newer starts. Every chat message
    /// starts one; run to end, old load reread pages newer was reading anyway.
    generation: Arc<AtomicU64>,
}

impl ChatMedia {
    /// Row count.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Set account; start over if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.start_over();
        }
    }

    /// Set chat; start over if changed.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.source_changed();
            self.start_over();
        }
    }

    /// Set kind; start over if changed.
    pub fn set_kind(&mut self, kind: QString) {
        if self.kind.to_string() != kind.to_string() {
            self.kind = kind;
            self.source_changed();
            self.start_over();
        }
    }

    /// Drop rows, reread: held rows are other chat/kind or untrusted.
    fn start_over(&mut self) {
        self.rows.borrow_mut().reset_data(Vec::new());
        self.rows_changed();
        if self.loaded {
            self.loaded = false;
            self.loaded_changed();
        }
        self.reload();
    }

    /// Reread, keep read rows. Fetch only missing: no-op event costs one call.
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
        let generation = self
            .generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let current = Arc::clone(&self.generation);
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
            if this.borrow().generation.load(Ordering::Relaxed) != generation {
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
                // Newer load supersedes this one; page now would be dropped.
                if current.load(Ordering::Relaxed) != generation {
                    return;
                }
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

    /// Apply one load step.
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
                // Loaded where it matters: wait over.
                self.set_loading(false);
                self.set_loaded(true);
                self.error(err.into());
            }
        }
    }

    /// Ids back: one row per id, in order, keeping read rows.
    /// Unchanged list touches nothing.
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

    /// Fill rows by id, not index, as conversation (list may change mid-read). Returns rows filled.
    fn fill(&mut self, items: Vec<MessageListItem>) -> usize {
        let mut rows = self.rows.borrow_mut();
        let index_of: BTreeMap<u32, usize> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.message_id, index))
            .collect();
        let mut landed: usize = 0;
        for item in items {
            if let Some(index) = index_of.get(&item.message_id).copied() {
                rows.change_line(index, item);
                landed = landed.saturating_add(1);
            }
        }
        landed
    }

    /// Reread one message, replace its row: rest of download-limited message landing, or
    /// edit from elsewhere.
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
        let generation = self.generation.load(Ordering::Relaxed);
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Vec<MessageListItem>, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation.load(Ordering::Relaxed) != generation {
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

    /// Delete here and on server.
    pub fn delete_message(&mut self, message_id: u32) {
        self.remove("delete_messages", message_id);
    }

    /// Delete for all (`delete_messages_for_all`). Core refuses some;
    /// see `ChatMessages::delete_message_for_all`.
    pub fn delete_message_for_all(&mut self, message_id: u32) {
        self.remove("delete_messages_for_all", message_id);
    }

    /// Ask core to delete, then reread list.
    ///
    /// Core announces deletion and list rereads on that; reread here so row goes on
    /// agreement, not a turn later, and refusal reported as any failure. Second read moves
    /// nothing. No core: nothing to delete from (every row came from one).
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
            // Arrived or gone: reread; fetch only new.
            "IncomingMsg" | "MsgDeleted" => self.reload(),
            // Reread, and the named message: download held back by limit lands as this
            // event, nothing else different, filling row already here.
            "MsgsChanged" => {
                self.reload();
                if let Some(message_id) = json::u32_opt(&payload, "msgId").filter(|id| *id != 0) {
                    self.refresh_one(message_id);
                }
            }
            // Core dropped events: held rows may be wrong in ways no later event says. Restart, don't patch.
            "EventChannelOverflow" => self.start_over(),
            _ => {}
        }
    }

    /// Set loading flag; emits on change.
    fn set_loading(&mut self, loading: bool) {
        if self.loading != loading {
            self.loading = loading;
            self.loaded_changed();
        }
    }

    /// Set loaded flag; emits on change.
    fn set_loaded(&mut self, loaded: bool) {
        if self.loaded != loaded {
            self.loaded = loaded;
            self.loaded_changed();
        }
    }
}

/// Row with id only.
fn placeholder(message_id: u32) -> MessageListItem {
    MessageListItem {
        message_id,
        loaded: false,
        ..MessageListItem::default()
    }
}

/// Ids of these kinds in chat, newest first.
///
/// Core answers oldest first, asks not to re-sort; reversing isn't sorting.
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
