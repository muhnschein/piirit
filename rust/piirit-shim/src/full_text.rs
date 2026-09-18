//! The whole text of a message too long for the bubble it arrived in.
//!
//! A message the sending core cut in two has its rest behind
//! `get_message_html` and nowhere else, and even one it did not cut can
//! run longer than a bubble should be. Either way what the reader wants
//! is a page, a scrollbar and the words.
//!
//! Only a message. A file somebody attached was read here too for a
//! while, so that a note or a to-do list could be shown rather than
//! handed to a phone that has nothing to open it with -- and the reader
//! whose problem that was said plainly that there should be no such
//! page. A file is opened elsewhere or kept; reading belongs to
//! messages.
//!
//! The Markdown rendering is made here as the message list makes its
//! own, so the page draws what the reader's setting asks for without
//! going back to the shim for it.

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;
use crate::{html, markdown};

/// What one load found.
#[derive(Default)]
struct Loaded {
    text: String,
}

/// The whole text of one message, or of one text file.
///
/// ```qml
/// FullText {
///     id: whole
///     account_id: page.accountId
///     message_id: page.messageId
/// }
/// Label { text: whole.text }
/// ```
#[derive(QObject, Default)]
pub struct FullText {
    base: qt_base_class!(trait QObject),

    /// Which account the message belongs to. Setting it reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Which message to read whole, or 0 to read a file instead.
    /// Setting it reloads.
    pub message_id: qt_property!(u32; WRITE set_message_id NOTIFY source_changed),
    /// Emitted when what is being read changes.
    pub source_changed: qt_signal!(),

    /// The text, as written.
    pub text: qt_property!(QString; NOTIFY loaded_changed),
    /// The same rendered as `Text.StyledText`, for when Markdown is
    /// drawn. Made here so the page renders it once rather than on every
    /// scroll.
    pub styled_text: qt_property!(QString; NOTIFY loaded_changed),
    /// A load is under way. What the page shows a spinner for.
    pub loading: qt_property!(bool; NOTIFY loaded_changed),
    /// True once a load has finished, however it went.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted after every load, and when one starts.
    pub loaded_changed: qt_signal!(),

    /// Nothing could be read. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Read it again.
    pub reload: qt_method!(fn(&mut self)),

    /// Counts loads, so a slow answer to an older question cannot land
    /// on top of a newer one; see `ChatInfo`.
    generation: u64,
}

impl FullText {
    /// Set the account and reload if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set the message and reload if it changed.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            self.message_id = message_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Read it again.
    pub fn reload(&mut self) {
        let (account_id, message_id) = (self.account_id, self.message_id);
        if account_id == 0 || message_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.fail("not started".to_string());
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.loading = true;
        self.loaded = false;
        self.loaded_changed();

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Loaded, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().finish(result);
        });

        runtime.spawn(async move {
            done(fetch(&rpc, account_id, message_id).await);
        });
    }

    /// Take what a load found, however it went.
    fn finish(&mut self, result: Result<Loaded, String>) {
        match result {
            Ok(found) => {
                self.text = found.text.as_str().into();
                self.styled_text = markdown::render(&found.text).into();
                self.loading = false;
                self.loaded = true;
                self.loaded_changed();
            }
            Err(message) => self.fail(message),
        }
    }

    /// Nothing to show, and why.
    fn fail(&mut self, message: String) {
        self.text = QString::default();
        self.styled_text = QString::default();
        self.loading = false;
        // Loaded in the sense that matters: the wait is over.
        self.loaded = true;
        self.loaded_changed();
        self.error(message.into());
    }
}

/// The whole text of one message.
///
/// `hasHtml` marks the ones with more to them than their text: for those
/// the rest is in the HTML part, and only there. For everything else the
/// message's own text *is* the whole of it, and asking the core for an
/// HTML part it has not got would be a round trip for an empty string.
/// (Which of those the row offers this page for is the row's own
/// question, and a narrower one: see `wasCut` in `MessageDelegate.qml`.)
async fn fetch(rpc: &RpcClient, account_id: u32, message_id: u32) -> Result<Loaded, String> {
    let message: serde_json::Value = rpc
        .call("get_message", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    let text = json::str_at(&message, "text").to_string();
    if !json::flag(&message, "hasHtml") {
        return Ok(Loaded { text });
    }

    let whole: Option<String> = rpc
        .call("get_message_html", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    let whole = whole.unwrap_or_default();
    Ok(Loaded {
        // Falling back to the cut version rather than to nothing: a
        // message whose HTML part did not survive its trip still has the
        // words that did.
        text: match html::to_text(&whole) {
            found if found.is_empty() => text,
            found => found,
        },
    })
}
