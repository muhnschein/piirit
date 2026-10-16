//! Pictures, sounds, files, apps, calls of a chat: pages behind contact/group page tiles.
//!
//! Core's `get_chat_media`: ids of up to three kinds (core's limit; reference clients' four
//! tabs each fit one call, hence four pages), oldest first, not to re-sort; only reversed.
//!
//! Messages read as conversation (`get_messages`, pages of fifty) but top down unasked:
//! placeholder rows from ids, first screen first. Read rows kept across reload: new picture =
//! one row fetched, one row inserted; views keep their place (swiped viewer stays on its
//! picture).

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
/// No stickers (gallery slots as reference clients). Contacts in files: else findable nowhere.
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

    /// Rows, newest first, for `model`; id-only until read (row's `loaded`).
    pub rows: qt_property!(RefCell<MessageListModel>; CONST),
    /// Row count.
    pub count: qt_property!(u32; READ count NOTIFY rows_changed),
    /// Emitted on any `rows` change.
    pub rows_changed: qt_signal!(),

    /// Ids in (or failed): `count` valid; rows may still fill.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// True from load start until last page.
    pub loading: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted on `loaded`/`loading` change.
    pub loaded_changed: qt_signal!(),

    /// Failure; core's message.
    pub error: qt_signal!(message: QString),

    /// Reload list, keeping read rows.
    pub reload: qt_method!(fn(&mut self)),
    /// Delete here and on server (as conversation menu).
    pub delete_message: qt_method!(fn(&mut self, message_id: u32)),
    /// Delete for all in chat.
    pub delete_message_for_all: qt_method!(fn(&mut self, message_id: u32)),
    /// Row of message, -1 if not here (yet).
    pub index_of: qt_method!(fn(&self, message_id: u32) -> i32),
    /// Apply core event; this chat only.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Load counter, as `ChatInfo`: older page can't land in newer rows. Checked per page, so
    /// old load stops (every chat message starts one).
    generation: Arc<AtomicU64>,
}

impl ChatMedia {
    /// Row count.
    pub fn count(&self) -> u32 {
        u32::try_from(self.rows.borrow().iter().count()).unwrap_or(u32::MAX)
    }

    /// Row of message, -1 if not here (yet).
    pub fn index_of(&self, message_id: u32) -> i32 {
        self.rows
            .borrow()
            .iter()
            .position(|row| row.message_id == message_id)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
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

    /// Drop rows, reread: held rows other chat/kind or untrusted.
    fn start_over(&mut self) {
        self.rows.borrow_mut().reset_data(Vec::new());
        self.rows_changed();
        if self.loaded {
            self.loaded = false;
            self.loaded_changed();
        }
        self.reload();
    }

    /// Reread, keep read rows; fetch only missing (no-op event = one call).
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
                // Newer load started.
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

    /// Ids back: row per id, keeping read rows; unchanged list touches nothing.
    ///
    /// Arrival or delete = row inserts/removes, not reset: reset sends every view to row 0
    /// and rebuilds every delegate (viewer swiped to old picture jumped to newest).
    fn place(&mut self, ids: &[u32]) {
        let old: Vec<u32> = self
            .rows
            .borrow()
            .iter()
            .map(|row| row.message_id)
            .collect();
        if old != ids {
            // Empty: one reset, not a signal per row.
            if let Some((removals, insertions)) = edits(&old, ids).filter(|_| !old.is_empty()) {
                let mut rows = self.rows.borrow_mut();
                for index in removals {
                    rows.remove(index);
                }
                for (index, id) in insertions {
                    rows.insert(index, placeholder(id));
                }
            } else {
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
            }
            self.rows_changed();
        }
        self.set_loaded(true);
    }

    /// Fill rows by id, not index (list may change mid-read). Returns rows filled.
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

    /// Reread one message's row: limited download landing, or remote edit.
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
    /// Reread here too (core event follows): row goes at once, refusal reported as failure.
    /// No core: nothing to delete.
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
            // Also reread named message: limited download lands as this event.
            "MsgsChanged" => {
                self.reload();
                if let Some(message_id) = json::u32_opt(&payload, "msgId").filter(|id| *id != 0) {
                    self.refresh_one(message_id);
                }
            }
            // Core dropped events: rows may be wrong silently. Restart.
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

/// Row edits taking `old` ids to `new` in place: removals (indexes, back to front), then
/// insertions (index, id; front to back). `None` when kept ids changed order: no such path.
fn edits(old: &[u32], new: &[u32]) -> Option<(Vec<usize>, Vec<(usize, u32)>)> {
    let wanted: BTreeSet<u32> = new.iter().copied().collect();
    let removals: Vec<usize> = old
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, id)| !wanted.contains(id))
        .map(|(index, _)| index)
        .collect();
    let mut kept = old.iter().filter(|id| wanted.contains(id)).peekable();
    let mut insertions = Vec::new();
    for (index, id) in new.iter().enumerate() {
        if kept.peek() == Some(&id) {
            kept.next();
        } else {
            insertions.push((index, *id));
        }
    }
    // Kept row left over: order changed.
    if kept.next().is_some() {
        return None;
    }
    Some((removals, insertions))
}

/// Ids of these kinds in chat, newest first.
///
/// Core answers oldest first, not to re-sort; reversed only.
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
    use super::{edits, view_types};

    /// `old` after `edits`, as model applies them.
    fn applied(old: &[u32], new: &[u32]) -> Option<Vec<u32>> {
        let (removals, insertions) = edits(old, new)?;
        let mut rows = old.to_vec();
        for index in removals {
            rows.remove(index);
        }
        for (index, id) in insertions {
            rows.insert(index, id);
        }
        Some(rows)
    }

    #[test]
    fn arrivals_and_deletes_become_row_edits_not_a_reset() {
        // Newest first: arrival at top, one gone mid-list, one gone at end.
        let old = [9, 7, 5, 3, 1];
        let new = [11, 9, 5, 3];
        assert_eq!(applied(&old, &new), Some(new.to_vec()));
        let (removals, insertions) = edits(&old, &new).unwrap_or_default();
        // Back to front: earlier index still valid after each removal.
        assert_eq!(removals, vec![4, 1]);
        assert_eq!(insertions, vec![(0, 11)]);
        // Several in, several places.
        assert_eq!(
            applied(&[8, 4], &[10, 8, 6, 4, 2]),
            Some(vec![10, 8, 6, 4, 2])
        );
        // All gone, all new, none before.
        assert_eq!(applied(&[3, 2], &[]), Some(vec![]));
        assert_eq!(applied(&[], &[2, 1]), Some(vec![2, 1]));
        // Unchanged: nothing to do.
        assert_eq!(edits(&[2, 1], &[2, 1]), Some((vec![], vec![])));
    }

    #[test]
    fn kept_rows_out_of_order_have_no_edit_path() {
        assert_eq!(edits(&[2, 1], &[1, 2]), None);
        assert_eq!(edits(&[3, 2, 1], &[3, 1, 2, 0]), None);
    }

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
