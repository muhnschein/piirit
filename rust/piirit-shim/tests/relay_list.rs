//! The list of public relays the two relay pages offer.
//!
//! `qml/js/Relays.js` is copied by hand from chatmail.at/relays, which
//! publishes no list a build could read, so what can drift is checked
//! here: every row is a relay the core could be pointed at, no relay is
//! offered twice, and the list starts at the relay the app sets profiles
//! up on by default.

use std::fs;
use std::path::PathBuf;

use piirit_shim::DEFAULT_PROVIDER_QR;

/// The `(domain, location)` of every row, in the file's order.
fn relays() -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/js/Relays.js");
    let text = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read Relays.js: {err}"));
    let field = |line: &str, name: &str| -> Option<String> {
        let start = line.find(&format!("{name}: \""))? + name.len() + 3;
        let end = line[start..].find('"')? + start;
        Some(line[start..end].to_string())
    };
    text.lines()
        .filter_map(|line| Some((field(line, "domain")?, field(line, "location")?)))
        .collect()
}

#[test]
fn every_relay_is_offered_once_and_well_formed() {
    let relays = relays();
    assert!(
        relays.len() >= 20,
        "only {} relays read from Relays.js; did its format change?",
        relays.len()
    );

    let default = DEFAULT_PROVIDER_QR.trim_start_matches("dcaccount:");
    assert_eq!(
        relays.first().map(|(domain, _)| domain.as_str()),
        Some(default),
        "the list does not start at the relay new profiles are made on"
    );

    let mut seen = std::collections::BTreeSet::new();
    for (domain, location) in &relays {
        assert!(seen.insert(domain), "{domain} is offered twice");
        assert!(
            domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && domain
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'),
            "{domain:?} is not a host name the core could be pointed at"
        );
        assert!(
            !location.trim().is_empty(),
            "{domain} says nothing about where it is"
        );
    }
}
