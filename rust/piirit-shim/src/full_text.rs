//! Whole text of a too-long message. Messages only, not files.
//!
//! Message cut in two by sender: rest only via `get_message_html`; uncut ones can still outgrow
//! a bubble. Markdown rendered here as message list does, so page needs no shim round trip.

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;
use crate::{html, markdown};

/// Result of one load.
#[derive(Default)]
struct Loaded {
    text: String,
    /// Rendered where fetched, not on Qt thread: HTML part unbounded; page would stall.
    styled: String,
}

impl Loaded {
    /// Text, rendered.
    fn new(text: String) -> Self {
        let styled = markdown::render(&text);
        Self { text, styled }
    }
}

/// Whole text of one message.
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

    /// Account of message. Setting reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Message to read whole; 0 loads nothing. Setting reloads.
    pub message_id: qt_property!(u32; WRITE set_message_id NOTIFY source_changed),
    /// Emitted when read target changes.
    pub source_changed: qt_signal!(),

    /// Text as written.
    pub text: qt_property!(QString; NOTIFY loaded_changed),
    /// Same as `Text.StyledText`, for Markdown display; rendered once here, not per scroll.
    pub styled_text: qt_property!(QString; NOTIFY loaded_changed),
    /// Load running; page shows spinner.
    pub loading: qt_property!(bool; NOTIFY loaded_changed),
    /// True once load ends.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted on load start/end.
    pub loaded_changed: qt_signal!(),

    /// Failed; message from core.
    pub error: qt_signal!(message: QString),

    /// Read again.
    pub reload: qt_method!(fn(&mut self)),

    /// Stale answers dropped; see `ChatInfo`.
    generation: u64,
}

impl FullText {
    /// Set account; reload on change.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set message; reload on change.
    pub fn set_message_id(&mut self, message_id: u32) {
        if self.message_id != message_id {
            self.message_id = message_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Read again.
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

    /// Apply load result.
    fn finish(&mut self, result: Result<Loaded, String>) {
        match result {
            Ok(found) => {
                self.text = found.text.as_str().into();
                self.styled_text = found.styled.as_str().into();
                self.loading = false;
                self.loaded = true;
                self.loaded_changed();
            }
            Err(message) => self.fail(message),
        }
    }

    /// Clear shown text; signal error.
    fn fail(&mut self, message: String) {
        self.text = QString::default();
        self.styled_text = QString::default();
        self.loading = false;
        // Counts as loaded.
        self.loaded = true;
        self.loaded_changed();
        self.error(message.into());
    }
}

/// Whole text of one message.
///
/// `hasHtml`: rest only in HTML part; else text is whole, no fetch.
/// Rows: see `wasCut` in `MessageDelegate.qml`.
async fn fetch(rpc: &RpcClient, account_id: u32, message_id: u32) -> Result<Loaded, String> {
    let message: serde_json::Value = rpc
        .call("get_message", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    let text = json::str_at(&message, "text").to_string();
    if !json::flag(&message, "hasHtml") {
        return Ok(Loaded::new(text));
    }

    let whole: Option<String> = rpc
        .call("get_message_html", (account_id, message_id))
        .await
        .map_err(|err| err.to_string())?;
    let whole = whole.unwrap_or_default();
    // HTML part lost in transit: fall back to cut text.
    Ok(Loaded::new(match html::to_text(&whole) {
        found if found.is_empty() => text,
        found => found,
    }))
}
