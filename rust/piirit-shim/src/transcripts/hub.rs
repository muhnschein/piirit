//! Qt-thread side: queue, running job, live `Transcript`s, store thread. One per process.
//!
//! Hub borrowed only to read or change data, never while a `QObject` is touched: a view's
//! handler may call straight back in. Views change from queued callbacks only (fresh stack).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use deltachat_jsonrpc::RpcClient;
use piirit_speech::{Error, Options, Outcome};
use qmetaobject::{queued_callback, QPointer};
use serde_json::Value;

use super::files;
use super::schedule::{self, Conditions, Entry, Kind};
use super::store::{Row, Store};
use super::worker::{Key, Order, Report, Worker};
use super::{
    Transcriber, Transcript, STATE_DONE, STATE_FAILED, STATE_NO_SPEECH, STATE_OTHER_LANGUAGE,
    STATE_QUEUED, STATE_RUNNING,
};
use crate::core::connection;
use crate::json;

/// Ids per `get_messages` / `get_existing_msg_ids` call.
const PAGE: usize = 50;
const SWEEP_PAGE: usize = 500;

/// `DC_CONTACT_ID_SELF`: own messages never queue by themselves.
const SELF: u32 = 1;

/// What a `Transcript` shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Shown {
    pub state: u32,
    pub manual: bool,
    pub permille: u32,
    pub text: String,
    pub language: String,
    pub error: String,
}

impl Shown {
    fn of_row(row: Row) -> Self {
        Self {
            state: row.state,
            text: row.text,
            language: row.language,
            error: row.error,
            ..Self::default()
        }
    }
}

/// Downloaded voice message, as core says.
#[derive(Clone, Debug)]
struct Found {
    key: Key,
    chat: u32,
    timestamp: i64,
    path: PathBuf,
    from_self: bool,
}

/// Core message JSON: `Some` if downloaded voice message with file.
fn voice_of(account: u32, msg: u32, message: &Value) -> Option<Found> {
    let file = json::str_at(message, "file");
    let downloaded = matches!(json::str_at(message, "downloadState"), "" | "Done");
    if json::str_at(message, "kind") == "loadingError"
        || json::str_at(message, "viewType") != "Voice"
        || file.is_empty()
        || !downloaded
    {
        return None;
    }
    Some(Found {
        key: (account, msg),
        chat: json::u32_at(message, "chatId"),
        timestamp: json::i64_at(message, "timestamp"),
        path: PathBuf::from(file),
        from_self: json::u32_at(message, "fromId") == SELF,
    })
}

/// Never becomes voice message: downloaded, other kind.
fn never_voice(message: &Value) -> bool {
    matches!(json::str_at(message, "downloadState"), "" | "Done")
        && json::str_at(message, "viewType") != "Voice"
}

struct Job {
    key: Key,
    chat: u32,
    timestamp: i64,
    path: PathBuf,
    kind: Kind,
    seq: u64,
    any_language: bool,
}

impl Job {
    fn entry(&self) -> Entry {
        Entry {
            kind: self.kind,
            seq: self.seq,
            timestamp: self.timestamp,
        }
    }
}

struct Running {
    job: Job,
    permille: u32,
    text: String,
    cancel: Arc<AtomicBool>,
    /// Stopped for charger or battery saving: back in queue when it ends.
    requeue: bool,
}

type Task = Box<dyn FnOnce(&Store) + Send>;

/// Store's own thread: `SQLite` blocks on disk, never on Qt thread or core's runtime.
#[derive(Clone)]
struct StoreThread(mpsc::Sender<Task>);

impl StoreThread {
    fn start() -> Option<Self> {
        let (tasks, receive) = mpsc::channel::<Task>();
        let started = std::thread::Builder::new()
            .name("piirit-transcripts".to_owned())
            .spawn(move || {
                let Some(store) = open_store() else { return };
                while let Ok(task) = receive.recv() {
                    task(&store);
                }
            });
        if let Err(err) = started {
            eprintln!("piirit: could not start the transcript store: {err}");
            return None;
        }
        Some(Self(tasks))
    }

