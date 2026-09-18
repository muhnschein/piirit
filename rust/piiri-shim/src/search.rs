//! One search over everything, grouped the way the reference clients show
//! it: chats, then contacts, then messages, each under a counted heading.
//!
//! The three kinds arrive from three different core calls but land in one
//! flat model. QML's `section` groups a flat model and a view takes one
//! model, so three lists stacked in a flickable would be the alternative --
//! and nested flickables on Silica fight each other for the drag.

use std::cell::RefCell;
use std::collections::HashMap;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chatlist::chat_items;
use crate::contacts::contact_row;
use crate::core::connection;
use crate::json;
use crate::models::{SearchItem, SearchListModel};

/// How many message hits become rows.
///
/// A common word matches thousands of messages, and every one of them
/// costs a row to build and a delegate to draw. `message_total` says how
/// many there really were, so the heading can be honest about the cut.
///
/// Not a link from the properties above: it is private, and rustdoc
/// refuses a public item that points at one.
const MESSAGE_LIMIT: usize = 50;

/// Everything one query found, in one model.
///
/// ```qml
/// SearchResults { id: search; account_id: page.accountId; query: field.text }
/// SilicaListView { model: search.rows; section.property: "kind" }
/// ```
#[derive(QObject, Default)]
pub struct SearchResults {
    base: qt_base_class!(trait QObject),

    /// Which account to search. Setting it re-runs the query.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Emitted when the account changes.
    pub account_changed: qt_signal!(),

    /// What to look for. Empty empties the model rather than listing
    /// everything: an empty search is not a search.
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Emitted when the query changes.
    pub query_changed: qt_signal!(),

    /// The rows, for a `SilicaListView`'s `model`.
    pub rows: qt_property!(RefCell<SearchListModel>; CONST),

    // The counts are stored rather than counted out of the model on
    // demand. A section heading binds to them, so QML reads them from
    // inside the model reset that sets the rows -- and a reader that
    // borrowed the row list there would find it already mutably borrowed
    // and take the process down with it.
    /// How many rows there are, across all three kinds.
    pub count: qt_property!(u32; NOTIFY rows_changed),
    /// Chats found, for the heading.
    pub chat_count: qt_property!(u32; NOTIFY rows_changed),
    /// Contacts found, for the heading.
    pub contact_count: qt_property!(u32; NOTIFY rows_changed),
    /// Messages listed, which the model caps -- see `message_total`
    /// for how many there really were.
    pub message_count: qt_property!(u32; NOTIFY rows_changed),
    /// Messages that matched, which can be more than are listed.
    pub message_total: qt_property!(u32; NOTIFY rows_changed),
    /// Emitted after any change to `rows`.
    pub rows_changed: qt_signal!(),

    /// True once an answer to the current query has landed.
    ///
    /// Without it a list with nothing in it yet is indistinguishable from
    /// one that found nothing, and "Nothing found" flashes up between
    /// every keystroke and its answer.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted when `loaded` changes.
    pub loaded_changed: qt_signal!(),

    /// Searching failed. The message is the core's own.
    pub error: qt_signal!(message: QString),

    /// Run the query again.
    pub reload: qt_method!(fn(&mut self)),

    /// Open the one-to-one chat with a contact, creating it if needed.
    /// Answers on `chat_ready`. A contact result has no chat until this.
    pub open_chat_with: qt_method!(fn(&mut self, contact_id: u32)),
    /// A chat is ready to be shown.
    pub chat_ready: qt_signal!(chat_id: u32),

    /// Counts searches, so a slow answer to an older query cannot land on
    /// top of a newer one. Typing "anna" starts four of these and they are
    /// not answered in the order they were asked.
    generation: u64,
}

impl SearchResults {
    /// Take the counts from a set of rows, before they become the model.
    fn recount(&mut self, rows: &[SearchItem]) {
        let of = |kind: &str| {
            u32::try_from(
                rows.iter()
                    .filter(|row| row.kind.to_string() == kind)
                    .count(),
            )
            .unwrap_or(u32::MAX)
        };
        self.count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        self.chat_count = of("chat");
        self.contact_count = of("contact");
        self.message_count = of("message");
    }

    /// Set the account and search again if it changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.reload();
        }
    }

    /// Set the query and search again if it changed.
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.reload();
        }
    }

    /// Empty the model, and say that nothing has been searched for.
    fn clear(&mut self) {
        self.recount(&[]);
        self.message_total = 0;
        self.rows.borrow_mut().reset_data(Vec::new());
        self.rows_changed();
        if self.loaded {
            self.loaded = false;
            self.loaded_changed();
        }
    }

    /// Run the query.
    pub fn reload(&mut self) {
        let account_id = self.account_id;
        // A query of nothing but spaces is rejected by the core outright,
        // so it counts as no query rather than as an error banner.
        let query = self.query.to_string().trim().to_string();
        if account_id == 0 || query.is_empty() {
            // Still a new generation: an answer to the last query must not
            // land after the reader has cleared the field.
            self.generation = self.generation.wrapping_add(1);
            self.clear();
            return;
        }
        // Silent, not an error: the core comes up after the first page
        // does, and a banner per keystroke until it does is noise. The
        // page runs this again when the core reports ready.
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(Vec<SearchItem>, u32), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Answered after something newer was asked: these are results
            // for a query the reader has already typed past.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok((rows, total)) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.message_total = total;
                        // Counted first: setting the rows is what makes
                        // QML read the counts back.
                        this_mut.recount(&rows);
                        this_mut.rows.borrow_mut().reset_data(rows);
                    }
                    this.borrow().rows_changed();
                    if !this.borrow().loaded {
                        this.borrow_mut().loaded = true;
                        this.borrow().loaded_changed();
                    }
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                // In the order they are shown. Sequential rather than
                // joined: the core answers one call at a time anyway, and
                // three concurrent ones only reorder the waiting.
                let mut rows = matching_chats(&rpc, account_id, &query).await?;
                rows.extend(matching_contacts(&rpc, account_id, &query).await?);
                let (messages, total) = matching_messages(&rpc, account_id, &query).await?;
                rows.extend(messages);
                Ok::<_, String>((rows, total))
            }
            .await;
            done(result);
        });
    }

    /// Open the one-to-one chat with a contact.
    pub fn open_chat_with(&mut self, contact_id: u32) {
        let account_id = self.account_id;
        if account_id == 0 || contact_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<u32, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(chat_id) => this.borrow().chat_ready(chat_id),
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = rpc
                .call::<_, u32>("create_chat_by_contact_id", (account_id, contact_id))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }
}

