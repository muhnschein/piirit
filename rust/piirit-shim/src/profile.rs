//! Account's own profile: name, status, picture, via core `get_config`/`set_config`.

use qmetaobject::*;

use crate::connectivity::{quota_from_report, Quota};
use crate::core::connection;
use crate::json;
use crate::media;

/// One account's profile, loaded and saved.
///
/// ```qml
/// Profile { id: profile; account_id: page.accountId }
/// TextField { text: profile.display_name }
/// ```
#[derive(QObject, Default)]
pub struct Profile {
    base: qt_base_class!(trait QObject),

    /// Whose profile; set reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Account changed.
    pub account_changed: qt_signal!(),

    /// Name others see (`displayname`).
    pub display_name: qt_property!(QString; NOTIFY loaded_changed),
    /// Status line in outgoing mail (`selfstatus`).
    pub status: qt_property!(QString; NOTIFY loaded_changed),
    /// Picture path in core's blob dir; empty if none.
    pub avatar_path: qt_property!(QString; NOTIFY loaded_changed),
    /// Address (`configured_addr`). Read-only: change means new transport.
    pub address: qt_property!(QString; NOTIFY loaded_changed),
    /// Reader's own contact colour, `#rrggbb`.
    pub color: qt_property!(QString; NOTIFY loaded_changed),

    /// Read receipts (`mdns_enabled`; default on).
    pub read_receipts: qt_property!(bool; NOTIFY loaded_changed),
    /// Attachment limit, bytes; 0 until read. See `media.rs`. f64 for QML.
    pub attachment_limit_bytes: qt_property!(f64; NOTIFY loaded_changed),

    /// `get_connectivity` band: 1000 offline, 2000 connecting, 3000 working, 4000 idle; 0 until
    /// asked. See `connectivity.rs`.
    pub connectivity: qt_property!(u32; NOTIFY connectivity_changed),
    /// Mailbox percent; valid while `quota_text` set.
    pub quota_percent: qt_property!(u32; NOTIFY connectivity_changed),
    /// Core's mailbox sentence (`1.3 MiB of 2 GiB used`); empty until relay says.
    pub quota_text: qt_property!(QString; NOTIFY connectivity_changed),
    /// Quota bytes behind `quota_text`, for "left" display as parla; 0 if none. f64: QML lacks
    /// 64-bit int.
    pub quota_used_bytes: qt_property!(f64; NOTIFY connectivity_changed),
    /// See `quota_used_bytes`.
    pub quota_limit_bytes: qt_property!(f64; NOTIFY connectivity_changed),
    /// Profile's device size, bytes. f64 for QML.
    pub storage_bytes: qt_property!(f64; NOTIFY connectivity_changed),
    /// Connection, quota or storage re-read.
    pub connectivity_changed: qt_signal!(),
    /// Re-read connection, mailbox, storage at load and on `ConnectivityChanged`, max 1/s
    /// (`ProfilePage.qml`): four round trips, one sizing every file.
    pub refresh_connectivity: qt_method!(fn(&mut self)),

    /// Loaded. Before: fields unloaded, not blank; save would wipe.
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Any field or `loaded` changed.
    pub loaded_changed: qt_signal!(),

    /// Failure; message from core.
    pub error: qt_signal!(message: QString),
    /// Profile stored.
    pub saved: qt_signal!(),

    /// Read the profile.
    pub reload: qt_method!(fn(&mut self)),
    /// Store name and status.
    pub save: qt_method!(fn(&mut self, display_name: QString, status: QString)),
    /// Use `path` as picture; core copies it to blob dir, picked file may go.
    pub set_picture: qt_method!(fn(&mut self, path: QString)),
    /// Remove the picture.
    pub clear_picture: qt_method!(fn(&mut self)),
    /// Turn read receipts on or off.
    pub set_read_receipts: qt_method!(fn(&mut self, enabled: bool)),

    /// Load counter: stale answers must not win.
    generation: u64,
}

/// Load result: name, status, picture, address, receipts, colour, limit.
type Fields = (String, String, String, String, bool, String, u64);

/// Connectivity result: band, quota if reported, device size.
type Connectivity = (u32, Option<Quota>, f64);

