//! Encryption page: status line and fingerprints, per contact or per group.
//!
//! Core gives text, not fields (`get_contact_encryption_info`, `get_chat_encryption_info`);
//! parsed into blocks split by blank lines. Shape pinned below at core 2.68.0; unknown shape =
//! raw text shown instead. Relays: domain only, never the address before `@` (app shows none).

use qmetaobject::*;

use crate::core::connection;

/// One person's key, as the page lists it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct KeyEntry {
    /// Core's display name; "Me" (stock string 2) for own key.
    pub(crate) name: String,
    /// Two lines of five four-digit groups, as core formats.
    pub(crate) fingerprint: String,
    /// Relay domains from the key; group entries only (contact's are `Parsed::relays`).
    pub(crate) relays: Vec<String>,
    /// Core has fingerprint but no key ("(key missing)").
    pub(crate) key_missing: bool,
}

/// Core's text, split.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Parsed {
    /// First line: "Messages are end-to-end encrypted." or "No encryption." (stock strings).
    pub(crate) status: String,
    pub(crate) keys: Vec<KeyEntry>,
    /// Contact's relays, domains.
    pub(crate) relays: Vec<String>,
}

/// `XXXX XXXX XXXX XXXX XXXX`: one line of core's `human_readable` fingerprint.
fn is_fingerprint_line(line: &str) -> bool {
    let groups: Vec<&str> = line.split(' ').collect();
    groups.len() == 5
        && groups
            .iter()
            .all(|group| group.len() == 4 && group.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Relay address to its domain; `None` if no `@`.
fn relay_domain(address: &str) -> Option<String> {
    address
        .trim()
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_string())
        .filter(|domain| !domain.is_empty())
}

/// Fingerprint lines at end of block, joined; `None` unless exactly two.
fn fingerprint_of(lines: &[&str]) -> Option<String> {
    (lines.len() == 2 && lines.iter().all(|line| is_fingerprint_line(line)))
        .then(|| lines.join("\n"))
}

/// Contact text: `status\nFingerprints:`, then `name:\nfp\nfp` per key, then `Relays:\naddr…`.
pub(crate) fn parse_contact(text: &str) -> Option<Parsed> {
    let mut blocks = text.trim().split("\n\n");
    let head: Vec<&str> = blocks.next()?.lines().collect();
    let status = (*head.first()?).to_string();
    let mut parsed = Parsed {
        status,
        ..Parsed::default()
    };
    // "No encryption.": nothing else.
    if head.len() == 1 {
        return blocks.next().is_none().then_some(parsed);
    }
    for block in blocks {
        let lines: Vec<&str> = block.lines().collect();
        let (first, rest) = lines.split_first()?;
        if let Some(fingerprint) = fingerprint_of(rest) {
            let name = first.strip_suffix(':')?;
            parsed.keys.push(KeyEntry {
                name: name.to_string(),
                fingerprint,
                ..KeyEntry::default()
            });
        } else if parsed.relays.is_empty() && !rest.is_empty() && !parsed.keys.is_empty() {
            // Last block, core's untranslated "Relays:".
            parsed.relays = rest.iter().filter_map(|line| relay_domain(line)).collect();
        } else {
            return None;
        }
    }
    (parsed.keys.len() == 2).then_some(parsed)
}

/// Group text: status, then per member `name(relay,relay)` or `name`, optional `(key missing)`,
/// fingerprint.
pub(crate) fn parse_chat(text: &str) -> Option<Parsed> {
    let mut blocks = text.trim().split("\n\n");
    let status = blocks.next()?.trim().to_string();
    if status.contains('\n') {
        return None;
    }
    let mut parsed = Parsed {
        status,
        ..Parsed::default()
    };
    for block in blocks {
        let lines: Vec<&str> = block.lines().collect();
        let (first, rest) = lines.split_first()?;
        let (key_missing, rest) = match rest.split_first() {
            Some((&"(key missing)", tail)) => (true, tail),
            _ => (false, rest),
        };
        let fingerprint = fingerprint_of(rest)?;
        // Relays as `(a,b)` glued to name; addresses have `@`, a name's own brackets don't.
        let (name, relays) = match first.strip_suffix(')').and_then(|s| s.rsplit_once('(')) {
            Some((name, list)) if list.split(',').all(|a| relay_domain(a).is_some()) => (
                name.to_string(),
                list.split(',').filter_map(relay_domain).collect(),
            ),
            _ => ((*first).to_string(), Vec::new()),
        };
        parsed.keys.push(KeyEntry {
            name,
            fingerprint,
            relays,
            key_missing,
        });
    }
    Some(parsed)
}

/// Fingerprints to copy: names and keys, no addresses.
fn copy_text(parsed: &Parsed) -> String {
    parsed
        .keys
        .iter()
        .map(|key| format!("{}:\n{}", key.name, key.fingerprint))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// What core says about one contact's or one group's encryption.
///
/// ```qml
/// EncryptionInfo { account_id: 1; contact_id: 10 }   // or chat_id for a group
/// ```
#[derive(QObject, Default)]
pub struct EncryptionInfo {
    base: qt_base_class!(trait QObject),

    /// Account. Set reloads.
    pub account_id: qt_property!(u32; WRITE set_account_id NOTIFY source_changed),
    /// Contact, for one-to-one. Set reloads.
    pub contact_id: qt_property!(u32; WRITE set_contact_id NOTIFY source_changed),
    /// Group; used when `contact_id` is 0. Set reloads.
    pub chat_id: qt_property!(u32; WRITE set_chat_id NOTIFY source_changed),
    /// Account, contact or chat changed.
    pub source_changed: qt_signal!(),

    /// Answer in (or failed).
    pub loaded: qt_property!(bool; NOTIFY loaded_changed),
    /// Core's text split; false: shape not known, `raw` only.
    pub parsed: qt_property!(bool; NOTIFY loaded_changed),
    /// Core's first line.
    pub status: qt_property!(QString; NOTIFY loaded_changed),
    /// Keys: maps `name`, `fingerprint`, `relays` (domains, ", "), `key_missing`.
    pub keys: qt_property!(QVariantList; NOTIFY loaded_changed),
    /// Contact's relay domains, ", ".
    pub relays: qt_property!(QString; NOTIFY loaded_changed),
    /// Core's text as given, when not `parsed`.
    pub raw: qt_property!(QString; NOTIFY loaded_changed),
    /// Names and fingerprints, for clipboard.
    pub copy_text: qt_property!(QString; NOTIFY loaded_changed),
    /// Emitted per answer.
    pub loaded_changed: qt_signal!(),

    /// Failure; core's message.
    pub error: qt_signal!(message: QString),
    /// Ask core again.
    pub reload: qt_method!(fn(&mut self)),

    /// Stale-answer guard.
    generation: u64,
}

impl EncryptionInfo {
    /// Set account; reload.
    pub fn set_account_id(&mut self, account_id: u32) {
        if self.account_id != account_id {
            self.account_id = account_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set contact; reload.
    pub fn set_contact_id(&mut self, contact_id: u32) {
        if self.contact_id != contact_id {
            self.contact_id = contact_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Set group; reload.
    pub fn set_chat_id(&mut self, chat_id: u32) {
        if self.chat_id != chat_id {
            self.chat_id = chat_id;
            self.source_changed();
            self.reload();
        }
    }

    /// Ask core; contact if set, else chat.
    pub fn reload(&mut self) {
        let (account_id, contact_id, chat_id) = (self.account_id, self.contact_id, self.chat_id);
        if account_id == 0 || (contact_id == 0 && chat_id == 0) {
            return;
        }
        let Some((rpc, runtime)) = connection() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let for_contact = contact_id != 0;
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Result<String, String>| {
            let Some(this) = ptr.as_pinned() else { return };
            if this.borrow().generation != generation {
                return;
            }
            match result {
                Ok(text) => this.borrow_mut().take(&text, for_contact),
                Err(err) => {
                    this.borrow_mut().loaded = true;
                    this.borrow().loaded_changed();
                    this.borrow().error(err.into());
                }
            }
        });
        runtime.spawn(async move {
            let result = if for_contact {
                rpc.call::<_, String>("get_contact_encryption_info", (account_id, contact_id))
                    .await
            } else {
                rpc.call::<_, String>("get_chat_encryption_info", (account_id, chat_id))
                    .await
            };
            done(result.map_err(|err| err.to_string()));
        });
    }

    /// Answer into properties.
    fn take(&mut self, text: &str, for_contact: bool) {
        let parsed = if for_contact {
            parse_contact(text)
        } else {
            parse_chat(text)
        };
        self.raw = text.trim().into();
        self.parsed = parsed.is_some();
        let parsed = parsed.unwrap_or_default();
        self.status = parsed.status.as_str().into();
        self.relays = parsed.relays.join(", ").into();
        self.copy_text = copy_text(&parsed).into();
        self.keys = parsed
            .keys
            .iter()
            .map(|key| {
                let mut map = QVariantMap::default();
                map.insert(
                    "name".into(),
                    QVariant::from(QString::from(key.name.as_str())),
                );
                map.insert(
                    "fingerprint".into(),
                    QVariant::from(QString::from(key.fingerprint.as_str())),
                );
                map.insert(
                    "relays".into(),
                    QVariant::from(QString::from(key.relays.join(", ").as_str())),
                );
                map.insert("key_missing".into(), QVariant::from(key.key_missing));
                QVariant::from(map)
            })
            .collect();
        self.loaded = true;
        self.loaded_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_text, parse_chat, parse_contact, KeyEntry};

    const FP_ME: &str = "2F91 C04E 6B7D A315 8C2E\n90F4 D16B 3A7E 05C9 B842";
    const FP_THEM: &str = "7C3A 9E15 D0B4 462F 8E1A\nC93B 0F5D 27E6 A41C 88D2";

    /// As core 2.68.0 `Contact::get_encrinfo` writes it.
    fn contact_text(relays: &str) -> String {
        format!(
            "Messages are end-to-end encrypted.\nFingerprints:\n\nMe:\n{FP_ME}\n\nRandy:\n{FP_THEM}{relays}"
        )
    }

    #[test]
    fn a_contact_s_text_splits_into_status_two_keys_and_relay_domains() {
        let parsed = parse_contact(&contact_text(
            "\n\nRelays:\nrandy@relay.example.org\nr2@other.example.net",
        ))
        .expect("parsed");
        assert_eq!(parsed.status, "Messages are end-to-end encrypted.");
        assert_eq!(
            parsed.keys,
            vec![
                KeyEntry {
                    name: "Me".into(),
                    fingerprint: FP_ME.into(),
                    ..KeyEntry::default()
                },
                KeyEntry {
                    name: "Randy".into(),
                    fingerprint: FP_THEM.into(),
                    ..KeyEntry::default()
                },
            ]
        );
        // Domain only: the part before `@` is their address.
        assert_eq!(
            parsed.relays,
            vec!["relay.example.org", "other.example.net"]
        );
    }

    #[test]
    fn a_contact_without_relays_still_parses() {
        let parsed = parse_contact(&contact_text("")).expect("parsed");
        assert_eq!(parsed.keys.len(), 2);
        assert!(parsed.relays.is_empty());
    }

    #[test]
    fn no_encryption_is_status_alone() {
        let parsed = parse_contact("No encryption.").expect("parsed");
        assert_eq!(parsed.status, "No encryption.");
        assert!(parsed.keys.is_empty());
    }

    #[test]
    fn an_unknown_contact_shape_is_refused_not_guessed() {
        // One key only, a stray block, a broken fingerprint: raw text instead.
        assert!(parse_contact(&format!(
            "Messages are end-to-end encrypted.\nFingerprints:\n\nMe:\n{FP_ME}"
        ))
        .is_none());
        assert!(parse_contact(&format!("{}\n\nsomething new", contact_text(""))).is_none());
        assert!(parse_contact(
            "Messages are end-to-end encrypted.\nFingerprints:\n\nMe:\nnot hex\n\nRandy:\nx"
        )
        .is_none());
        assert!(parse_contact("").is_none());
    }

    #[test]
    fn a_group_s_text_names_each_member_with_relays_and_missing_keys() {
        // As core 2.68.0 `ChatId::get_encryption_info`, after its final trim.
        let text = format!(
            "Messages are end-to-end encrypted.\n\nBrian(b@relay.example.net,b2@x.example.org)\n{FP_ME}\n\n\
             Judith\n(key missing)\n{FP_THEM}\n\nThe (other) Reg\n{FP_ME}"
        );
        let parsed = parse_chat(&text).expect("parsed");
        assert_eq!(parsed.status, "Messages are end-to-end encrypted.");
        assert_eq!(
            parsed.keys,
            vec![
                KeyEntry {
                    name: "Brian".into(),
                    fingerprint: FP_ME.into(),
                    relays: vec!["relay.example.net".into(), "x.example.org".into()],
                    key_missing: false,
                },
                KeyEntry {
                    name: "Judith".into(),
                    fingerprint: FP_THEM.into(),
                    relays: Vec::new(),
                    key_missing: true,
                },
                KeyEntry {
                    name: "The (other) Reg".into(),
                    fingerprint: FP_ME.into(),
                    relays: Vec::new(),
                    key_missing: false,
                },
            ]
        );
    }

    #[test]
    fn a_name_ending_in_brackets_is_not_read_as_relays() {
        let parsed = parse_chat(&format!(
            "Messages are end-to-end encrypted.\n\nBob (work)\n{FP_ME}"
        ))
        .expect("parsed");
        assert_eq!(parsed.keys[0].name, "Bob (work)");
        assert!(parsed.keys[0].relays.is_empty());
    }

    #[test]
    fn a_group_alone_or_unencrypted_is_status_alone_and_odd_shapes_are_refused() {
        let alone = parse_chat("Messages are end-to-end encrypted.").expect("parsed");
        assert!(alone.keys.is_empty());
        assert_eq!(
            parse_chat("No encryption.").expect("parsed").status,
            "No encryption."
        );
        assert!(parse_chat("Messages are end-to-end encrypted.\n\nBob\nnot a key").is_none());
        assert!(parse_chat("two\nlines").is_none());
    }

    #[test]
    fn copying_takes_names_and_fingerprints_never_addresses() {
        let parsed =
            parse_contact(&contact_text("\n\nRelays:\nrandy@relay.example.org")).expect("parsed");
        let copied = copy_text(&parsed);
        assert_eq!(copied, format!("Me:\n{FP_ME}\n\nRandy:\n{FP_THEM}"));
        assert!(!copied.contains('@'));
    }
}
