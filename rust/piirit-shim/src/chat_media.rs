//! Pictures, sounds, files, apps, calls of a chat: pages behind contact/group page tiles.
//!
//! Core's `get_chat_media`: ids of up to three kinds (core's limit; reference clients' four
//! tabs each fit one call, hence four pages), oldest first, not to re-sort; only reversed.
//!
//! Messages read as conversation (`get_messages`, pages of fifty) but top down unasked:
//! placeholder rows from ids, first screen first. Read rows kept across reload: new picture =
//! one row fetched, one row inserted; views keep their place (swiped viewer stays on its
//! picture).
//!
//! Gallery also has pictures, videos not yet downloaded: core keeps such as Text pre-message
//! (name, size, no file) until fetched, so `get_chat_media` never names them. Found by reading
//! chat's Text messages (`held_kind`), once per process (`PLAIN`).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chat::{act_on_message, fetch_messages, message_entries};
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

/// Suffixes core makes a picture (`guess_msgtype_from_suffix`); GIF apart.
const PICTURES: [&str; 5] = ["jpg", "jpeg", "jpe", "png", "webp"];

/// Suffixes core makes a video; `m4v`, `3g2` as `video.rs` reads them.
const VIDEOS: [&str; 7] = ["mp4", "m4v", "mov", "3gp", "3g2", "avi", "wmv"];

/// Text messages read and found no held-back picture or video, by account and chat. Never
/// become one: pre-message turns Image on download, so leaves Text. Process-wide: each viewer
/// and grid has own model; long chat read once, not per open.
static PLAIN: Mutex<BTreeMap<(u32, u32), BTreeSet<u32>>> = Mutex::new(BTreeMap::new());

/// Gallery kind a not-yet-downloaded message's file will have, by name; `None`: not one, or
/// downloaded. Core's pre-message is Text until fetched; real kind not in JSON. Picture needs
/// size: core sends it for Image and GIF, never File (a `.jpg` sent as file). Video by name
/// alone: few senders size it.
fn held_kind(row: &MessageListItem) -> Option<&'static str> {
    let state = row.download_state.to_string();
    if state.is_empty() || state == "Done" {
        return None;
    }
    let name = row.file_name.to_string();
    let suffix = std::path::Path::new(&name)
        .extension()
        .map(|suffix| suffix.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let sized = row.image_width > 0 && row.image_height > 0;
    if suffix == "gif" && sized {
        Some("Gif")
    } else if PICTURES.contains(&suffix.as_str()) && sized {
        Some("Image")
    } else if VIDEOS.contains(&suffix.as_str()) {
        Some("Video")
    } else {
        None
    }
}

/// Row as gallery shows it: held-back one as kind its file will be (`held_kind`).
fn as_gallery(mut row: MessageListItem) -> MessageListItem {
    if let Some(kind) = held_kind(&row) {
        row.view_type = kind.into();
    }
    row
}

/// Not yet downloaded: core's text and Download stand in for picture.
fn is_held(row: &MessageListItem) -> bool {
    let state = row.download_state.to_string();
    !state.is_empty() && state != "Done"
}

/// Plain Text ids known for chat.
fn plain_of(key: (u32, u32)) -> BTreeSet<u32> {
    PLAIN
        .lock()
        .map(|plain| plain.get(&key).cloned().unwrap_or_default())
        .unwrap_or_default()
}