    fn send(&self, task: impl FnOnce(&Store) + Send + 'static) {
        // Thread gone: store unusable, logged at open.
        self.0.send(Box::new(task)).ok();
    }

    /// For async code; `None` if store unusable.
    async fn ask<R: Send + 'static>(
        &self,
        task: impl FnOnce(&Store) -> R + Send + 'static,
    ) -> Option<R> {
        let (answer, answered) = tokio::sync::oneshot::channel();
        self.send(move |store| {
            answer.send(task(store)).ok();
        });
        answered.await.ok()
    }
}

/// File store; else memory, so session works.
fn open_store() -> Option<Store> {
    let opened = files::make_speech_dir()
        .map(|dir| dir.join("transcripts.sqlite"))
        .and_then(|path| Store::open(&path).map_err(|err| format!("transcripts.sqlite: {err}")));
    match opened {
        Ok(store) => {
            if !store.has_search() {
                eprintln!("piirit: this SQLite has no FTS5; transcripts are stored unindexed");
            }
            Some(store)
        }
        Err(err) => {
            eprintln!("piirit: transcripts kept in memory only, store unusable: {err}");
            Store::open_in_memory()
                .map_err(|err| eprintln!("piirit: no transcript store at all: {err}"))
                .ok()
        }
    }
}

#[derive(Default)]
struct Hub {
    transcriber: QPointer<Transcriber>,
    conditions: Conditions,
    languages: Vec<String>,
    queue: Vec<Job>,
    running: Option<Running>,
    seq: u64,
    live: BTreeMap<Key, Vec<QPointer<Transcript>>>,
    worker: Option<Worker>,
    store: Option<StoreThread>,
    swept: bool,
    scanning: bool,
    /// Messages seen this session needing no look again (row, or not voice).
    skip: BTreeSet<Key>,
}

impl Hub {
    fn is_live(&self, key: Key) -> bool {
        self.running.as_ref().is_some_and(|run| run.job.key == key)
            || self.queue.iter().any(|job| job.key == key)
    }

    /// Shown from queue state alone; `None`: row decides.
    fn live_shown(&self, key: Key) -> Option<Shown> {
        if !self.conditions.enabled {
            return Some(Shown::default());
        }
        if let Some(run) = self.running.as_ref().filter(|run| run.job.key == key) {
            return Some(Shown {
                state: STATE_RUNNING,
                manual: run.job.kind == Kind::Manual,
                permille: run.permille,
                text: run.text.clone(),
                ..Shown::default()
            });
        }
        self.queue
            .iter()
            .find(|job| job.key == key)
            .map(|job| Shown {
                state: STATE_QUEUED,
                manual: job.kind == Kind::Manual,
                ..Shown::default()
            })
    }

    fn views(&mut self, key: Key) -> Vec<QPointer<Transcript>> {
        let Some(views) = self.live.get_mut(&key) else {
            return Vec::new();
        };
        views.retain(|view| !view.is_null());
        let found = views.clone();
        if views.is_empty() {
            self.live.remove(&key);
        }
        found
    }

    /// Queue `found` as `kind`; tap on queued job moves it up. False: nothing changed.
    fn enqueue(&mut self, found: &Found, kind: Kind, any_language: bool) -> bool {
        if !schedule::keeps(kind, &self.conditions)
            || self
                .running
                .as_ref()
                .is_some_and(|run| run.job.key == found.key)
        {
            return false;
        }
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        if let Some(job) = self.queue.iter_mut().find(|job| job.key == found.key) {
            if kind != Kind::Manual {
                return false;
            }
            if job.kind != Kind::Manual {
                job.kind = Kind::Manual;
                job.seq = seq;
            }
            job.any_language |= any_language;
            return true;
        }
        self.queue.push(Job {
            key: found.key,
            chat: found.chat,
            timestamp: found.timestamp,
            path: found.path.clone(),
            kind,
            seq,
            any_language,
        });
        true
    }

