//! Speech thread: owns [`Engine`], runs one job at a time. Never Qt thread or core's runtime:
//! a run takes cores for minutes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use piirit_speech::{Engine, Error, Options, Outcome, Progress};

/// Longest audio decoded: 30 min of 16 kHz `f32` = 115 MB.
const MAX_SECONDS: u32 = 30 * 60;

/// Idle this long: engine unloaded (frees model's memory).
const IDLE: Duration = Duration::from_secs(60);

/// Message as `(account, msg)`.
pub(crate) type Key = (u32, u32);

/// One run.
pub(crate) struct Order {
    pub key: Key,
    /// Voice file.
    pub path: PathBuf,
    pub options: Options,
    /// Model file.
    pub model: PathBuf,
    /// Engine libraries.
    pub libs: PathBuf,
    /// Set: run stops at next progress call.
    pub cancel: Arc<AtomicBool>,
}

/// What thread reports.
pub(crate) enum Report {
    /// Running: 0..=1000, text so far.
    Progress {
        key: Key,
        permille: u32,
        text: String,
    },
    /// Ended.
    Done {
        key: Key,
        result: Result<Outcome, Error>,
    },
}

enum Command {
    Run(Order),
    Unload,
}

/// Handle; dropping it ends thread once current run ends.
pub(crate) struct Worker {
    commands: mpsc::Sender<Command>,
}

impl Worker {
    /// Start thread. `report` called on it, in order.
    pub(crate) fn start(report: impl Fn(Report) + Send + 'static) -> Result<Self, String> {
        let (commands, receive) = mpsc::channel();
        std::thread::Builder::new()
            .name("piirit-speech".to_owned())
            .spawn(move || serve(&receive, &report))
            .map_err(|err| format!("could not start the speech thread: {err}"))?;
        Ok(Self { commands })
    }

    /// Queue run; false if thread gone.
    pub(crate) fn run(&self, order: Order) -> bool {
        self.commands.send(Command::Run(order)).is_ok()
    }

    /// Free engine now (feature off).
    pub(crate) fn unload(&self) {
        // Thread gone: nothing loaded.
        self.commands.send(Command::Unload).ok();
    }
}

/// Thread body. Blocks on channel; with engine loaded, at most [`IDLE`].
fn serve(commands: &mpsc::Receiver<Command>, report: &dyn Fn(Report)) {
    let mut engine: Option<Engine> = None;
    loop {
        let command = if engine.is_some() {
            match commands.recv_timeout(IDLE) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    engine = None;
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match commands.recv() {
                Ok(command) => command,
                Err(_) => return,
            }
        };
        match command {
            Command::Unload => engine = None,
            Command::Run(order) => {
                let result = run(&mut engine, &order, report);
                report(Report::Done {
                    key: order.key,
                    result,
                });
            }
        }
    }
}

/// Decode, load engine if needed, transcribe.
fn run(
    engine: &mut Option<Engine>,
    order: &Order,
    report: &dyn Fn(Report),
) -> Result<Outcome, Error> {
    if order.cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let samples = piirit_speech::decode(&order.path, MAX_SECONDS)?;
    if order.cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let loaded = match engine.take() {
        Some(loaded) => loaded,
        None => Engine::load(&order.libs, &order.model)?,
    };
    let engine = engine.insert(loaded);
    let mut last = (u32::MAX, String::new());
    let mut progress = |now: &Progress| {
        let permille = u32::from(now.permille.min(1000));
        // Only changes cross threads.
        if permille != last.0 || now.text != last.1 {
            last = (permille, now.text.clone());
            report(Report::Progress {
                key: order.key,
                permille,
                text: now.text.clone(),
            });
        }
        !order.cancel.load(Ordering::Relaxed)
    };
    engine.transcribe(&samples, &order.options, &mut progress)
}
