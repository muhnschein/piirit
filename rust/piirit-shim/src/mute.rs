//! Chat mute: how long, and when it ends. Core keeps the end, tells no client: app keeps
//! choice beside it, under `ui.` key of the profile. Muted on other device or before this
//! app kept it: length and end unknown (0).

use deltachat_jsonrpc::RpcClient;
use qmetaobject::*;

use crate::chatlist::mute_duration;
use crate::core::connection;
use crate::location::unix_now;

/// Where app keeps chat's mute: `"LENGTH:END"` (seconds, Unix time; `-1:0` forever).
pub(crate) fn mute_key(chat_id: u32) -> String {
    format!("ui.piirit.mute.{chat_id}")
}

/// Kept value for mute of `seconds` starting `now`; None for unmute.
pub(crate) fn kept_value(seconds: i32, now: u64) -> Option<String> {
    match seconds {
        0 => None,
        forever if forever < 0 => Some("-1:0".to_string()),
        _ => Some(format!(
            "{seconds}:{}",
            now.saturating_add(u64::from(seconds.unsigned_abs()))
        )),
    }
}

/// Length and end from kept value, while core says muted: `(-1, 0)` forever, `(0, 0)` not
/// known (nothing kept, end passed: kept one is an earlier mute's).
pub(crate) fn read_kept(kept: Option<&str>, muted: bool, now: u64) -> (i32, u64) {
    let Some((length, until)) = kept.and_then(|kept| kept.split_once(':')) else {
        return (0, 0);
    };
    let (Ok(length), Ok(until)) = (length.parse::<i32>(), until.parse::<u64>()) else {
        return (0, 0);
    };
    match length {
        _ if !muted => (0, 0),
        -1 => (-1, 0),
        length if length > 0 && until > now => (length, until),
        _ => (0, 0),
    }
}

/// Keep choice, then mute: core's `ChatModified` then finds it kept.
pub(crate) async fn apply(
    rpc: &RpcClient,
    account_id: u32,
    chat_id: u32,
    seconds: i32,
) -> Result<(), String> {
    rpc.call::<_, ()>(
        "set_config",
        (
            account_id,
            mute_key(chat_id),
            kept_value(seconds, unix_now()),
        ),
    )
    .await
    .map_err(|err| err.to_string())?;
    rpc.call::<_, ()>(
        "set_chat_mute_duration",
        (account_id, chat_id, mute_duration(seconds)),
    )
    .await
    .map_err(|err| err.to_string())
}

/// Chat's mute as app kept it.
///
/// ```qml
/// MuteState { account_id: 1; chat_id: 7; muted: chat.is_muted }
/// Label { text: Mute.rowValue(state.muted, state.length, state.until) }
/// ```
#[derive(QObject, Default)]
pub struct MuteState {
    base: qt_base_class!(trait QObject),

    /// Account. Set reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Chat. Set reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY source_changed),
    /// Core's word: muted now. Set reloads.
    pub muted: qt_property!(bool; WRITE set_muted NOTIFY source_changed),
    /// Account, chat or `muted` changed.
    pub source_changed: qt_signal!(),

    /// Seconds chosen; -1 forever, 0 not muted or not known.
    pub length: qt_property!(i32; NOTIFY loaded_changed),
    /// End, Unix seconds; 0 forever or not known.
    pub until: qt_property!(f64; NOTIFY loaded_changed),
    /// Answer in.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Emitted per answer.
    pub loaded_changed: qt_signal!(),

    /// Read again (other choice saved elsewhere).
    pub reload: qt_method!(fn(&mut self)),
    /// Now, Unix seconds: time left counted from it.
    pub now: qt_method!(fn(&self) -> f64),

    /// Stale-answer guard.
    generation: u64,
}

impl MuteState {
    /// Set account; reload.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set chat; reload.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set core's word; reload.
    pub fn set_muted(&mut self, muted: bool) {
        if self.muted != muted {
            self.muted = muted;
            self.source_changed();
            self.reload();
        }
    }

    /// Clock, for QML.
    #[must_use]
    pub fn now(&self) -> f64 {
        // Seconds since 1970 fit f64 exactly.
        #[allow(clippy::cast_precision_loss)]
        let now = unix_now() as f64;
        now
    }

    /// Ask core for kept value.
    pub fn reload(&mut self) {
        let (account_id, chat_id, muted) = (self.account_id, self.chat_id, self.muted);
        if account_id == 0 || chat_id == 0 {
            return;
        }
        // Not muted: nothing to read.
        if !muted {
            self.take(None);
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |kept: Option<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            this.borrow_mut().take(kept.as_deref());
        });
        runtime.spawn(async move {
            let kept = rpc
                .call::<_, Option<String>>("get_config", (account_id, mute_key(chat_id)))
                .await
                .ok()
                .flatten();
            done(kept);
        });
    }

    fn take(&mut self, kept: Option<&str>) {
        let (length, until) = read_kept(kept, self.muted, unix_now());
        self.length = length;
        #[allow(clippy::cast_precision_loss)]
        let until = until as f64;
        self.until = until;
        self.loaded = true;
        self.loaded_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::{kept_value, read_kept};

    #[test]
    fn kept_value_holds_length_and_end() {
        assert_eq!(kept_value(0, 1000), None);
        assert_eq!(kept_value(-1, 1000).as_deref(), Some("-1:0"));
        assert_eq!(kept_value(3600, 1000).as_deref(), Some("3600:4600"));
    }

    #[test]
    fn kept_mute_read_only_while_core_says_muted_and_end_ahead() {
        assert_eq!(read_kept(Some("28800:5000"), true, 1000), (28800, 5000));
        assert_eq!(read_kept(Some("-1:0"), true, 1000), (-1, 0));
        // Unmuted elsewhere: kept value stale.
        assert_eq!(read_kept(Some("28800:5000"), false, 1000), (0, 0));
        // End passed: earlier mute's; core's mute is another one.
        assert_eq!(read_kept(Some("3600:900"), true, 1000), (0, 0));
        // Nothing kept, or not ours.
        assert_eq!(read_kept(None, true, 1000), (0, 0));
        assert_eq!(read_kept(Some("soon"), true, 1000), (0, 0));
        assert_eq!(read_kept(Some("0:5000"), true, 1000), (0, 0));
    }
}
