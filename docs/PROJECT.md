# Piirit

*Native SailfishOS client for [Delta Chat](https://delta.chat).*

## What this is

- Delta Chat: chatmail messenger, E2E encrypted, no phone number, no central operator.
- **Don't build messenger; build SailfishOS UI on one.** Protocol, crypto, storage = upstream core's. Piirit = UI, platform integration, packaging; target Jolla's Harbour store.

## What this isn't

- **No protocol logic**: no IMAP/SMTP/MIME, Autocrypt, encryption. Writing protocol code = misusing core. Key boundary.
- **No hand-written C FFI.** CFFI exists; JSON-RPC = sanctioned, cheaper path.
- **No push service**: no Delta Chat push infra for third-party clients.
- **No plain-email chats, multi-protocol bridging, desktop/web build, chatmail server.** Single-purpose.
- **No old Sailfish releases.** One modern baseline, widened only for future Jolla products. [Buy a Jolla Phone 2026](https://commerce.jolla.com/); support European-made alternatives.
- **No OpenRepos/Chum.** Harbour or nothing: platform fit for non-technical people needs no dev mode, SSH fixes, community repos. Means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food), telling Jolla what's missing.

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

Every `qsTr()` in `qml/` → 39 languages; English = translators' source. Metaphor, ellipsis, phrasal verbs break invisibly. Undone after 1.0.0: "Draws" (meant *renders*) read as *sketches*; "Take the profile over" got three meanings in German, Finnish, Russian; verbless "... Every profile, from now on" broke in inflected languages.

One finished sentence per string:

- One clause, subject + verb. No fragment coda, epigram, rule of three.
- Literal verbs ("formats" not "draws", "stores" not "holds"); plain over phrasal ("copy" not "take over").
- Delta Chat's noun where it has one; better, its whole string verbatim: `values-<lang>/strings.xml` translation comes along, community-proofread.
- Sentence case; `…`, not three dots.

Changed source string orphans translation in every catalog; `tests/translation_catalogs.rs` fails until all refilled. Wrong English costs 39×.

## Platform baseline

- Toolchain floor **Rust 1.75.0, Qt 5.6.3** (what Sailfish ships).
- Built vs **5.2** SDK (Jolla Phone baseline). Older out of scope: newer-SDK binary may call symbols older phone lacks; accepted. Harbour demands 5.x anyway (`__libc_start_main@GLIBC_2.34`, `HARBOUR.md`).
- `aarch64`, `armv7hl` devices; `i486`, `x86_64` emulator.
- Accounts: core's storage, in sailjail grant `$XDG_DATA_HOME/piirit/piirit/accounts` (`PIIRIT_ACCOUNTS_DIR` overrides).

## What is missing

By priority:

1. **Harbour-readiness.** Source-checkable rules = mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`); real validator on each built RPM. One blocker, unfixable here: bundled `deltachat-rpc-server` = second ELF executable, permitted nowhere.
2. **Message polish**: avatars on bubbles.
3. **Rest of webxdc API.** Apps sent, shown, run (`webxdc.rs`, `WebxdcPage.qml`); status updates both ways. Missing: `importFiles`, realtime channels; `source_code_url` not shown.
