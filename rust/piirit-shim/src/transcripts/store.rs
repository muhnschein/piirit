//! Transcripts on disk: `SQLite` (phone's `libsqlite3`), one row per voice message.
//!
//! Self-contained (std, `rusqlite`): tests include it by `#[path]`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

/// `STATE_DONE`; store stays std-only (tests include it).
pub(crate) const DONE: u32 = 3;

/// Schema version in `user_version`.
const VERSION: i32 = 1;

/// `id` aliases rowid: stable across `VACUUM`, which external-content FTS needs.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS transcripts (
        id INTEGER PRIMARY KEY,
        account INTEGER NOT NULL,
        msg INTEGER NOT NULL,
        chat INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        state INTEGER NOT NULL,
        language TEXT NOT NULL DEFAULT '',
        model TEXT NOT NULL DEFAULT '',
        text TEXT NOT NULL DEFAULT '',
        error TEXT NOT NULL DEFAULT '',
        updated INTEGER NOT NULL,
        UNIQUE (account, msg)
    );
    CREATE INDEX IF NOT EXISTS transcripts_by_chat ON transcripts (account, chat, timestamp);
";

/// Search index over `text`, kept by triggers. Needs FTS5; absent without it.
const SEARCH: &str = "
    CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts
        USING fts5(text, content='transcripts', content_rowid='id');
    CREATE TRIGGER IF NOT EXISTS transcripts_fts_insert AFTER INSERT ON transcripts BEGIN
        INSERT INTO transcripts_fts (rowid, text) VALUES (new.id, new.text);
    END;
    CREATE TRIGGER IF NOT EXISTS transcripts_fts_delete AFTER DELETE ON transcripts BEGIN
        INSERT INTO transcripts_fts (transcripts_fts, rowid, text)
            VALUES ('delete', old.id, old.text);
    END;
    CREATE TRIGGER IF NOT EXISTS transcripts_fts_update AFTER UPDATE ON transcripts BEGIN
        INSERT INTO transcripts_fts (transcripts_fts, rowid, text)
            VALUES ('delete', old.id, old.text);
        INSERT INTO transcripts_fts (rowid, text) VALUES (new.id, new.text);
    END;
";

/// Triggers naming index: dropped when FTS5 missing, else every write fails.
const NO_SEARCH: &str = "
    DROP TRIGGER IF EXISTS transcripts_fts_insert;
    DROP TRIGGER IF EXISTS transcripts_fts_delete;
    DROP TRIGGER IF EXISTS transcripts_fts_update;
";

/// One message's outcome.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Row {
    /// Profile.
    pub account: u32,
    /// Message.
    pub msg: u32,
    /// Chat of message.
    pub chat: u32,
    /// Message's time, Unix seconds.
    pub timestamp: i64,
    /// `STATE_*` outcome: done, no speech, other language, failed.
    pub state: u32,
    /// Whisper code.
    pub language: String,
    /// Model that made it.
    pub model: String,
    /// Transcript.
    pub text: String,
    /// Why it failed.
    pub error: String,
    /// When written, Unix seconds.
    pub updated: i64,
}

/// Open store.
pub(crate) struct Store {
    connection: Connection,
    search: bool,
}

