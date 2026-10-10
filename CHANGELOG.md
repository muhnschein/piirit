# Changelog

What each release of Piirit brings, newest first. The heading of an entry
is the version and the day it was cut; the section under it is the text
of the GitHub release, which `scripts/release-notes.sh` cuts out of this
file when the release is made (`.github/workflows/rpm.yml`;
docs/BUILDING.md, "Cutting a release").

## Unreleased

- Bundles deltachat-rpc-server 2.63.0 (was 2.62.0).
- Enter in the chat search closes the keyboard and keeps the search.
- **Smaller videos.** A picked video close to the relay's recommended size is made smaller before it is sent, with progress and a way to stop it; with media quality set to worse, every picked video is.

## 2.1.0 — 2026-10-08

- **Unread filter.** Search and Unread pills at the top of the chat list; Unread shows only chats with unread messages.
- **Any emoji as a reaction**, drawn as Twemoji, and emoji in messages too; "Use Twemoji" in Settings turns the pictures off. Faster emoji picker.
- **Onboarding on several relays.** A new profile picks a relay automatically and adds more in the background; choosing one yourself is under Advanced.
- Scanning a relay's QR code offers to add that relay to the profile.
- **Chat descriptions.** Show and edit the description of a group or channel.
- Text the core writes itself is now translated into every language.
- Relay list brought in line with chatmail.at/relays.
- Contacts seen long ago say so.
- Notifications follow their chat: edits update them, deletions take them down.
- Turning on an experimental feature asks first.
- A large attachment gets a warning instead of being refused.
- Chat-list ages keep moving instead of freezing at "now".
- The scanner no longer re-locks its focus every couple of seconds.
- Avatars on the app cover no longer go flat.

## 2.0.0 — 2026-09-25

- Bundles deltachat-rpc-server 2.62.0 (was 2.60.0).
- **Multiple transports per profile.**
- **Second device.** Offer a profile to another device from Piirit.
- **More efficient voice messages.** Now sent as MP3 instead of FLAC: *much* smaller, and checked against attachment size limits.
- **Improved UI for contact blocking**
- Attachments save to `Downloads/Piirit` by default
- Part of a long-form message can be selected and copied
- **Customizable quick actions** for the app cover
- **Voice calls (experimental).** Place, answer and end calls; off by default, turn on in Settings
- "Enter sends the message" setting removed
- Reduced package size
- Manually reviewed English and German strings, removed most menu subtitles, refreshed non-EN/DE translations.

## 1.0.0 — 2026-09-18

The first release.