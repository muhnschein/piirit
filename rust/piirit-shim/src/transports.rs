//! Relays a profile is reached through.
//!
//! Mail arrives on any transport (`list_transports`); core picks outgoing per connect (newest
//! answering since 2.61), unnamed over JSON-RPC. Own-address relay (`configured_addr`) listed
//! first. `delete_transport`: core refuses last; removing own-address relay makes core pick
//! another, so rows read back: own address is core's answer, not a guess. Rows also carry
//! report dot, status, mailbox (`connectivity.rs`), one parse for all.

use std::cell::RefCell;

use qmetaobject::*;

use crate::connectivity::transport_reports;
use crate::core::connection;
use crate::json;
use crate::models::{TransportItem, TransportListModel};

/// Profile's relays, own-address relay first.
///
/// ```qml
/// Transports { id: transports; account_id: page.accountId }
/// Repeater { model: transports.rows }
/// ```
#[derive(QObject, Default)]
pub struct Transports {
    base: qt_base_class!(trait QObject),

    /// Account whose relays these are; setting reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY account_changed),
    /// Account changed.
    pub account_changed: qt_signal!(),

    /// Own-address relay first, rest in core (add) order. Reset whole per load: few rows, and
    /// removal countdown lives beside list (`PendingRemoval.qml`), not on row.
    pub rows: qt_property!(RefCell<TransportListModel>; CONST),

    /// Row count; gates removing last (core refuses). Field, not `rows.len()`: read mid-rebuild.
    pub count: qt_property!(u32; NOTIFY rows_changed),
    /// Own address (`configured_addr`); empty until loaded.
    pub primary: qt_property!(QString; NOTIFY rows_changed),
    /// `rows` changed.
    pub rows_changed: qt_signal!(),

    /// Core's words ("Cannot remove the last transport").
    pub error: qt_signal!(message: QString),
    /// Relay removed, rows read back; page re-reads profile.
    pub changed: qt_signal!(),

    /// Read relays from core.
    pub reload: qt_method!(fn(&mut self)),
    /// Remove relay `addr` is on. Core refuses last; picks another own address if needed.
    pub remove: qt_method!(fn(&mut self, addr: QString)),
    /// Feed `core_event`; other accounts' ignored.
    pub handle_event:
        qt_method!(fn(&mut self, context_id: u32, kind: QString, payload_json: QString)),

    /// Stale loads dropped: removal reloads, and its `TransportsModified` reloads again.
    generation: u64,
}

/// Load result: transports, own address, report, band.
type Listed = (Vec<serde_json::Value>, String, String, u32);

/// Band at or above which core is connecting; below: not connected.
const CONNECTING: u32 = 2000;

impl Transports {
    /// Set account; reload if changed.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.account_changed();
            self.reload();
        }
    }

    /// Read relays from core.
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
        let done = queued_callback(move |result: Result<Listed, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok((transports, primary, report, band)) => {
                    let rows = rows_from(&transports, &primary, &report, band);
                    {
                        let mut this_mut = this.borrow_mut();
                        this_mut.count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
                        this_mut.primary = primary.into();
                        this_mut.rows.borrow_mut().reset_data(rows);
                    }
                    this.borrow().rows_changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        });

        runtime.spawn(async move {
            let result = async {
                let transports: Vec<serde_json::Value> = rpc
                    .call("list_transports", (account_id,))
                    .await
                    .map_err(|err| err.to_string())?;
                let primary: String = rpc
                    .call::<_, Option<String>>("get_config", (account_id, "configured_addr"))
                    .await
                    .map(Option::unwrap_or_default)
                    .map_err(|err| err.to_string())?;
                // Optional: relay listed even if report unreadable.
                let report: String = rpc
                    .call("get_connectivity_html", (account_id,))
                    .await
                    .unwrap_or_default();
                // Band: see `TransportItem::offline`; 0 if core won't say.
                let band: u32 = rpc
                    .call("get_connectivity", (account_id,))
                    .await
                    .unwrap_or_default();
                Ok::<_, String>((transports, primary, report, band))
            }
            .await;
            done(result);
        });
    }

    /// Remove relay `addr` is on.
    pub fn remove(&mut self, addr: QString) {
        let account_id = self.account_id;
        let addr = addr.to_string();
        if account_id == 0 || addr.is_empty() {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            self.error(QString::from("not started"));
            return;
        };
        let done = self.applied_callback();
        runtime.spawn(async move {
            let result = rpc
                .call::<_, ()>("delete_transport", (account_id, addr))
                .await
                .map_err(|err| err.to_string());
            done(result);
        });
    }

    /// Core event: transports changed here or elsewhere, or overflow.
    pub fn handle_event(&mut self, context_id: u32, kind: QString, _payload_json: QString) {
        if context_id != self.account_id || self.account_id == 0 {
            return;
        }
        if matches!(
            kind.to_string().as_str(),
            "TransportsModified" | "EventChannelOverflow"
        ) {
            self.reload();
        }
    }

    /// Removal applied: read rows back.
    fn applied_callback(&self) -> impl Fn(Result<(), String>) {
        let ptr: QPointer<Self> = QPointer::from(self);
        queued_callback(move |result: Result<(), String>| {
            let Some(this) = ptr.as_pinned() else { return };
            match result {
                Ok(()) => {
                    this.borrow_mut().reload();
                    this.borrow().changed();
                }
                Err(err) => this.borrow().error(err.into()),
            }
        })
    }
}

