//! Write profile to backup file.
//!
//! Wraps core `export_backup`; other half of `signup.rs` takeover. Per profile, owned by its page.
//!
//! Core names file, answers nothing: name found by folder diff (before vs after).
//! `ImexFileWritten` unusable: events polled on own call (`deltachat-jsonrpc` event loop),
//! so one emitted before export answers can arrive after; page waiting on it may hang.
//! Folder is no race.
//!
//! Progress via `ImexProgress`, shared with import, nothing says which. Page feeds core events
//! (`handle_event`, like chat models); shown progress is only this object's write.

use std::collections::BTreeSet;
use std::path::PathBuf;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;

/// Profile backup, written to a folder.
///
/// ```qml
/// Backup { id: backup; account_id: page.accountId }
/// Button { onClicked: backup.write(StandardPaths.documents) }
/// ```
#[derive(QObject, Default)]
pub struct Backup {
    base: qt_base_class!(trait QObject),

    /// Account written. Set once by owning page.
    pub account_id: qt_property!(u32),

    /// True while the core is writing.
    pub running: qt_property!(bool; NOTIFY running_changed),
    /// Emitted when [`Self::running`] changes.
    pub running_changed: qt_signal!(),

    /// Write progress, permille.
    pub permille: qt_property!(u32; NOTIFY permille_changed),
    /// Emitted when [`Self::permille`] changes.
    pub permille_changed: qt_signal!(),

    /// Write into `folder`, made if absent. Answers `written` or `error`.
    pub write: qt_method!(fn(&mut self, folder: QString)),

    /// Stop write. Core's refusal not reported.
    pub cancel: qt_method!(fn(&mut self)),

    /// Feed `core_event`. Other accounts and non-progress kinds ignored.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Written to `path`, or folder if file unnamed.
    pub written: qt_signal!(path: QString),

    /// Nothing written; core's words.
    pub error: qt_signal!(message: QString),

    /// Reader stopped; core's refusal not a failure.
    cancelled: bool,
}

impl Backup {
    /// Write backup into `folder`.
    pub fn write(&mut self, folder: QString) {
        // Core refuses a second (one ongoing process per account);
        // refusal would show as the running write failing.
        if self.running {
            return;
        }
        let account_id = self.account_id;
        let folder = folder.to_string();
        if account_id == 0 || folder.is_empty() {
            self.error(QString::from("no profile to back up"));
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        self.cancelled = false;
        self.set_permille(0);
        self.set_running(true);

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            this.borrow_mut().set_running(false);
            let cancelled = this.borrow().cancelled;
            match result {
                Ok(path) => this.borrow().written(path.into()),
                // Stopped write's refusal is not news.
                Err(err) => {
                    if !cancelled {
                        this.borrow().error(err.into());
                    }
                }
            }
        });

        runtime.spawn(async move {
            done(export(&rpc, account_id, &folder).await);
        });
    }

    /// Stop running write.
    pub fn cancel(&mut self) {
        if !self.running {
            return;
        }
        // Set before call: refusal answer must know who asked.
        self.cancelled = true;
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            // Fire and forget; write's answer clears page.
            rpc.call::<_, ()>("stop_ongoing_process", (account_id,))
                .await
                .ok();
        });
    }

    /// Take one core event: write progress, or nothing.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, payload_json: QString) {
        if !self.running || self.account_id == 0 || context_id != self.account_id {
            return;
        }
        if kind.to_string() != "ImexProgress" {
            return;
        }
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json.to_string()).unwrap_or_default();
        let Some(permille) = json::u32_opt(&payload, "progress") else {
            return;
        };
        // 0 is failure; reason arrives in call answer. Reset would look like restart.
        if permille > 0 {
            self.set_permille(permille);
        }
    }

    /// Set [`Self::running`] and announce it.
    fn set_running(&mut self, running: bool) {
        if self.running != running {
            self.running = running;
            self.running_changed();
        }
    }

    /// Set [`Self::permille`] and announce it.
    fn set_permille(&mut self, permille: u32) {
        if self.permille != permille {
            self.permille = permille;
            self.permille_changed();
        }
    }
}

/// Run core export into `folder`; answer with what it wrote.
///
/// One new `.tar` is the backup. None or several (other writers): answer folder.
async fn export(rpc: &RpcClient, account_id: u32, folder: &str) -> Result<String, String> {
    let path = folder.to_string();
    let before = off_the_runtime(move || prepare(&path)).await?;
    rpc.call::<_, ()>(
        "export_backup",
        (account_id, folder, Option::<String>::None),
    )
    .await
    .map_err(|err| err.to_string())?;
    let path = folder.to_string();
    let mut written = off_the_runtime(move || Ok(tars_in(&path))).await?;
    for existing in &before {
        written.remove(existing);
    }
    let mut written = written.into_iter();
    match (written.next(), written.next()) {
        (Some(one), None) => Ok(one.to_string_lossy().into_owned()),
        _ => Ok(folder.to_string()),
    }
}

/// Run blocking fs work off runtime threads.
///
/// `std::fs` blocks thread; runtime threads carry core's connection and drain its events.
/// `spawn_blocking` pool is for this.
async fn off_the_runtime<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        // Pool dropped work: runtime shutting down.
        Err(err) => Err(err.to_string()),
    }
}

/// Make folder if absent; answer existing `.tar`s.
///
/// Core makes folder too, but only once it gets there: fail now, not after a minute of writing.
/// Plain fn (like `tars_in`) so blocking work goes to `off_the_runtime` whole.
fn prepare(folder: &str) -> Result<BTreeSet<PathBuf>, String> {
    std::fs::create_dir_all(folder).map_err(|err| format!("cannot use {folder}: {err}"))?;
    Ok(tars_in(folder))
}

/// `.tar` files in folder; none if unreadable.
fn tars_in(folder: &str) -> BTreeSet<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return BTreeSet::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "tar"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::tars_in;

    #[test]
    fn a_folder_that_is_not_there_holds_no_backups() {
        assert!(tars_in("/nowhere/piirit/does/not/exist").is_empty());
    }

    #[test]
    fn only_the_tar_files_count() {
        let folder = std::env::temp_dir().join(format!("piirit-tars-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("create the folder");
        for name in ["one.tar", "two.tar", "notes.txt", "picture.jpg"] {
            std::fs::write(folder.join(name), b"x").expect("write a file");
        }
        let found = tars_in(&folder.to_string_lossy());
        assert_eq!(found.len(), 2, "found {found:?}");
        assert!(found.contains(&folder.join("one.tar")));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
