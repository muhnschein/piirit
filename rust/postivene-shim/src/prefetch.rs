//! Loading a conversation before the page that shows it exists.
//!
//! The fetch runs before the push, while the reader is still looking at
//! the chat list: during the page transition it stutters the transition,
//! and after it the page arrives empty and fills a moment later. What it
//! finds is left here for the model the page then builds, which takes it
//! without going near the core, so the page arrives with its rows in it.
//!
//! One chat is kept, the one most recently asked for. It is taken rather
//! than read, so a second page built from the same account and chat --
//! reopening it half an hour later, say -- does a normal fetch and cannot
//! be served something stale.

use std::sync::Mutex;

use qmetaobject::*;

use crate::chat::{
    chat_shape, fetch_messages, ids_of, message_entries, opening_page, ChatShape, Entry,
};
use crate::core::connection;
use crate::models::MessageListItem;

/// What one prefetch found, waiting for the model that asked for it.
struct Cached {
    account_id: u32,
    chat_id: u32,
    shape: ChatShape,
    /// Every message in the chat, each under its day: the model holds a row
    /// for each. Cheap to carry and pointless to fetch twice.
    entries: Vec<Entry>,
    /// The messages of the one page that is filled in to start with.
    rows: Vec<MessageListItem>,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// What one prefetch found: the chat's shape, every message in it, and the
/// content of the one page that is filled in to start with.
type Loaded = (ChatShape, Vec<Entry>, Vec<MessageListItem>);

/// Hand a finished prefetch over to whichever model asks for it next.
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

/// Take the prefetch for this chat, if the one being held is it.
///
/// Anything else is left alone: a prefetch for another chat is still
/// wanted by the page that asked for it.
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

/// Loads a chat so a page can be opened onto it already full.
///
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

    /// Start loading a chat. Answers on `ready`, whatever happens.
    ///
    /// `find_message_id` names a message the chat should open at -- what a
    /// search result gives -- and 0 opens at the newest messages.
    pub start: qt_method!(fn(&mut self, chat_id: u32, find_message_id: u32)),

    /// The chat is loaded, or loading it failed and waiting longer would
    /// not help. Either way the page can be opened now: a page that never
    /// opens is worse than one that opens empty.
    pub ready: qt_signal!(chat_id: u32),

    /// True while a prefetch is running, for a busy indicator.
    pub loading: qt_property!(bool; NOTIFY loading_changed),
    /// Emitted when `loading` changes.
    pub loading_changed: qt_signal!(),

    /// Counts prefetches, so a chat tapped while an earlier one is still
    /// loading does not open onto the earlier one.
    generation: u64,
}

impl ChatPrefetch {
    /// Start loading a chat.
    pub fn start(&mut self, chat_id: u32, find_message_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || chat_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            // Nothing to wait for; let the page open and say so itself.
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
            // A second tap started a newer one: that is the chat the
            // reader is waiting for, and this answer would open the wrong
            // page.
            if this.borrow().generation != generation {
                return;
            }
            // Under the account the fetch was made for, not the one the
            // object points at now: a property change while it ran would
            // otherwise file this chat's rows under another account.
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
            let loaded = async {
                let shape = chat_shape(&rpc, account_id, chat_id).await;
                let entries = message_entries(&rpc, account_id, chat_id).await?;
                // The same page the model would have filled in for
                // itself. Fetching every message in the chat here would
                // put back exactly the cost the placeholders remove, one
                // step earlier. Around the message a search found, when
                // there is one, so the page arrives showing it rather than
                // showing today and jumping.
                let page = ids_of(opening_page(&entries, find_message_id));
                let rows = fetch_messages(&rpc, account_id, &page).await?;
                Ok::<_, String>((shape, entries, rows))
            }
            .await;
            // A failure is not reported here: the page's own model will
            // run the same fetch and has somewhere to show the error.
            done(loaded.ok());
        });
    }
}