/// Rows: own-address relay first, mailbox from report; shown not connected below `band`.
fn rows_from(
    transports: &[serde_json::Value],
    primary: &str,
    report: &str,
    band: u32,
) -> Vec<TransportItem> {
    let reports = transport_reports(report);
    let not_connected = band > 0 && band < CONNECTING;
    let mut rows: Vec<TransportItem> = transports
        .iter()
        .map(|transport| json::str_at(transport, "addr"))
        .filter(|addr| !addr.is_empty())
        .map(|addr| {
            let domain = addr.rsplit('@').next().unwrap_or(addr);
            let reported = reports
                .iter()
                .find(|reported| reported.domain.eq_ignore_ascii_case(domain));
            let quota = reported.and_then(|reported| reported.quota.as_ref());
            let offline = reported.is_none() && not_connected;
            // Exact to 2^53 bytes.
            #[allow(clippy::cast_precision_loss)]
            TransportItem {
                id: row_id(addr),
                addr: addr.into(),
                domain: domain.into(),
                is_primary: addr.eq_ignore_ascii_case(primary),
                dot: match reported {
                    Some(reported) => reported.dot.as_str().into(),
                    None if offline => "red".into(),
                    None => QString::default(),
                },
                status: reported
                    .map_or_else(QString::default, |reported| reported.status.as_str().into()),
                offline,
                has_quota: quota.is_some(),
                quota_percent: quota.map_or(0, |quota| quota.percent),
                quota_text: quota.map_or_else(QString::default, |quota| quota.text.as_str().into()),
                quota_used_bytes: quota.map_or(0.0, |quota| quota.used_bytes as f64),
                quota_limit_bytes: quota.map_or(0.0, |quota| quota.limit_bytes as f64),
            }
        })
        .collect();
    // Stable: rest keep core's order.
    rows.sort_by_key(|row| !row.is_primary);
    rows
}

/// Stable number per address: FNV-1a, top bit dropped for QML `int`; keys removal countdown.
/// Collision: one countdown on wrong row.
fn row_id(addr: &str) -> u32 {
    let hash = addr.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    hash & 0x7fff_ffff
}

#[cfg(test)]
mod tests {
    use super::{row_id, rows_from, TransportItem};
    use serde_json::json;

    const REPORT: &str = "<html><body><h3>Incoming messages</h3><ul>\
        <li class=\"transport\"><span class=\"red dot\"></span> <b>old.example.net:</b> Not connected: timed out<br />\
        <ul class=\"quota-list\"><li>1.9 GiB of 2 GiB used\
        <div class=\"bar\"><div class=\"progress red\" style=\"width: 95%\">95%</div></div>\
        </li></ul></li>\
        <li class=\"transport\"><span class=\"green dot\"></span> <b>nine.testrun.org:</b> Connected<br />\
        <ul class=\"quota-list\"><li>1.34 GiB of 2 GiB used\
        <div class=\"bar\"><div class=\"progress grey\" style=\"width: 67%\">67%</div></div>\
        </li></ul></li></ul></body></html>";

    fn listed(addrs: &[&str]) -> Vec<serde_json::Value> {
        addrs.iter().map(|addr| json!({"addr": addr})).collect()
    }

