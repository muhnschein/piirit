//! Writing a profile out to a backup file.
//!
//! The other half of taking a profile over from one (`signup.rs`), and the
//! core's own export: `export_backup` puts one account's messages, its
//! contacts and its key into a single `.tar`, which is exactly what
//! `import_backup` reads back. It is per profile, not per phone -- the
//! core takes an account id and writes that account -- so this is a small
//! object a profile's page owns rather than anything on the core object.
//!
//! Two things the core does not do for the caller, and both are here.
//!
//! The core names the file itself and answers with nothing, so the name is
//! found by looking at the folder before and after. The event that carries
//! it (`ImexFileWritten`) is no use for that: events are polled on a call
//! of their own (`deltachat-jsonrpc`'s event loop), so one emitted before
//! the export answers can still arrive after it, and a page that waited
//! for it would sometimes wait for ever. What is in the folder is not a
//! race.
//!
//! And the export reports itself in `ImexProgress` events, the same ones
//! an import reports itself in, with nothing in them to say which. The
//! page feeds the core's events in (`handle_event`), as the chat models
//! do, so the progress this object shows is the progress of the write it
//! started and nothing else.

use std::collections::BTreeSet;
use std::path::PathBuf;

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::core::connection;
use crate::json;

/// One profile's backup, written to a folder on the phone.
///
/// ```qml
/// Backup { id: backup; account_id: page.accountId }
/// Button { onClicked: backup.write(StandardPaths.documents) }
/// ```
#[derive(QObject, Default)]
pub struct Backup {
    base: qt_base_class!(trait QObject),

    /// Which profile is written. The core's export takes an account, so
    /// this is the whole of what "which backup" means. Set once, by the
    /// page that owns this object, from the profile it was opened for.
    pub account_id: qt_property!(u32),

    /// True while the core is writing.
    pub running: qt_property!(bool; NOTIFY running_changed),
    /// Emitted when [`Self::running`] changes.
    pub running_changed: qt_signal!(),

    /// How far the write has got, in permille, as the core counts it.
    pub permille: qt_property!(u32; NOTIFY permille_changed),
    /// Emitted when [`Self::permille`] changes.
    pub permille_changed: qt_signal!(),

    /// Write this profile into `folder`, which is made if it is not
    /// there. Answers on `written` or `error`.
    pub write: qt_method!(fn(&mut self, folder: QString)),

    /// Stop a write that is running. The core's refusal of the stopped
    /// export is then the reader's own doing, and is not reported.
    pub cancel: qt_method!(fn(&mut self)),

    /// Feed a `core_event` in. Events for other accounts, and every kind
    /// but the progress of this write, are ignored -- so a page can
    /// connect this without filtering first.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// The backup is written. `path` is the file when it could be named
    /// and the folder it went into when it could not; either way it is
    /// where the reader has to look.
    pub written: qt_signal!(path: QString),

    /// Nothing was written, in the core's own words.
    pub error: qt_signal!(message: QString),

    /// The reader asked for the write to stop, so the core refusing it is
    /// not a failure to report.
    cancelled: bool,
}

impl Backup {
    /// Write this profile's backup into `folder`.
    pub fn write(&mut self, folder: QString) {
        // A second write while one is running would be refused by the
        // core -- one ongoing process per account -- and the refusal
        // would land on the page as a failure of the write it is already
        // showing.
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
                // A write the reader stopped ends in the core's refusal
                // of it, which is not news to them.
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

    /// Stop a write that is running.
    pub fn cancel(&mut self) {
        if !self.running {
            return;
        }
        // Set before the call goes out: the export answers with the
        // core's refusal, and by then this has to say who asked.
        self.cancelled = true;
        let account_id = self.account_id;
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        runtime.spawn(async move {
            // Fire and forget: the write's own answer is what clears the
            // page, and it comes either way.
            let _ = rpc
                .call::<_, ()>("stop_ongoing_process", (account_id,))
                .await;
        });
    }

    /// Take in one core event: the progress of the write, or nothing.
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
        // The core reports a failure as progress 0. The call answers with
        // the reason a moment later, in words; a bar that drops back to
        // nothing first says less than that and looks like a restart.
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

/// Run the core's export into `folder` and answer with what it wrote.
///
/// The file is found by difference: what `.tar` files the folder held
/// before the export, and what it holds after. One new file is the
/// backup. Anything else -- none, because the core wrote somewhere this
/// cannot read back, or several, because something else was writing into
/// the same folder -- answers with the folder, which is still where the
/// reader has to go looking.
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

/// Run one piece of blocking filesystem work somewhere it can block.
///
/// `std::fs` stops the thread it is called on, and the threads this
/// future runs on are the runtime's: the same ones carrying the core's
/// connection and draining its events. `spawn_blocking` has a pool kept
/// for exactly this. A folder with a thousand files in it is not a long
/// wait, but it is not the core's wait to take.
async fn off_the_runtime<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        // The pool dropped the work, which on this runtime means it is
        // shutting down: the app is on its way out.
        Err(err) => Err(err.to_string()),
    }
}

/// Make the folder if it is not there, and answer with the `.tar` files
/// already in it.
///
/// The core makes the folder itself, but only once it has got that far:
/// a folder that cannot be made is worth saying before a minute of
/// writing rather than after it. A plain function, like `tars_in` below
/// and for the same reason: what blocks belongs where it can be handed
/// to `off_the_runtime` whole.
fn prepare(folder: &str) -> Result<BTreeSet<PathBuf>, String> {
    std::fs::create_dir_all(folder).map_err(|err| format!("cannot use {folder}: {err}"))?;
    Ok(tars_in(folder))
}

/// The `.tar` files in a folder, or nothing at all for a folder that
/// cannot be read.
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
