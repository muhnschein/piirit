# Piirit

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

## What this is

Delta Chat is a chatmail messenger with end-to-end encryption, no phone
number and no central operator.

The thesis is narrow: **do not build a messenger, build a SailfishOS UI on
top of one.** Protocol, cryptography and storage are the upstream core's.
Piirit contributes the presentation layer, the platform integration and
the packaging, and aspires to ships to Jolla's Harbour app store.

## What this isn't

- **Reimplementing any protocol logic** — no IMAP/SMTP/MIME, no Autocrypt,
  no encryption. If protocol code is being written, the core dependency is
  being misused. This is the most important boundary in the project.
- **Hand-written C FFI bindings.** The CFFI exists; JSON-RPC is the
  sanctioned, lower-maintenance path.
- **A push-notification service.** No Delta Chat push infrastructure is
  available to third-party clients.
- **Plain-email chats.**
- **Multi-protocol bridging**, a **desktop or web build**, and **running a
  chatmail server**. Single-purpose client only.
- **Old Sailfish releases.** One modern baseline, expanded only for future
  Jolla products. [Buy a Jolla Phone 2026](https://commerce.jolla.com/) and
  support European-made alternatives. 👊🇪🇺🔥
- **Shipping via OpenRepos/Chum.** Harbour or nothing: a platform that is
  competitive for regular, non-technical people is one that needs no
  developer mode, no SSH to fix small things, and no community repos.
  That means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food),
  and telling Jolla what the platform is still missing.

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

Every `qsTr()` in `qml/` is translated into 39 languages, so the English
is not only what a reader sees -- it is the source text a translator
works from. English that leans on metaphor, ellipsis or an ambiguous
phrasal verb does not survive that trip, and the damage is invisible from
here. Three that shipped in 1.0.0 and had to be undone:

- **A figurative verb read literally.** "Draws *stars* and `backticks`
  rather than showing them" meant *renders*. Finnish, Russian and
  Hungarian all took it for *sketches*.
- **An ambiguous phrasal verb resolved three ways.** "Take the profile
  over" became *assume control of* in German, *receive* in Finnish and
  *transfer* in Russian -- three meanings for one button.
- **A verbless fragment kept as a verbless fragment.** "... Every profile,
  from now on" is fine in English and broken in an inflected language.

So a string says what the thing does, once, in a finished sentence:

- One clause with a subject and a verb. No second fragment as a coda, no
  epigram, no rule of three.
- Literal verbs. "Formats", not "draws"; "stores", not "holds".
- No phrasal verb where a plain one exists. "Copy", not "take over".
- Delta Chat's noun where Delta Chat has one -- it is the same domain and
  the same 39 languages. Better still, Delta Chat's whole string: adopt
  it verbatim and its `values-<lang>/strings.xml` translation comes with
  it, already proof-read by its community.
- Sentence case, and `…` rather than three dots.

Changing a source string orphans its translation in every catalog and
`tests/translation_catalogs.rs` fails until each is refilled, so the cost
of getting the English wrong is paid 39 times. Write it once.

## Platform baseline

- Toolchain floor **Rust 1.75.0, Qt 5.6.3** — what Sailfish ships.
- Built against the **5.2** SDK, the Jolla Phone's baseline. Anything older
  is out of scope: a binary from a newer SDK can call symbols an older
  phone lacks, and that is accepted rather than worked around. Harbour
  requires it too -- it rejects a binary that does not link
  `__libc_start_main@GLIBC_2.34`, which only a 5.x glibc provides.
- `aarch64` and `armv7hl` for devices; `i486`/`x86_64` for the emulator.
- Account storage is the core's own, pinned inside the sailjail grant at
  `$XDG_DATA_HOME/piirit/piirit/accounts` (`PIIRIT_ACCOUNTS_DIR`
  overrides).

## What is missing

In order of what matters:

1. **Harbour-readiness.** Every rule a source tree can answer is now a
   mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`), and the real
   validator runs against each built RPM. One blocker remains, and it is not
   fixable here: the bundled `deltachat-rpc-server` is a second ELF
   executable, which Harbour permits nowhere.
2. **Message polish**: avatars on bubbles, and a way to react with an
   emoji the quick row does not offer.
3. **The rest of the webxdc API.** Apps are sent, shown and run
   (`webxdc.rs`, `WebxdcPage.qml`), and status updates go both ways. What
   is not offered is the newer calls -- `importFiles`, realtime channels.
   Nor is an app's `source_code_url` shown anywhere.