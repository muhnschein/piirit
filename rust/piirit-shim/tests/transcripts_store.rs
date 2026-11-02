//! Transcript store: rows by message, deletion paths, search index kept by triggers.

#![allow(clippy::expect_used, clippy::unwrap_used)]

#[allow(dead_code)]
#[path = "../src/transcripts/store.rs"]
mod store;

use rusqlite::Connection;
use store::{Row, Store};

const DONE: u32 = 3;
const FAILED: u32 = 6;

/// Scratch dir, removed when dropped (panic too).
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "piirit-transcripts-store-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self) -> std::path::PathBuf {
        self.0.join("transcripts.sqlite")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn row(account: u32, msg: u32, state: u32, text: &str) -> Row {
    Row {
        account,
        msg,
        chat: 7,
        timestamp: 1_700_000_000 + i64::from(msg),
        state,
        language: "en".into(),
        model: "ggml-small-q5_1.bin".into(),
        text: text.into(),
        error: if state == FAILED {
            "audio: broken".into()
        } else {
            String::new()
        },
        updated: 1_800_000_000,
    }
}

/// Messages whose transcript matches `query`, via index only.
fn search(file: &std::path::Path, query: &str) -> Vec<(u32, u32)> {
    let connection = Connection::open(file).unwrap();
    let mut statement = connection
        .prepare(
            "SELECT t.account, t.msg FROM transcripts_fts f JOIN transcripts t ON t.id = f.rowid
             WHERE transcripts_fts MATCH ?1 ORDER BY t.account, t.msg",
        )
        .unwrap();
    let found = statement
        .query_map([query], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    found
}

/// Index entries matching `query`, rows or not: a stale entry counts.
fn hits(file: &std::path::Path, query: &str) -> u32 {
    Connection::open(file)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM transcripts_fts WHERE transcripts_fts MATCH ?1",
            [query],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_row_is_kept_per_message_and_rewritten_in_place() {
    let scratch = Scratch::new("rows");
    let store = Store::open(&scratch.file()).unwrap();
    assert_eq!(store.get(1, 13).unwrap(), None);

    store.put(&row(1, 13, FAILED, "")).unwrap();
    assert_eq!(store.get(1, 13).unwrap(), Some(row(1, 13, FAILED, "")));
    // Try again: same message, new outcome, still one row.
    store
        .put(&row(1, 13, DONE, "meet me at the station"))
        .unwrap();
    assert_eq!(
        store.get(1, 13).unwrap(),
        Some(row(1, 13, DONE, "meet me at the station"))
    );
    // Same message id in another profile is another message.
    store.put(&row(2, 13, DONE, "other profile")).unwrap();
    assert_eq!(
        store.get(1, 13).unwrap().unwrap().text,
        "meet me at the station"
    );
    assert_eq!(store.get(2, 13).unwrap().unwrap().text, "other profile");

    // Survives reopening: it is a file.
    drop(store);
    let store = Store::open(&scratch.file()).unwrap();
    assert_eq!(store.get(2, 13).unwrap().unwrap().text, "other profile");
    let rows: u32 = Connection::open(scratch.file())
        .unwrap()
        .query_row("SELECT COUNT(*) FROM transcripts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 2);
}

#[test]
fn rows_go_with_their_message_or_profile() {
    let store = Store::open_in_memory().unwrap();
    for (account, msg) in [(1, 10), (1, 11), (1, 12), (2, 10), (3, 10)] {
        store.put(&row(account, msg, DONE, "x")).unwrap();
    }
    assert_eq!(store.accounts().unwrap(), vec![1, 2, 3]);
    assert_eq!(store.messages(1).unwrap(), vec![10, 11, 12]);
    assert_eq!(
        store.known(1, &[9, 10, 12, 13]).unwrap(),
        [10, 12].into_iter().collect()
    );

    // MsgDeleted.
    assert_eq!(store.delete(1, 11).unwrap(), 1);
    assert_eq!(store.delete(1, 11).unwrap(), 0);
    assert_eq!(store.get(1, 11).unwrap(), None);
    // Sweep: messages gone while app was closed.
    assert_eq!(store.delete_messages(1, &[10, 99]).unwrap(), 1);
    assert_eq!(store.messages(1).unwrap(), vec![12]);
    // Profiles removed: only listed ones keep rows.
    assert_eq!(store.keep_accounts(&[1, 3]).unwrap(), 1);
    assert_eq!(store.accounts().unwrap(), vec![1, 3]);
    assert!(store.get(2, 10).unwrap().is_none());
}

#[test]
fn the_search_index_follows_every_write() {
    let scratch = Scratch::new("search");
    let store = Store::open(&scratch.file()).unwrap();
    assert!(
        store.has_search(),
        "the host SQLite has FTS5, so the store must build its search index"
    );
    store
        .put(&row(1, 1, DONE, "the train leaves at nine"))
        .unwrap();
    store.put(&row(1, 2, DONE, "bring the umbrella")).unwrap();
    store.put(&row(2, 1, DONE, "train tickets bought")).unwrap();
    assert_eq!(search(&scratch.file(), "train"), vec![(1, 1), (2, 1)]);
    assert_eq!(search(&scratch.file(), "umbrella"), vec![(1, 2)]);

    // Rewrite drops old words, adds new ones.
    store
        .put(&row(1, 1, DONE, "the bus leaves at nine"))
        .unwrap();
    assert_eq!(search(&scratch.file(), "train"), vec![(2, 1)]);
    assert_eq!(search(&scratch.file(), "bus"), vec![(1, 1)]);
    assert_eq!(hits(&scratch.file(), "train"), 1);
    // Each delete path leaves no stale entry.
    store.delete(1, 2).unwrap();
    assert_eq!(hits(&scratch.file(), "umbrella"), 0);
    store.keep_accounts(&[1]).unwrap();
    assert_eq!(hits(&scratch.file(), "train"), 0);
    store.delete_messages(1, &[1]).unwrap();
    assert_eq!(hits(&scratch.file(), "bus"), 0);
    // Index agrees with table (FTS5's own check).
    Connection::open(scratch.file())
        .unwrap()
        .execute(
            "INSERT INTO transcripts_fts (transcripts_fts) VALUES ('integrity-check')",
            [],
        )
        .unwrap();
}
