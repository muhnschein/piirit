//! Loads a chat before its page exists, so page opens full.
//!
//! Fetch runs before push, while chat list shows: during transition it stutters; after, page
//! arrives empty. Page's model takes result, no core call. Holds one chat, latest asked.
//! Taken, not read: later reopen fetches normally, never stale.

use std::sync::Mutex;

use qmetaobject::*;

use crate::chat::{open_chat, ChatShape, Entry};
use crate::core::connection;
use crate::models::MessageListItem;

/// Prefetch result, waiting for its model.
struct Cached {
    account_id: u32,
    chat_id: u32,
    shape: ChatShape,
    /// Every message, by day; not worth fetching twice.
    entries: Vec<Entry>,
    /// Page's first fill.
    rows: Vec<MessageListItem>,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// Chat shape, all messages, first page's content.
type Loaded = (ChatShape, Vec<Entry>, Vec<MessageListItem>);

/// Hand prefetch to next model asking.
fn store(account_id: u32, chat_id: u32, loaded: Loaded) {
    let (shape, entries, rows) = loaded;
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some(Cached {
            account_id,
            chat_id,
            shape,
            entries,
            rows,
        });
    }
}

/// Take this chat's prefetch if held; others stay.
pub(crate) fn take(account_id: u32, chat_id: u32) -> Option<Loaded> {
    let mut cache = CACHE.lock().ok()?;
    let held = cache.as_ref()?;
    if held.account_id != account_id || held.chat_id != chat_id {
        return None;
    }
    cache
        .take()
        .map(|held| (held.shape, held.entries, held.rows))
}

/// Loads a chat so page opens full.
/// ```qml
/// ChatPrefetch {
///     id: prefetch
///     account_id: page.accountId
///     onReady: pageStack.push(Qt.resolvedUrl("ConversationPage.qml"), { ... })
/// }
/// ```
#[derive(QObject, Default)]
pub struct ChatPrefetch {
    base: qt_base_class!(trait QObject),

    /// Whose chats these are.
    pub account_id: qt_property!(u32),

    /// Start loading; answers on `ready`. `find_message_id` (0: newest) to open at.
    pub start: qt_method!(fn(&mut self, chat_id: u32, find_message_id: u32)),

    /// Loaded or failed: opening empty beats never.
    pub ready: qt_signal!(chat_id: u32),

    /// True while loading.
    pub loading: qt_property!(bool; NOTIFY loading_changed),
    /// `loading` changed.
    pub loading_changed: qt_signal!(),

    /// Prefetch counter: earlier tap's answer mustn't win.
    generation: u64,
}

impl ChatPrefetch {
    /// Start loading chat.
    pub fn start(&mut self, chat_id: u32, find_message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            // Nothing to wait for: page opens, reports it.
            self.ready(chat_id);
            return;
        };

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        if !self.loading {
            self.loading = true;
            self.loading_changed();
        }

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Option<Loaded>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Newer tap started: this would open wrong page.
            if this.borrow().generation != generation {
                return;
            }
            // Under fetch's account, not current: property may have changed.
            if let Some(loaded) = result {
                store(account_id, chat_id, loaded);
            }
            {
                let mut this_mut = this.borrow_mut();
                this_mut.loading = false;
            }
            this.borrow().loading_changed();
            this.borrow().ready(chat_id);
        });

        runtime.spawn(async move {
            // First fill only, not all: would undo placeholders. Around search hit: no jump.
            let loaded = open_chat(&rpc, account_id, chat_id, find_message_id).await;
            // Errors: page's model reruns, shows them.
            done(loaded.ok());
        });
    }
}
