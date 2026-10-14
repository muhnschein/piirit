# Piirit

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

## What this is

- Delta Chat = chatmail messenger. E2E, no phone number, no central operator.
- Thesis: **do not build a messenger; build a SailfishOS UI on top of one.** Protocol, crypto, storage = upstream core. Piirit = UI, platform integration, packaging. Target: Jolla's Harbour store.

## What this isn't

- **No protocol logic**: no IMAP/SMTP/MIME, Autocrypt, encryption. Protocol code = core dependency misused. Key boundary.
- **No hand-written C FFI.** CFFI exists; JSON-RPC = sanctioned, lower-maintenance path.
- **No push service.** No Delta Chat push infra for third-party clients.
- **No plain-email chats.**
- **No multi-protocol bridging, desktop/web build, chatmail server.** Single-purpose.
- **No old Sailfish releases.** One modern baseline, expanded only for future Jolla products. [Buy a Jolla Phone 2026](https://commerce.jolla.com/); support European-made alternatives.
- **No OpenRepos/Chum.** Harbour or nothing: platform competitive for non-technical people needs no dev mode, no SSH fixes, no community repos. Means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food) and telling Jolla what is missing.

## Architecture

```
QML / Silica UI
        |  models / signals
Rust shim (qmetaobject-rs): JSON-RPC client, event loop -> Qt queued
signals, QAbstractListModel adapters for chats/messages/accounts
        |  JSON-RPC over stdio
deltachat-rpc-server (bundled binary, subprocess) = the entire core
```

## The words on screen

- Every `qsTr()` in `qml/` translated into 39 languages. English = translators' source. Metaphor, ellipsis, phrasal verbs break invisibly.
- Shipped in 1.0.0, undone:
  - Figurative verb read literally: "Draws *stars* and `backticks` rather than showing them" meant *renders*; Finnish, Russian, Hungarian took *sketches*.
  - Phrasal verb resolved three ways: "Take the profile over" became *assume control of* (German), *receive* (Finnish), *transfer* (Russian). One button, three meanings.
  - Verbless fragment: "... Every profile, from now on" fine in English, broken in inflected language.

Rules, one finished sentence per string:

- Subject + verb, one clause. No fragment coda, epigram, rule of three.
- Literal verbs: "formats" not "draws", "stores" not "holds". Plain over phrasal: "copy" not "take over".
- Delta Chat's noun where it has one (same domain, same 39 languages). Better: its whole string verbatim; its `values-<lang>/strings.xml` translation comes along, community-proofread.
- Sentence case. `…`, not three dots.

Changing source string orphans its translation in every catalog; `tests/translation_catalogs.rs` fails until all refilled. Wrong English costs 39 times. Write once.

## Platform baseline

- Toolchain floor **Rust 1.75.0, Qt 5.6.3** (what Sailfish ships).
- Built against **5.2** SDK (Jolla Phone baseline). Older = out of scope: binary from newer SDK can call symbols older phone lacks; accepted, not worked around. Harbour requires it too: rejects binary not linking `__libc_start_main@GLIBC_2.34`, which only 5.x glibc provides.
- `aarch64`, `armv7hl` devices; `i486`, `x86_64` emulator.
- Accounts = core storage, in sailjail grant `$XDG_DATA_HOME/piirit/piirit/accounts` (`PIIRIT_ACCOUNTS_DIR` overrides).

## What is missing

1. **Harbour-readiness.** Every source-checkable rule = mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`). Real validator runs against each built RPM. One blocker, not fixable here: bundled `deltachat-rpc-server` = second ELF executable; Harbour permits nowhere.
2. **Message polish**: avatars on bubbles.
3. **Rest of webxdc API.** Apps sent, shown, run (`webxdc.rs`, `WebxdcPage.qml`); status updates both ways. Missing: `importFiles`, realtime channels. App's `source_code_url` not shown.