/// Chats whose name matches, in the core's own order.
async fn matching_chats(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
) -> Result<Vec<SearchItem>, String> {
    // No flags: a plain query searches every chat, archived ones included,
    // which is what a search from the chat list should reach.
    let entries: Vec<u32> = rpc
        .call(
            "get_chatlist_entries",
            (
                account_id,
                Option::<u32>::None,
                Some(query.to_string()),
                Option::<u32>::None,
            ),
        )
        .await
        .map_err(|err| err.to_string())?;
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let items = chat_items(rpc, account_id, &entries).await?;
    Ok(entries
        .into_iter()
        .filter_map(|chat_id| items.get(&chat_id))
        .map(|chat| SearchItem {
            kind: "chat".into(),
            chat_id: chat.chat_id,
            contact_id: 0,
            message_id: 0,
            title: chat.name.clone(),
            subtitle: chat.preview.clone(),
            timestamp: chat.last_updated,
            color: chat.color.clone(),
            avatar_path: chat.avatar_path.clone(),
        })
        .collect())
}

/// Contacts whose name or address matches.
async fn matching_contacts(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
) -> Result<Vec<SearchItem>, String> {
    // listFlags 0, as the contact list uses: known, unblocked contacts.
    let contacts: Vec<serde_json::Value> = rpc
        .call("get_contacts", (account_id, 0, Some(query.to_string())))
        .await
        .map_err(|err| err.to_string())?;
    Ok(contacts
        .iter()
        .map(contact_row)
        .map(|contact| SearchItem {
            kind: "contact".into(),
            chat_id: 0,
            contact_id: contact.contact_id,
            message_id: 0,
            title: contact.display_name,
            subtitle: contact.address,
            timestamp: 0,
            color: contact.color,
            avatar_path: contact.avatar_path,
        })
        .collect())
}

/// Messages whose text matches, newest first as the core returns them, and
/// how many there were before the cut.
async fn matching_messages(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
) -> Result<(Vec<SearchItem>, u32), String> {
    // Three arguments: the last is the chat to search within, and null
    // means every chat. Two is rejected outright.
    let hits: Vec<u32> = rpc
        .call(
            "search_messages",
            (account_id, query.to_string(), Option::<u32>::None),
        )
        .await
        .map_err(|err| err.to_string())?;
    let total = u32::try_from(hits.len()).unwrap_or(u32::MAX);
    let shown: Vec<u32> = hits.into_iter().take(MESSAGE_LIMIT).collect();
    if shown.is_empty() {
        return Ok((Vec::new(), total));
    }

    let messages: HashMap<u32, serde_json::Value> = rpc
        .call("get_messages", (account_id, shown.clone()))
        .await
        .map_err(|err| err.to_string())?;

    // Which chat each hit is in, named in one call rather than one per
    // message: a search across a busy account otherwise costs fifty round
    // trips before the first row can be drawn.
    let mut chat_ids: Vec<u32> = Vec::new();
    for message_id in &shown {
        let Some(chat_id) = chat_of(messages.get(message_id)) else {
            continue;
        };
        if !chat_ids.contains(&chat_id) {
            chat_ids.push(chat_id);
        }
    }
    let chats = if chat_ids.is_empty() {
        HashMap::new()
    } else {
        chat_items(rpc, account_id, &chat_ids).await?
    };

    let rows = shown
        .into_iter()
        .filter_map(|message_id| {
            let message = messages.get(&message_id)?;
            // A message the core could not load comes back as
            // `{kind: "loadingError"}`; skip it rather than show a blank.
            if json::str_at(message, "kind") == "loadingError" {
                return None;
            }
            let chat_id = chat_of(Some(message)).unwrap_or(0);
            let chat = chats.get(&chat_id);
            Some(SearchItem {
                kind: "message".into(),
                chat_id,
                contact_id: 0,
                message_id,
                title: chat.map(|chat| chat.name.clone()).unwrap_or_default(),
                subtitle: json::text(message, "text"),
                timestamp: json::i64_at(message, "timestamp"),
                color: chat.map(|chat| chat.color.clone()).unwrap_or_default(),
                avatar_path: chat
                    .map(|chat| chat.avatar_path.clone())
                    .unwrap_or_default(),
            })
        })
        .collect();
    Ok((rows, total))
}

/// The chat a message object says it is in.
fn chat_of(message: Option<&serde_json::Value>) -> Option<u32> {
    json::u32_opt(message?, "chatId").filter(|id| *id != 0)
}