    /// Drop key from queue, stop it if running.
    fn drop_key(&mut self, key: Key) {
        self.queue.retain(|job| job.key != key);
        if let Some(run) = self.running.as_mut().filter(|run| run.job.key == key) {
            run.requeue = false;
            run.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn store(&mut self) -> Option<StoreThread> {
        if self.store.is_none() {
            self.store = StoreThread::start();
        }
        self.store.clone()
    }

    /// Store, only if file there already: reads and deletes don't make one.
    fn existing_store(&mut self) -> Option<StoreThread> {
        if self.store.is_none() && files::store_path().is_some_and(|path| path.is_file()) {
            return self.store();
        }
        self.store.clone()
    }
}

thread_local! {
    static HUB: RefCell<Hub> = RefCell::new(Hub::default());
}

fn with<R>(task: impl FnOnce(&mut Hub) -> R) -> R {
    HUB.with(|hub| task(&mut hub.borrow_mut()))
}

/// App-wide object, for answers.
pub(crate) fn attach(transcriber: QPointer<Transcriber>) {
    with(|hub| hub.transcriber = transcriber);
}

/// Settings or power changed.
pub(crate) fn set_conditions(conditions: Conditions, languages: Vec<String>) {
    let (was, dropped) = with(|hub| {
        let was = std::mem::replace(&mut hub.conditions, conditions);
        hub.languages = languages;
        let mut dropped = Vec::new();
        hub.queue.retain(|job| {
            let keep = schedule::keeps(job.kind, &conditions);
            if !keep {
                dropped.push(job.key);
            }
            keep
        });
        if let Some(run) = hub.running.as_mut() {
            if !schedule::keeps(run.job.kind, &conditions) {
                run.requeue = false;
                run.cancel.store(true, Ordering::Relaxed);
            }
        }
        if was.enabled && !conditions.enabled {
            if let Some(worker) = &hub.worker {
                worker.unload();
            }
            hub.skip.clear();
        }
        (was, dropped)
    });
    if was.enabled == conditions.enabled {
        for key in dropped {
            refresh(key);
        }
    } else {
        refresh_all();
    }
    if schedule::queues_backlog(&conditions) && !schedule::queues_backlog(&was) {
        scan_backlog();
    }
    pump();
}

/// View `view` moved from `old` to `new`; shows `new` soon.
pub(crate) fn watch(view: &QPointer<Transcript>, old: Key, new: Key) {
    let me = view.cpp_ptr();
    with(|hub| {
        if let Some(views) = hub.live.get_mut(&old) {
            views.retain(|other| other.cpp_ptr() != me);
        }
        if new.0 != 0 && new.1 != 0 {
            hub.live.entry(new).or_default().push(view.clone());
        }
    });
    refresh(new);
}

/// Show `key`'s state in its views, from queue or store. Deferred: never on caller's stack.
pub(crate) fn refresh(key: Key) {
    if key.0 == 0 || key.1 == 0 {
        return;
    }
    let reply = queued_callback(move |row: Option<Row>| apply(key, row));
    let store = with(|hub| {
        if hub.conditions.enabled && hub.live_shown(key).is_none() {
            hub.existing_store()
        } else {
            None
        }
    });
    match store {
        Some(store) => store.send(move |store| {
            reply(store.get(key.0, key.1).ok().flatten());
        }),
        None => reply(None),
    }
}

fn refresh_all() {
    let keys: Vec<Key> = with(|hub| hub.live.keys().copied().collect());
    for key in keys {
        refresh(key);
    }
}

/// Views of `key` show queue state, else `row`.
fn apply(key: Key, row: Option<Row>) {
    let (shown, views) = with(|hub| {
        let shown = hub
            .live_shown(key)
            .unwrap_or_else(|| row.map(Shown::of_row).unwrap_or_default());
        (shown, hub.views(key))
    });
    for view in views {
        if let Some(view) = view.as_pinned() {
            view.borrow_mut().show(&shown);
        }
    }
}

/// Live state only (progress); from queued callback.
fn apply_live(key: Key) {
    let (shown, views) = with(|hub| (hub.live_shown(key), hub.views(key)));
    let Some(shown) = shown else { return };
    for view in views {
        if let Some(view) = view.as_pinned() {
            view.borrow_mut().show(&shown);
        }
    }
}

/// Start next job if none runs; stop running automatic one power no longer allows.
pub(crate) fn pump() {
    let job = with(|hub| {
        if let Some(run) = hub.running.as_mut() {
            if !run.requeue && !schedule::may_run(run.job.kind, &hub.conditions) {
                run.requeue = true;
                run.cancel.store(true, Ordering::Relaxed);
            }
            return None;
        }
        let entries: Vec<Entry> = hub.queue.iter().map(Job::entry).collect();
        let index = schedule::next(&entries, &hub.conditions)?;
        (index < hub.queue.len()).then(|| hub.queue.remove(index))
    });
    let Some(job) = job else { return };
    let key = job.key;
    let Some(model) = files::model_path() else {
        record(&job, Err(Error::Model("no place for the model".to_owned())));
        return;
    };
    let needs_worker = with(|hub| hub.worker.is_none());
    if needs_worker {
        let report = queued_callback(on_report);
        match Worker::start(report) {
            Ok(worker) => with(|hub| hub.worker = Some(worker)),
            Err(err) => {
                record(&job, Err(Error::Unavailable(err)));
                return;
            }
        }
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let sent = with(|hub| {
        let options = Options {
            languages: if job.any_language {
                Vec::new()
            } else {
                hub.languages.clone()
            },
            cpus: piirit_speech::big_cores(),
            background: job.kind != Kind::Manual,
        };
        let order = Order {
            key,
            path: job.path.clone(),
            options,
            model,
            libs: files::libs_dir(),
            cancel: Arc::clone(&cancel),
        };
        let sent = hub.worker.as_ref().is_some_and(|worker| worker.run(order));
        if sent {
            hub.running = Some(Running {
                job,
                permille: 0,
                text: String::new(),
                cancel,
                requeue: false,
            });
            None
        } else {
            hub.worker = None;
            Some(job)
        }
    });
    if let Some(job) = sent {
        record(
            &job,
            Err(Error::Unavailable("the speech thread ended".to_owned())),
        );
        return;
    }
    refresh(key);
}

/// From speech thread, on Qt thread.
fn on_report(report: Report) {
    match report {
        Report::Progress {
            key,
            permille,
            text,
        } => {
            with(|hub| {
                if let Some(run) = hub.running.as_mut().filter(|run| run.job.key == key) {
                    run.permille = permille;
                    run.text = text;
                }
            });
            apply_live(key);
        }
        Report::Done { key, result } => {
            let ended = with(|hub| {
                if hub.running.as_ref().is_some_and(|run| run.job.key == key) {
                    hub.running.take()
                } else {
                    None
                }
            });
            let Some(run) = ended else { return };
            match result {
                Err(Error::Cancelled) => {
                    if run.requeue {
                        with(|hub| hub.queue.push(run.job));
                    }
                    refresh(key);
                }
                result => record(&run.job, result),
            }
            pump();
        }
    }
}

/// Store outcome of `job`, then show it.
fn record(job: &Job, result: Result<Outcome, Error>) {
    let mut row = Row {
        account: job.key.0,
        msg: job.key.1,
        chat: job.chat,
        timestamp: job.timestamp,
        model: files::MODEL_FILE.to_owned(),
        updated: chrono::Utc::now().timestamp(),
        ..Row::default()
    };
    match result {
        Ok(Outcome::Text { language, text }) => {
            row.state = STATE_DONE;
            row.language = language;
            row.text = text;
        }
        Ok(Outcome::NoSpeech) => row.state = STATE_NO_SPEECH,
        Ok(Outcome::OtherLanguage { language }) => {
            row.state = STATE_OTHER_LANGUAGE;
            row.language = language;
        }
        Err(err) => {
            // Detail for log; bubble says only that it failed.
            eprintln!("piirit: transcribing message {}: {err}", job.key.1);
            row.state = STATE_FAILED;
            row.error = err.to_string();
        }
    }
    write(row);
}

fn write(row: Row) {
    let key = (row.account, row.msg);
    with(|hub| hub.skip.insert(key));
    let reply = queued_callback(move |row: Option<Row>| apply(key, row));
    match with(Hub::store) {
        Some(store) => store.send(move |store| {
            if let Err(err) = store.put(&row) {
                eprintln!("piirit: storing a transcript: {err}");
            }
            reply(Some(row));
        }),
        None => reply(Some(row)),
    }
}

/// Tap: queue ahead of automatic jobs.
pub(crate) fn transcribe(key: Key, any_language: bool) {
    if key.0 == 0 || key.1 == 0 {
        return;
    }
    let (enabled, moved, running) = with(|hub| {
        let moved = hub.queue.iter().any(|job| job.key == key)
            && hub.enqueue(
                &Found {
                    key,
                    chat: 0,
                    timestamp: 0,
                    path: PathBuf::new(),
                    from_self: false,
                },
                Kind::Manual,
                any_language,
            );
        let running = hub.running.as_ref().is_some_and(|run| run.job.key == key);
        (hub.conditions.enabled, moved, running)
    });
    if !enabled || running {
        return;
    }
    if moved {
        refresh(key);
        pump();
        return;
    }
    let Some((rpc, runtime)) = connection() else {
        eprintln!("piirit: cannot transcribe before the core runs");
        return;
    };
    let done = queued_callback(move |found: Result<Option<Found>, String>| match found {
        Ok(Some(found)) => {
            if with(|hub| hub.enqueue(&found, Kind::Manual, any_language)) {
                refresh(key);
                pump();
            }
        }
        Ok(None) => record(
            &tapped(key),
            Err(Error::Audio("not a downloaded voice message".to_owned())),
        ),
        Err(err) => record(&tapped(key), Err(Error::Failed(err))),
    });
    runtime.spawn(async move {
        let message = rpc
            .call::<_, Value>("get_message", key)
            .await
            .map_err(|err| err.to_string());
        done(message.map(|message| voice_of(key.0, key.1, &message)));
    });
}

/// Job stand-in for a tap that never got as far as the queue.
fn tapped(key: Key) -> Job {
    Job {
        key,
        chat: 0,
        timestamp: 0,
        path: PathBuf::new(),
        kind: Kind::Manual,
        seq: 0,
        any_language: false,
    }
}

/// Drop from queue or stop.
pub(crate) fn cancel(key: Key) {
    with(|hub| hub.drop_key(key));
    refresh(key);
}

/// Core event: message arrived or changed (limited download landing).
pub(crate) fn arrived(account: u32, msg: u32) {
    let key = (account, msg);
    let wanted = with(|hub| {
        schedule::queues_new(&hub.conditions) && !hub.skip.contains(&key) && !hub.is_live(key)
    });
    if !wanted {
        return;
    }
    let Some((rpc, runtime)) = connection() else {
        return;
    };
    let store = with(Hub::existing_store);
    // Some: voice to queue; None: look no more; not called: may change still.
    let done = queued_callback(move |found: Option<Found>| {
        let queued = with(|hub| match found {
            Some(found) if !found.from_self => hub.enqueue(&found, Kind::New, false),
            _ => {
                hub.skip.insert(key);
                false
            }
        });
        if queued {
            refresh(key);
            pump();
        }
    });
    runtime.spawn(async move {
        if let Some(store) = store {
            if store
                .ask(move |store| store.get(account, msg).ok().flatten().is_some())
                .await
                .unwrap_or(false)
            {
                done(None);
                return;
            }
        }
        let Ok(message) = rpc.call::<_, Value>("get_message", (account, msg)).await else {
            return;
        };
        if let Some(found) = voice_of(account, msg, &message) {
            done(Some(found));
        } else if never_voice(&message) {
            done(None);
        }
    });
}

/// Core event: message gone.
pub(crate) fn deleted(account: u32, msg: u32) {
    let key = (account, msg);
    let store = with(|hub| {
        hub.drop_key(key);
        hub.skip.remove(&key);
        hub.existing_store()
    });
    let reply = queued_callback(move |()| refresh(key));
    match store {
        Some(store) => store.send(move |store| {
            if let Err(err) = store.delete(account, msg) {
                eprintln!("piirit: deleting a transcript: {err}");
            }
            reply(());
        }),
        None => reply(()),
    }
}

/// Core event: profile list changed; rows of gone ones go.
pub(crate) fn accounts_changed() {
    let Some((rpc, runtime)) = connection() else {
        return;
    };
    let store = with(Hub::existing_store);
    let done = queued_callback(after_accounts);
    runtime.spawn(async move {
        if let Ok(accounts) = rpc.call_unit::<Vec<u32>>("get_all_account_ids").await {
            if let Some(store) = store {
                let kept = accounts.clone();
                if let Some(Err(err)) = store.ask(move |store| store.keep_accounts(&kept)).await {
                    eprintln!("piirit: dropping transcripts of profiles: {err}");
                }
            }
            done(accounts);
        }
    });
}

/// Jobs of gone profiles go too.
fn after_accounts(accounts: Vec<u32>) {
    let gone: Vec<Key> = with(|hub| {
        let gone: Vec<Key> = hub
            .queue
            .iter()
            .map(|job| job.key)
            .chain(hub.running.as_ref().map(|run| run.job.key))
            .filter(|key| !accounts.contains(&key.0))
            .collect();
        for key in &gone {
            hub.drop_key(*key);
        }
        gone
    });
    for key in gone {
        refresh(key);
    }
    refresh_all();
}

/// Core up (start or restart). First time: drop rows of messages gone meanwhile.
pub(crate) fn connected() {
    let first = with(|hub| !std::mem::replace(&mut hub.swept, true));
    if first {
        sweep();
    }
    if with(|hub| schedule::queues_backlog(&hub.conditions)) {
        scan_backlog();
    }
}

/// Rows whose profile or message no longer exists (auto-deleted while app was closed).
fn sweep() {
    let Some((rpc, runtime)) = connection() else {
        return;
    };
    let Some(store) = with(Hub::existing_store) else {
        return;
    };
    let done = queued_callback(after_accounts);
    runtime.spawn(async move {
        match swept(&rpc, &store).await {
            Ok(accounts) => done(accounts),
            Err(err) => eprintln!("piirit: checking transcripts against messages: {err}"),
        }
    });
}

async fn swept(rpc: &RpcClient, store: &StoreThread) -> Result<Vec<u32>, String> {
    let accounts: Vec<u32> = rpc
        .call_unit("get_all_account_ids")
        .await
        .map_err(|err| err.to_string())?;
    let kept = accounts.clone();
    store
        .ask(move |store| store.keep_accounts(&kept))
        .await
        .transpose()
        .map_err(|err| err.to_string())?;
    for &account in &accounts {
        let stored = store
            .ask(move |store| store.messages(account))
            .await
            .transpose()
            .map_err(|err| err.to_string())?
            .unwrap_or_default();
        for page in stored.chunks(SWEEP_PAGE) {
            let existing: BTreeSet<u32> = rpc
                .call::<_, Vec<u32>>("get_existing_msg_ids", (account, page))
                .await
                .map_err(|err| err.to_string())?
                .into_iter()
                .collect();
            let gone: Vec<u32> = page
                .iter()
                .copied()
                .filter(|msg| !existing.contains(msg))
                .collect();
            if !gone.is_empty() {
                store
                    .ask(move |store| store.delete_messages(account, &gone))
                    .await
                    .transpose()
                    .map_err(|err| err.to_string())?;
            }
        }
    }
    Ok(accounts)
}

/// Older voice messages of every profile without row, as backlog.
fn scan_backlog() {
    let Some((rpc, runtime)) = connection() else {
        return;
    };
    let Some(store) = with(|hub| {
        if std::mem::replace(&mut hub.scanning, true) {
            None
        } else {
            hub.store()
        }
    }) else {
        return;
    };
    let done = queued_callback(|found: Result<Vec<Found>, String>| {
        with(|hub| hub.scanning = false);
        match found {
            Ok(found) => queue_all(&found, Kind::Backlog),
            Err(err) => eprintln!("piirit: looking for older voice messages: {err}"),
        }
    });
    runtime.spawn(async move {
        done(find_voice(&rpc, Some(&store), None).await);
    });
}

fn queue_all(found: &[Found], kind: Kind) {
    let queued: Vec<Key> = with(|hub| {
        found
            .iter()
            .filter(|found| kind == Kind::Manual || !found.from_self)
            .filter(|found| hub.enqueue(found, kind, false))
            .map(|found| found.key)
            .collect()
    });
    for key in queued {
        refresh(key);
    }
    pump();
}

/// Voice messages without row: every profile, or one chat.
async fn find_voice(
    rpc: &RpcClient,
    store: Option<&StoreThread>,
    chat: Option<(u32, u32)>,
) -> Result<Vec<Found>, String> {
    let accounts: Vec<u32> = match chat {
        Some((account, _)) => vec![account],
        None => rpc
            .call_unit("get_all_account_ids")
            .await
            .map_err(|err| err.to_string())?,
    };
    let mut found = Vec::new();
    for account in accounts {
        let wanted = without_row(rpc, store, account, chat.map(|(_, chat)| chat)).await?;
        for page in wanted.chunks(PAGE) {
            let loaded: BTreeMap<u32, Value> = rpc
                .call("get_messages", (account, page))
                .await
                .map_err(|err| err.to_string())?;
            found.extend(
                page.iter()
                    .filter_map(|msg| voice_of(account, *msg, loaded.get(msg)?)),
            );
        }
    }
    Ok(found)
}

/// Voice message ids of profile (or chat) without row, newest first.
async fn without_row(
    rpc: &RpcClient,
    store: Option<&StoreThread>,
    account: u32,
    chat: Option<u32>,
) -> Result<Vec<u32>, String> {
    let ids: Vec<u32> = rpc
        .call(
            "get_chat_media",
            (account, chat, "Voice", None::<&str>, None::<&str>),
        )
        .await
        .map_err(|err| err.to_string())?;
    let known = match store {
        Some(store) => {
            let asked = ids.clone();
            store
                .ask(move |store| store.known(account, &asked))
                .await
                .transpose()
                .map_err(|err| err.to_string())?
                .unwrap_or_default()
        }
        None => BTreeSet::new(),
    };
    Ok(ids
        .into_iter()
        .rev()
        .filter(|msg| !known.contains(msg))
        .collect())
}

/// Chat's voice messages without transcript, not queued; answer on `Transcriber`. False: no
/// core.
pub(crate) fn count_chat(account: u32, chat: u32) -> bool {
    let Some((rpc, runtime)) = connection() else {
        return false;
    };
    let store = with(Hub::existing_store);
    let done = queued_callback(move |ids: Vec<u32>| {
        let (count, transcriber) = with(|hub| {
            let count = ids
                .iter()
                .filter(|msg| !hub.is_live((account, **msg)))
                .count();
            (count, hub.transcriber.clone())
        });
        if let Some(transcriber) = transcriber.as_pinned() {
            transcriber.borrow().chat_counted(
                account,
                chat,
                u32::try_from(count).unwrap_or(u32::MAX),
            );
        }
    });
    runtime.spawn(async move {
        match without_row(&rpc, store.as_ref(), account, Some(chat)).await {
            Ok(ids) => done(ids),
            Err(err) => {
                eprintln!("piirit: counting voice messages: {err}");
                done(Vec::new());
            }
        }
    });
    true
}

/// Queue chat's voice messages without transcript, as taps.
pub(crate) fn transcribe_chat(account: u32, chat: u32) {
    if !with(|hub| hub.conditions.enabled) {
        return;
    }
    let Some((rpc, runtime)) = connection() else {
        return;
    };
    let store = with(Hub::existing_store);
    let done = queued_callback(|found: Result<Vec<Found>, String>| match found {
        Ok(found) => queue_all(&found, Kind::Manual),
        Err(err) => eprintln!("piirit: queueing a chat's voice messages: {err}"),
    });
    runtime.spawn(async move {
        done(find_voice(&rpc, store.as_ref(), Some((account, chat))).await);
    });
}
