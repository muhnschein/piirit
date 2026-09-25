# Changelog

What each release of Piirit brings, newest first. The heading of an entry
is the version and the day it was cut; the section under it is the text
of the GitHub release, which `scripts/release-notes.sh` cuts out of this
file when the release is made (`.github/workflows/rpm.yml`;
docs/BUILDING.md, "Cutting a release").

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