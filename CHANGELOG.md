# Changelog

What each release of Piirit brings, newest first. The heading of an entry
is the version and the day it was cut; the section under it is the text
of the GitHub release, which `scripts/release-notes.sh` cuts out of this
file when the release is made (`.github/workflows/rpm.yml`;
docs/BUILDING.md, "Cutting a release").

## Unreleased

A profile can have more than one transport -- a relay set up long ago
beside the one it sends from now -- and the app took the wrong one of
them in two places, reported from a phone whose profiles page named a
relay its profile had left.

- The address under a profile's name on the profiles page is the relay it
  sends from (`configured_addr`) rather than the account list's `addr`,
  a key the core deprecated and only falls back to it with.
- The mailbox figure on the profile page is that same relay's. The core's
  connectivity report covers every transport, and the first quota bar in
  it belongs to whichever was set up first.
- The relay list is the one chatmail.at/relays publishes today, and the
  dialog no longer opens on a relay of its own choosing: it asks, and
  Create waits until the answer is there.

A profile's relays are on its page now, not just the one it sends from.

- The profile page lists every relay the profile is reached through, the
  one it sends from first and marked as such, each with its own address.
  A row's menu sends from that relay instead, or removes it after the
  countdown every other deletion gets; the last relay is not offered for
  removal, since the core refuses it. The profiles page's row follows
  the switch, as does everything else that names the address.
- Storage and connectivity are reported relay by relay: for each, the
  core's own dot and words about its connection, and its mailbox on a
  bar. Before them, what will go in one message -- the core's own
  ceiling, the same for every relay -- and what the profile takes on
  the phone.
- One more relay can be added from the plus under the rows, from the same
  list the profile was made from or a typed server, with the same wait,
  Cancel and time-out the first relay had. A relay that answers after the
  reader gave up is kept on the profile rather than undone: the profile
  is theirs, and its page lists what the relay added.
- The relays follow the other devices the profile is on: the core says
  when they change, and the page reads them again.
- The relay pickers say what they are for: the list is "Select a public
  chatmail relay", the field "Use a custom chatmail relay", and the note
  under them says what a chatmail relay is and where the list of public
  ones lives. The name field on the profile page has no line under it.

## 1.0.0 — 2026-09-18

The first release. Piirit is a native Sailfish OS client for
[Delta Chat](https://delta.chat): a Silica/QML app over the upstream core,
which it bundles as `deltachat-rpc-server` 2.60.0 and drives over JSON-RPC.
It implements no messaging protocol and no cryptography of its own.

Built for the Jolla Phone 2026 (Sailfish OS 5.2, aarch64) and nothing
older.

### Profiles

- Set up a profile on a chatmail relay, take one over from another device
  by scanning its code, or restore one from a backup file.
- Several profiles side by side, with the chat list switching between them
  and the app reopening on the one last shown.
- Name, picture and signature on the profile's own page, read receipts on
  or off, the invite code to show, a backup to write out, and deletion with
  a countdown to change one's mind in.

### Chats

- The chat list with search, an archive, and unread counts; muted chats
  stay quiet.
- New chats with known contacts, groups made from a page of their own with
  members added and the count shown, and contact requests to accept or
  block.
- Contacts blocked from the request or from their page, and a list of who
  is blocked under Privacy.
- A line where the reader left off, and the view held still while a menu is
  up or a message lands mid-chat.

### Messages

- Text with Markdown rendering, long messages opened on a page of their
  own, and tracking parameters cleaned off links.
- Pictures sent at a chosen quality, files, voice messages, and photos or
  video taken in the app.
- Reactions, editing, deleting for oneself or for everyone, quotes, and
  a period after which old messages go.
- A chat's pictures, sounds, files and apps on pages of their own, with
  Show in chat, Save and Delete; pictures and videos save to the gallery
  folders, everything else to Downloads.

### Apps

- webxdc apps run inside the app: sent from the attach tray, taken from the
  store page, with status updates flowing both ways and a way for an app to
  hand a file to the chat or to the reader. Off until switched on in
  Settings.

### The phone

- One of the phone's share targets, so a picture or a file from anywhere
  can be sent to a chat.
- Notifications for new messages, which can be switched off.
- A cover that draws the people in the chat list and the unread count
  while it counts.
- The core's connections stop while the phone has no network and resume
  when it is back.
- Every language Sailfish OS ships in: 39 catalogs.

### Known limitations

- **Harbour will reject this package.** The bundled `deltachat-rpc-server`
  is a second ELF executable at `/usr/libexec/harbour-piirit/`, and Jolla's
  validator allows an executable at `/usr/bin/<name>` only. This is
  structural rather than a bug (`docs/HARBOUR.md`, "The open blockers"):
  the in-process alternative needs a newer Rust than the SDK ships, and
  the C library that would fit the rule is being retired upstream. This
  release is submitted as the opening of that conversation with Jolla.
- Chats over plain email or non-chatmail servers are out of scope.
- No voice or video calls.
- Avatars on message bubbles and a free choice of emoji for a reaction are
  not there yet; the newer webxdc calls (`importFiles`, realtime channels)
  are not offered.

### Installing

The device RPM below is `harbour-piirit-1.0.0-1.aarch64.rpm`. Until it is
in the store it installs the way any sideloaded package does, from the
phone's terminal in developer mode:

```
pkcon install-local harbour-piirit-1.0.0-1.aarch64.rpm
```
