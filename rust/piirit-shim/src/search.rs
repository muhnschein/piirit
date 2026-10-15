//! Search results: chats, contacts, messages, each counted.
//!
//! Three core calls, one flat model: `section` groups it in one view. Three lists in a flickable
//! nest flickables fighting for drag on Silica.

use std::cell::RefCell;
use std::collections::HashMap;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chatlist::chat_items;
use crate::contacts::contact_row;
use crate::core::connection;
use crate::json;
use crate::models::{SearchItem, SearchListModel};

/// Max message rows: common words match thousands; `message_total` keeps true count. Private:
/// rustdoc bars public links to it.
const MESSAGE_LIMIT: usize = 50;

/// One query's results.
///
/// ```qml
/// SearchResults { id: search; account_id: page.accountId; query: field.text }
/// SilicaListView { model: search.rows; section.property: "kind" }
/// ```
#[derive(QObject, Default)]
pub struct SearchResults {
    base: qt_base_class!(trait QObject),

    /// Account to search.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Emitted on account change.
    pub account_changed: qt_signal!(),

    /// Query; empty clears model (not "list all").
    pub query: qt_property!(QString; WRITE set_query NOTIFY query_changed),
    /// Emitted on query change.
    pub query_changed: qt_signal!(),

    /// Rows for `SilicaListView` `model`.
    pub rows: qt_property!(RefCell<SearchListModel>; CONST),

    // Counts stored, not counted: heading bindings read them inside the reset setting rows,
    // where borrowing rows aborts (already mutably borrowed).
    /// Rows, all kinds.
    pub count: qt_property!(u32; NOTIFY rows_changed),
    /// Chats found.
    pub chat_count: qt_property!(u32; NOTIFY rows_changed),
    /// Contacts found.
    pub contact_count: qt_property!(u32; NOTIFY rows_changed),
    /// Messages listed, capped.
    pub message_count: qt_property!(u32; NOTIFY rows_changed),
    /// Messages matched; may exceed listed.
    pub message_total: qt_property!(u32; NOTIFY rows_changed),
    /// Emitted after `rows` change.
    pub rows_changed: qt_signal!(),

    /// Current query answered; else empty list flashes "Nothing found".
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted on `loaded` change.
    pub loaded_changed: qt_signal!(),

    /// Failure, core's message.
    pub error: qt_signal!(message: QString),

    /// Run query again.
    pub reload: qt_method!(fn(&mut self)),

    /// Open 1:1 chat with contact, creating it; answers `chat_ready`.
    pub open_chat_with: qt_method!(fn(&mut self, contact_id: u32)),
    /// Chat ready to show.
    pub chat_ready: qt_signal!(chat_id: u32),

    /// Drops stale out-of-order answers.
    generation: u64,
}

impl SearchResults {
    /// Counts from rows, before they become model.
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

    /// Set account; re-search on change.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.reload();
        }
    }

    /// Set query; re-search on change.
    pub fn set_query(&mut self, query: QString) {
        if self.query.to_string() != query.to_string() {
            self.query = query;
            self.query_changed();
            self.reload();
        }
    }

    /// Empty model; nothing searched.
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

    /// Run query.
    pub fn reload(&mut self) {
        let account_id = self.account_id;
        // Spaces-only: core rejects; no query.
        let query = self.query.to_string().trim().to_string();
        if account_id == 0 || query.is_empty() {
            // Late answer must not land after clear.
            self.generation = self.generation.wrapping_add(1);
            self.clear();
            return;
        }
        // Silent before core up (banner per keystroke); page reruns on ready.
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<(Vec<SearchItem>, u32), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            // Query already typed past.
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok((rows, total)) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.message_total = total;
                        // Count first: QML reads counts once rows set.
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
                // Display order; sequential: core does one call at a time.
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

    /// Open 1:1 chat with contact.
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

/// Chats matching name, core order.
async fn matching_chats(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
) -> Result<Vec<SearchItem>, String> {
    // No flags: searches archived chats too.
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

/// Contacts matching name or address.
async fn matching_contacts(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
) -> Result<Vec<SearchItem>, String> {
    // listFlags 0: known, unblocked, as contact list.
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

/// Messages matching text, newest first; total before cut.
async fn matching_messages(
    rpc: &RpcClient,
    account_id: u32,
    query: &str,
) -> Result<(Vec<SearchItem>, u32), String> {
    // Three args, last chat to search (null: all); two rejected.
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

    // Hits' chats in one call: one per message is fifty round trips before first row.
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
            // Load failure: skip, no blank row.
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

/// Chat a message is in.
fn chat_of(message: Option<&serde_json::Value>) -> Option<u32> {
    json::u32_opt(message?, "chatId").filter(|id| *id != 0)
}