impl Profile {
    /// Set account; reload if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            // Fields hold old profile; save before load would write them onto this one.
            if self.loaded {
                self.loaded = false;
            }
            self.reload();
            self.refresh_connectivity();
        }
    }

    /// Ask core about connection, mailbox, storage.
    pub fn refresh_connectivity(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Connectivity, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().account_id != account_id {
                return;
            }
            match result {
                Ok((connectivity, quota, storage_bytes)) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.connectivity = connectivity;
                        let (percent, text, used, limit) =
                            quota.map_or((0, String::new(), 0, 0), |quota| {
                                (
                                    quota.percent,
                                    quota.text,
                                    quota.used_bytes,
                                    quota.limit_bytes,
                                )
                            });
                        this_mut.quota_percent = percent;
                        this_mut.quota_text = text.into();
                        // Exact to 2^53 bytes.
                        #[allow(clippy::cast_precision_loss)]
                        {
                            this_mut.quota_used_bytes = used as f64;
                            this_mut.quota_limit_bytes = limit as f64;
                        }
                        this_mut.storage_bytes = storage_bytes;
                    }
                    this.borrow().connectivity_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let connectivity: u32 = rpc
                    .call("get_connectivity", (account_id,))
                    .await
                    .map_err(|err| err.to_string())?;
                // Report is web-view page; quota read off it.
                let report: String = rpc
                    .call("get_connectivity_html", (account_id,))
                    .await
                    .map_err(|err| err.to_string())?;
                // Optional: unmeasurable profile still reports connection.
                let storage: u64 = rpc
                    .call("get_account_file_size", (account_id,))
                    .await
                    .unwrap_or(0);
                // Which relay's mailbox (report covers all). Asked, not from `address`: load may
                // not have filled it.
                let address: String = rpc
                    .call::<_, Option<String>>("get_config", (account_id, "configured_addr"))
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                Ok::<_, String>((
                    connectivity,
                    quota_from_report(&report, &address),
                    // f64 for QML.
                    #[allow(clippy::cast_precision_loss)]
                    {
                        storage as f64
                    },
                ))
            }
            .await;
            done(result);
        });
    }

    /// Read profile from core.
    pub fn reload(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };

        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;

        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<Fields, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok((
                    display_name,
                    status,
                    avatar_path,
                    address,
                    read_receipts,
                    color,
                    attachment_limit,
                )) => {
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.display_name = display_name.into();
                        this_mut.status = status.into();
                        this_mut.avatar_path = avatar_path.into();
                        this_mut.address = address.into();
                        this_mut.read_receipts = read_receipts;
                        this_mut.color = color.into();
                        // Exact to 2^53 bytes.
                        #[allow(clippy::cast_precision_loss)]
                        {
                            this_mut.attachment_limit_bytes = attachment_limit as f64;
                        }
                        this_mut.loaded = true;
                    }
                    this.borrow().loaded_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let get = |key: &'static str| {
                    let rpc = rpc.clone();
                    async move {
                        rpc.call::<_, Option<String>>("get_config", (account_id, key))
                            .await
                            .map(Option::unwrap_or_default)
                            .map_err(|err| err.to_string())
                    }
                };
                Ok::<_, String>((
                    get("displayname").await?,
                    get("selfstatus").await?,
                    get("selfavatar").await?,
                    get("configured_addr").await?,
                    // Core stores "1"/"0", default on: absent isn't off.
                    get("mdns_enabled").await? != "0",
                    // Own contact id 1 in every account. Optional: colourless still shows.
                    rpc.call::<_, serde_json::Value>("get_contact", (account_id, 1_u32))
                        .await
                        .map(|contact| json::str_at(&contact, "color").to_string())
                        .unwrap_or_default(),
                    // Recommended attachment ceiling. Optional.
                    media::attachment_limit(&rpc, account_id).await,
                ))
            }
            .await;
            done(result);
        });
    }

    /// Store name and status.
    pub fn save(&mut self, display_name: QString, status: QString) {
        // Saving before load overwrites unseen profile.
        if !self.loaded {
            return;
        }
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();
        let display_name = display_name.to_string();
        let status = status.to_string();

        runtime.spawn(async move {
            let result = async {
                rpc.call::<_, ()>(
                    "set_config",
                    (account_id, "displayname", Some(display_name)),
                )
                .await
                .map_err(|err| err.to_string())?;
                rpc.call::<_, ()>("set_config", (account_id, "selfstatus", Some(status)))
                    .await
                    .map_err(|err| err.to_string())?;
                Ok::<_, String>(())
            }
            .await;
            done(result);
        });
    }

    /// Set picture from `path`.
    pub fn set_picture(&mut self, path: QString) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        // Picker gives a URL; core wants a path.
        let path = path.to_string();
        let path = path.strip_prefix("file://").unwrap_or(&path).to_string();
        if path.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();

        runtime.spawn(async move {
            // `selfavatar`: path core copies to blob dir. Pinned binary refuses "" ("Copying new
            // blobfile failed"); null clears.
            let result = rpc
                .call::<_, ()>("set_config", (account_id, "selfavatar", Some(path)))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Remove picture.
    pub fn clear_picture(&mut self) {
        let account_id = self.account_id;
        if account_id == 0 {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>(
                    "set_config",
                    (account_id, "selfavatar", Option::<String>::None),
                )
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Turn read receipts on or off.
    pub fn set_read_receipts(&mut self, enabled: bool) {
        let account_id = self.account_id;
        // As `save`: before load, default not account's.
        if account_id == 0 || !self.loaded {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.stored_callback();
        let value = if enabled { "1" } else { "0" };

        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("set_config", (account_id, "mdns_enabled", Some(value)))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Report store; read back what core kept (picture rewritten into blob dir).
    fn stored_callback(&self) -> impl Fn(Result<(), String>) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => {
                    this.borrow_mut().reload();
                    this.borrow().saved();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        })
    }
}