/// Remember Text ids read and found plain.
fn note_plain(key: (u32, u32), ids: impl IntoIterator<Item = u32>) {
    if let Ok(mut plain) = PLAIN.lock() {
        plain.entry(key).or_default().extend(ids);
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
    /// Rows oldest first (conversation's order); default newest first. Set reloads.
    pub oldest_first: qt_property!(bool; WRITE set_oldest_first NOTIFY source_changed),
    /// Account, chat, kind or order changed.
    pub source_changed: qt_signal!(),

    /// Rows, newest first unless `oldest_first`, for `model`; id-only until read (row's
    /// `loaded`).
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
    /// Fetch not-yet-downloaded picture or video (as conversation's Download).
    pub download_full: qt_method!(fn(&mut self, message_id: u32)),
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

    /// Set order; start over if changed.
    pub fn set_oldest_first(&mut self, oldest_first: bool) {
        if self.oldest_first != oldest_first {
            self.oldest_first = oldest_first;
            self.source_changed();
            self.start_over();
        }
    }

    /// Drop rows, reread: held rows other chat/kind/order or untrusted.
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
    ///
    /// Gallery, two passes: rows known (pictures, and held-back ones in their place), then chat's
    /// unread Text messages for held-back ones (`held_kind`). Held-back rows reread each time:
    /// state moves (fetching, failed, landed as picture).
    pub fn reload(&mut self) {
        let (account_id, chat_id) = (self.account_id, self.chat_id);
        let kind = self.kind.to_string();
        let oldest_first = self.oldest_first;
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
        let ids = |held: bool| -> BTreeSet<u32> {
            self.rows
                .borrow()
                .iter()
                .filter(|row| row.loaded && is_held(row) == held)
                .map(|row| row.message_id)
                .collect()
        };
        let load = Load {
            rpc,
            account_id,
            chat_id,
            kinds: (first, second, third),
            gallery: kind == "gallery",
            oldest_first,
            known: ids(false),
            held_before: ids(true),
            generation,
            current,
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let step = queued_callback(move |step: Step| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation.load(Ordering::Relaxed) != generation {
                return;
            }
            this.borrow_mut().take(step);
        });
        runtime.spawn(async move {
            if let Err(err) = load.run(&step).await {
                step(Step::Failed(err));
            }
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
        let gallery = self.kind.to_string() == "gallery";
        for item in items {
            if let Some(index) = index_of.get(&item.message_id).copied() {
                // Reread held-back one (fetching, failed) stays gallery's kind.
                rows.change_line(index, if gallery { as_gallery(item) } else { item });
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

    /// Fetch rest of held-back message (as `ChatMessages::download_full`). Row reread now
    /// (fetching), again on `MsgsChanged` once landed.
    pub fn download_full(&mut self, message_id: u32) {
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
            match result {
                Ok(()) => this.borrow_mut().refresh_one(message_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });
        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("download_full_message", (account_id, message_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
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

/// One reload's inputs, moved into its task.
struct Load {
    rpc: Arc<RpcClient>,
    account_id: u32,
    chat_id: u32,
    /// `get_chat_media` kinds (`view_types`).
    kinds: (&'static str, Option<&'static str>, Option<&'static str>),
    /// Gallery: held-back pictures and videos too.
    gallery: bool,
    oldest_first: bool,
    /// Rows read, not held back: not fetched again.
    known: BTreeSet<u32>,
    /// Held-back rows in model now.
    held_before: BTreeSet<u32>,
    /// This load's number; `current` the latest's.
    generation: u64,
    current: Arc<AtomicU64>,
}

impl Load {
    /// Newer load started: this one stops.
    fn superseded(&self) -> bool {
        self.current.load(Ordering::Relaxed) != self.generation
    }

    /// Oldest-first ids in model's order.
    fn shown(&self, mut ids: Vec<u32>) -> Vec<u32> {
        if !self.oldest_first {
            ids.reverse();
        }
        ids
    }

    /// Read, handing steps over as they come; stops quietly if superseded.
    async fn run(self, step: &(impl Fn(Step) + Sync)) -> Result<(), String> {
        let (rpc, account_id, chat_id) = (&self.rpc, self.account_id, self.chat_id);
        let media = fetch_ids(rpc, account_id, chat_id, self.kinds).await?;
        // Pre-messages are Text: gallery's held-back ones among these.
        let texts = if self.gallery {
            fetch_ids(rpc, account_id, chat_id, ("Text", None, None)).await?
        } else {
            Vec::new()
        };
        // Held before, still Text: stays where it was until reread.
        let held: BTreeSet<u32> = texts
            .iter()
            .copied()
            .filter(|id| self.held_before.contains(id))
            .collect();
        let ids = in_order(rpc, account_id, chat_id, &media, &held).await?;
        let wanted: Vec<u32> = self
            .shown(media.clone())
            .into_iter()
            .filter(|id| !self.known.contains(id))
            .collect();
        step(Step::Ids(self.shown(ids)));
        for page in wanted.chunks(PAGE) {
            if self.superseded() {
                return Ok(());
            }
            step(Step::Rows(fetch_messages(rpc, account_id, page).await?));
        }
        if self.gallery {
            let Some(found) = self.find_held(&texts).await? else {
                return Ok(());
            };
            let now: BTreeSet<u32> = found.iter().map(|row| row.message_id).collect();
            if now != held {
                let ids = in_order(rpc, account_id, chat_id, &media, &now).await?;
                step(Step::Ids(self.shown(ids)));
            }
            if !found.is_empty() {
                step(Step::Rows(found));
            }
        }
        step(Step::Done);
        Ok(())
    }

    /// Held-back gallery rows among chat's Text messages not read before (held ones reread:
    /// state moves); plain ones noted (`PLAIN`). Newest first: recent ones found first. `None`:
    /// superseded.
    async fn find_held(&self, texts: &[u32]) -> Result<Option<Vec<MessageListItem>>, String> {
        let key = (self.account_id, self.chat_id);
        let plain = plain_of(key);
        let unread: Vec<u32> = texts
            .iter()
            .rev()
            .copied()
            .filter(|id| !self.known.contains(id) && !plain.contains(id))
            .collect();
        let mut found: Vec<MessageListItem> = Vec::new();
        for page in unread.chunks(PAGE) {
            if self.superseded() {
                return Ok(None);
            }
            let items = fetch_messages(&self.rpc, self.account_id, page).await?;
            let (gallery_rows, others): (Vec<MessageListItem>, Vec<MessageListItem>) =
                items.into_iter().partition(|row| held_kind(row).is_some());
            note_plain(key, others.iter().map(|row| row.message_id));
            // As kind to come: `fill`.
            found.extend(gallery_rows);
        }
        Ok(Some(found))
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

/// Media and held-back ids in conversation's order (core's: time, then id), oldest first. Two
/// lists don't merge without times; conversation's id list has them in order. No held-back:
/// media as core gave it.
async fn in_order(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
    media: &[u32],
    held: &BTreeSet<u32>,
) -> Result<Vec<u32>, String> {
    if held.is_empty() {
        return Ok(media.to_vec());
    }
    let wanted: BTreeSet<u32> = media.iter().chain(held).copied().collect();
    Ok(message_entries(rpc, account_id, chat_id)
        .await?
        .into_iter()
        .map(|entry| entry.message_id)
        .filter(|id| wanted.contains(id))
        .collect())
}

/// Ids of these kinds in chat, oldest first, as core answers (not to re-sort; reversed only).
async fn fetch_ids(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
    (first, second, third): (&str, Option<&str>, Option<&str>),
) -> Result<Vec<u32>, String> {
    rpc.call(
        "get_chat_media",
        (account_id, Some(chat_id), first, second, third),
    )
    .await
    .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::{as_gallery, edits, held_kind, view_types};
    use crate::models::MessageListItem;

    /// Row as core's pre-message reads: Text, name, size if sender gave one.
    fn pre(state: &str, name: &str, width: i32, height: i32) -> MessageListItem {
        MessageListItem {
            view_type: "Text".into(),
            download_state: state.into(),
            file_name: name.into(),
            image_width: width,
            image_height: height,
            ..MessageListItem::default()
        }
    }

    #[test]
    fn not_yet_downloaded_pictures_and_videos_are_told_by_name_and_size() {
        assert_eq!(
            held_kind(&pre("Available", "beach.JPG", 800, 600)),
            Some("Image")
        );
        assert_eq!(
            held_kind(&pre("Failure", "shot.webp", 10, 10)),
            Some("Image")
        );
        assert_eq!(
            held_kind(&pre("InProgress", "cat.gif", 300, 2)),
            Some("Gif")
        );
        assert_eq!(
            held_kind(&pre("Available", "trip.mp4", 0, 0)),
            Some("Video")
        );
        assert_eq!(held_kind(&pre("Available", "old.3gp", 0, 0)), Some("Video"));
        // Picture sent as file: core never sizes files.
        assert_eq!(held_kind(&pre("Available", "scan.png", 0, 0)), None);
        assert_eq!(held_kind(&pre("Available", "cat.gif", 300, 0)), None);
        assert_eq!(held_kind(&pre("Available", "report.pdf", 0, 0)), None);
        assert_eq!(held_kind(&pre("Available", "", 800, 600)), None);
        // Downloaded, or never held: core's own kind stands.
        assert_eq!(held_kind(&pre("Done", "beach.jpg", 800, 600)), None);
        assert_eq!(held_kind(&pre("", "trip.mp4", 0, 0)), None);
        // Gallery shows held one as kind to come; others untouched.
        assert_eq!(
            as_gallery(pre("Available", "trip.mp4", 0, 0))
                .view_type
                .to_string(),
            "Video"
        );
        assert_eq!(
            as_gallery(pre("Available", "report.pdf", 0, 0))
                .view_type
                .to_string(),
            "Text"
        );
    }

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