    fn summary(rows: &[TransportItem]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                format!(
                    "{}{}:{}",
                    row.domain,
                    if row.is_primary { "*" } else { "" },
                    if row.has_quota {
                        row.quota_percent.to_string()
                    } else {
                        "-".to_string()
                    }
                )
            })
            .collect()
    }

    /// Own-address relay first; own mailbox per row.
    #[test]
    fn the_profiles_own_relay_is_first_and_each_has_its_own_mailbox() {
        let rows = rows_from(
            &listed(&["ada@old.example.net", "ada@nine.testrun.org"]),
            "ada@nine.testrun.org",
            REPORT,
            4000,
        );
        assert_eq!(
            summary(&rows),
            vec!["nine.testrun.org*:67", "old.example.net:95"]
        );
        assert_eq!(rows[0].addr.to_string(), "ada@nine.testrun.org");
        assert_eq!(rows[0].quota_text.to_string(), "1.34 GiB of 2 GiB used");
        // Whole bytes exact through f64.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bytes = (
            rows[0].quota_used_bytes as u64,
            rows[0].quota_limit_bytes as u64,
        );
        assert_eq!(bytes, (1_438_814_044, 2_147_483_648));
        assert_eq!(rows[0].dot.to_string(), "green");
        assert_eq!(rows[0].status.to_string(), "Connected");
        assert_eq!(rows[1].dot.to_string(), "red");
        assert_eq!(rows[1].status.to_string(), "Not connected: timed out");
        // Core lowercases stored addresses.
        let rows = rows_from(
            &listed(&["ada@old.example.net", "Ada@Nine.Testrun.ORG"]),
            "ada@nine.testrun.org",
            REPORT,
            4000,
        );
        assert_eq!(
            summary(&rows),
            vec!["Nine.Testrun.ORG*:67", "old.example.net:95"]
        );
    }

    /// No primary, no report: relays, no mailbox.
    #[test]
    fn rows_stand_without_a_primary_or_a_report() {
        let rows = rows_from(
            &listed(&["ada@old.example.net", "ada@nine.testrun.org", ""]),
            "",
            "",
            0,
        );
        assert_eq!(
            summary(&rows),
            vec!["old.example.net:-", "nine.testrun.org:-"]
        );
        assert_eq!(rows[0].dot.to_string(), "");
        assert_eq!(rows[0].status.to_string(), "");
        assert!(rows_from(&[], "ada@nine.testrun.org", REPORT, 4000).is_empty());
    }

    /// Not connected: each relay shown so.
    #[test]
    fn a_profile_not_connected_at_all_has_relays_not_connected() {
        let relays = listed(&["ada@old.example.net", "ada@nine.testrun.org"]);
        let rows = rows_from(
            &relays,
            "ada@nine.testrun.org",
            "<h3>Not connected</h3>",
            1000,
        );
        assert_eq!(
            rows.iter()
                .map(|row| (row.dot.to_string(), row.status.to_string(), row.offline))
                .collect::<Vec<_>>(),
            vec![
                ("red".to_string(), String::new(), true),
                ("red".to_string(), String::new(), true),
            ]
        );
        let rows = rows_from(&relays, "ada@nine.testrun.org", REPORT, 1000);
        assert!(rows.iter().all(|row| !row.offline));
        assert_eq!(rows[1].status.to_string(), "Not connected: timed out");
        for band in [0, 2000, 4000] {
            let rows = rows_from(&relays, "", "", band);
            assert!(
                rows.iter().all(|row| !row.offline && row.dot.is_empty()),
                "band {band}"
            );
        }
    }

    /// Countdown key: stable, never negative.
    #[test]
    fn a_row_keeps_its_number_across_reloads() {
        let before = rows_from(
            &listed(&["ada@old.example.net", "ada@nine.testrun.org"]),
            "",
            "",
            0,
        );
        let after = rows_from(
            &listed(&[
                "ada@nine.testrun.org",
                "ada@mehl.cloud",
                "ada@old.example.net",
            ]),
            "ada@nine.testrun.org",
            REPORT,
            4000,
        );
        let id_of = |rows: &[TransportItem], addr: &str| {
            rows.iter()
                .find(|row| row.addr.to_string() == addr)
                .map(|row| row.id)
        };
        assert_eq!(
            id_of(&before, "ada@old.example.net"),
            id_of(&after, "ada@old.example.net")
        );
        assert_ne!(
            id_of(&after, "ada@old.example.net"),
            id_of(&after, "ada@nine.testrun.org")
        );
        assert_eq!(
            row_id("ada@nine.testrun.org"),
            row_id("ada@nine.testrun.org")
        );
        i32::try_from(row_id("a very long address that hashes high@example.org")).unwrap();
    }
}
