//! `HANDLED_EVENT_KINDS` is the whole of what the app reads.
//!
//! `DeltaChatCore::relay` fires `core_event` only for the kinds on that
//! list, because everything it fires reaches every page still on the stack
//! and one chat list per profile on the cover, each of which parses the
//! payload again -- work done with the screen off for events nothing reads.
//! See docs/POWER.md.
//!
//! The risk that buys is silence: a page that starts reading a new kind and
//! is not added to the list never sees one, and nothing about that looks
//! like a bug until someone notices a chat that does not update. So the
//! list is checked here against the code that reads it, both ways round.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use postivene_shim::HANDLED_EVENT_KINDS;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn qml_dir() -> PathBuf {
    crate_dir().join("../../qml")
}

/// The `{ ... }` body of every `fn handle_event` in a source file.
///
/// Brace-matched rather than read to the next line that looks like the end,
/// so a nested block cannot cut a body short. String literals are stepped
/// over; a brace inside one would otherwise unbalance the count.
fn handle_event_bodies(source: &str) -> Vec<String> {
    let mut bodies = Vec::new();
    let bytes: Vec<char> = source.chars().collect();
    let mut from = 0usize;
    while let Some(found) = source[from..].find("fn handle_event") {
        let at = from + found;
        from = at + "fn handle_event".len();
        // The character offset of the signature, then its opening brace.
        let head = source[..at].chars().count();
        let Some(open) = (head..bytes.len()).find(|&i| bytes[i] == '{') else {
            continue;
        };
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = None;
        for (i, &c) in bytes.iter().enumerate().skip(open) {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = end {
            bodies.push(bytes[open..=end].iter().collect());
        }
    }
    bodies
}

/// Whether a string is shaped like a core event kind.
///
/// Every one of them is CamelCase and nothing else: `IncomingMsg`,
/// `ChatlistItemChanged`, `Info`. The app has variables called `kind` that
/// hold something else entirely -- the sort of thing a gallery shows, say
/// -- and they are lower case, which is what tells them apart from these.
fn looks_like_a_kind(literal: &str) -> bool {
    literal.chars().next().is_some_and(char::is_uppercase)
        && literal.chars().all(char::is_alphanumeric)
}

/// Every literal in a piece of source that is shaped like a kind.
fn camel_case_literals(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        let literal = &after[..close];
        rest = &after[close + 1..];
        if looks_like_a_kind(literal) {
            found.insert(literal.to_string());
        }
    }
    found
}

/// Every kind named in a `kind === "X"` or `kind !== "X"` in QML.
fn qml_kinds(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for marker in ["kind ===", "kind !=="] {
        let mut rest = text;
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let Some(open) = rest.find('"') else { break };
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            let literal = &after[..close];
            if looks_like_a_kind(literal) {
                found.insert(literal.to_string());
            }
        }
    }
    found
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", path.display()))
}

/// Every kind anything in the app reads, from the code that reads it.
fn kinds_read() -> BTreeSet<String> {
    let mut found = BTreeSet::new();

    let src = crate_dir().join("src");
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&src)
        .unwrap_or_else(|err| panic!("cannot read {}: {err}", src.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "rs"))
        .collect();
    sources.sort();
    assert!(
        !sources.is_empty(),
        "no sources found under {}",
        src.display()
    );
    for path in &sources {
        for body in handle_event_bodies(&read(path)) {
            found.extend(camel_case_literals(&body));
        }
    }

    let mut qml: Vec<PathBuf> = Vec::new();
    let mut dirs = vec![qml_dir()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|err| panic!("cannot read {}: {err}", dir.display()))
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|kind| kind == "qml") {
                qml.push(path);
            }
        }
    }
    qml.sort();
    assert!(
        !qml.is_empty(),
        "no QML found under {}",
        qml_dir().display()
    );
    for path in &qml {
        found.extend(qml_kinds(&read(path)));
    }

    found
}

#[test]
fn the_list_is_sorted_and_says_each_kind_once() {
    let mut sorted = HANDLED_EVENT_KINDS.to_vec();
    sorted.sort_unstable();
    assert_eq!(
        HANDLED_EVENT_KINDS,
        &sorted[..],
        "HANDLED_EVENT_KINDS is out of order, which is the only thing that \
         makes it readable at a glance"
    );
    let unique: BTreeSet<&&str> = HANDLED_EVENT_KINDS.iter().collect();
    assert_eq!(
        unique.len(),
        HANDLED_EVENT_KINDS.len(),
        "HANDLED_EVENT_KINDS names a kind twice"
    );
}

#[test]
fn every_kind_the_app_reads_is_one_the_core_is_allowed_to_send_it() {
    let read = kinds_read();
    let allowed: BTreeSet<String> = HANDLED_EVENT_KINDS.iter().map(|&s| s.to_string()).collect();

    let unreachable: Vec<&String> = read.difference(&allowed).collect();
    assert!(
        unreachable.is_empty(),
        "these kinds are read somewhere in the app but are not in \
         HANDLED_EVENT_KINDS, so `relay` drops them and whatever reads them \
         will never fire -- add them to the list in src/core.rs: \
         {unreachable:?}"
    );

    let unread: Vec<&String> = allowed.difference(&read).collect();
    assert!(
        unread.is_empty(),
        "these kinds are in HANDLED_EVENT_KINDS but nothing reads them any \
         more, so every one of them is serialised and handed to every \
         listener for nothing -- take them out of the list in src/core.rs: \
         {unread:?}"
    );
}