impl Store {
    /// Opens or creates `path`.
    pub(crate) fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::with(Connection::open(path)?)
    }

    /// Store in memory: file unusable, session still works.
    pub(crate) fn open_in_memory() -> rusqlite::Result<Self> {
        Self::with(Connection::open_in_memory()?)
    }

    fn with(connection: Connection) -> rusqlite::Result<Self> {
        connection.execute_batch(SCHEMA)?;
        let search = match connection.execute_batch(SEARCH) {
            Ok(()) => true,
            Err(err) if err.to_string().contains("no such module") => {
                connection.execute_batch(NO_SEARCH)?;
                false
            }
            Err(err) => return Err(err),
        };
        connection.pragma_update(None, "user_version", VERSION)?;
        Ok(Self { connection, search })
    }

    /// Search index kept (FTS5 present).
    pub(crate) fn has_search(&self) -> bool {
        self.search
    }

    /// Row of message, if any.
    pub(crate) fn get(&self, account: u32, msg: u32) -> rusqlite::Result<Option<Row>> {
        self.connection
            .query_row(
                "SELECT account, msg, chat, timestamp, state, language, model, text, error, updated
                 FROM transcripts WHERE account = ?1 AND msg = ?2",
                params![account, msg],
                |row| {
                    Ok(Row {
                        account: row.get(0)?,
                        msg: row.get(1)?,
                        chat: row.get(2)?,
                        timestamp: row.get(3)?,
                        state: row.get(4)?,
                        language: row.get(5)?,
                        model: row.get(6)?,
                        text: row.get(7)?,
                        error: row.get(8)?,
                        updated: row.get(9)?,
                    })
                },
            )
            .optional()
    }

    /// Write row; replaces message's earlier one (update, so index follows).
    pub(crate) fn put(&self, row: &Row) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO transcripts
                 (account, msg, chat, timestamp, state, language, model, text, error, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT (account, msg) DO UPDATE SET
                 chat = excluded.chat, timestamp = excluded.timestamp, state = excluded.state,
                 language = excluded.language, model = excluded.model, text = excluded.text,
                 error = excluded.error, updated = excluded.updated",
            params![
                row.account,
                row.msg,
                row.chat,
                row.timestamp,
                row.state,
                row.language,
                row.model,
                row.text,
                row.error,
                row.updated
            ],
        )?;
        Ok(())
    }

    /// Delete one message's row.
    pub(crate) fn delete(&self, account: u32, msg: u32) -> rusqlite::Result<usize> {
        self.connection.execute(
            "DELETE FROM transcripts WHERE account = ?1 AND msg = ?2",
            params![account, msg],
        )
    }

    /// Delete rows of profiles not in `keep`.
    pub(crate) fn keep_accounts(&self, keep: &[u32]) -> rusqlite::Result<usize> {
        let keep: BTreeSet<u32> = keep.iter().copied().collect();
        let mut gone = 0_usize;
        for account in self.accounts()? {
            if !keep.contains(&account) {
                gone = gone.saturating_add(
                    self.connection
                        .execute("DELETE FROM transcripts WHERE account = ?1", [account])?,
                );
            }
        }
        Ok(gone)
    }

    /// Profiles with rows.
    pub(crate) fn accounts(&self) -> rusqlite::Result<Vec<u32>> {
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT account FROM transcripts ORDER BY account")?;
        let accounts = statement.query_map([], |row| row.get(0))?;
        accounts.collect()
    }

    /// Messages of profile with rows.
    pub(crate) fn messages(&self, account: u32) -> rusqlite::Result<Vec<u32>> {
        let mut statement = self
            .connection
            .prepare("SELECT msg FROM transcripts WHERE account = ?1 ORDER BY msg")?;
        let messages = statement.query_map([account], |row| row.get(0))?;
        messages.collect()
    }

    /// Delete rows of these messages.
    pub(crate) fn delete_messages(&self, account: u32, msgs: &[u32]) -> rusqlite::Result<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut gone = 0_usize;
        {
            let mut statement =
                transaction.prepare("DELETE FROM transcripts WHERE account = ?1 AND msg = ?2")?;
            for msg in msgs {
                gone = gone.saturating_add(statement.execute(params![account, msg])?);
            }
        }
        transaction.commit()?;
        Ok(gone)
    }

    /// Those of `msgs` with a row.
    pub(crate) fn known(&self, account: u32, msgs: &[u32]) -> rusqlite::Result<BTreeSet<u32>> {
        let mut statement = self
            .connection
            .prepare_cached("SELECT 1 FROM transcripts WHERE account = ?1 AND msg = ?2")?;
        let mut known = BTreeSet::new();
        for msg in msgs {
            if statement.exists(params![account, msg])? {
                known.insert(*msg);
            }
        }
        Ok(known)
    }

    /// Done transcripts among `msgs`, for previews.
    pub(crate) fn texts(
        &self,
        account: u32,
        msgs: &[u32],
    ) -> rusqlite::Result<BTreeMap<u32, String>> {
        let mut statement = self.connection.prepare_cached(
            "SELECT text FROM transcripts WHERE account = ?1 AND msg = ?2 AND state = ?3 AND text != ''",
        )?;
        let mut texts = BTreeMap::new();
        for msg in msgs {
            if let Some(text) = statement
                .query_row(params![account, msg, DONE], |row| row.get(0))
                .optional()?
            {
                texts.insert(*msg, text);
            }
        }
        Ok(texts)
    }
}
