# Changelog

What each release of Piirit brings, newest first. The heading of an entry
is the version and the day it was cut; the section under it is the text
of the GitHub release, which `scripts/release-notes.sh` cuts out of this
file when the release is made (`.github/workflows/rpm.yml`;
docs/BUILDING.md, "Cutting a release").

## Unreleased

- **Photo and video quick actions.** The cover's quick actions can open the camera for a photo, a video, a selfie or a selfie video; once it is taken, pick the chat to send it to. The chat opens with it ready for a caption, or, with "Send right away" turned on for that action, it goes as soon as the chat is picked. Turning that on asks first, since a chat tapped by mistake gets it too, and it is off again whenever the action is chosen anew. The choice of action is Silica's own drop-down again, which lists the kinds on a page of their own.
- **A camera like the system's.** The in-app camera now has the system camera's layout and controls: pinch to zoom, a ring per back camera on phones with more than one (labelled 1.0, 0.5 and so on, as in the system camera), front/back switch, flash and self-timer, white balance, exposure, grid, and a focus ring where you tap. It turns to landscape with the phone.
- The QR scanner can zoom (pinch, or the 1×/2×/4× button), light the torch, and switch cameras.
- Bundles deltachat-rpc-server 2.63.0 (was 2.62.0).
- Enter in the chat search closes the keyboard and keeps the search.
- A tap on a contact's or a group's picture shows it full screen. A group's picture is changed from the pull-down menu.
- **Smaller videos.** A picked video close to the relay's recommended size is made smaller as soon as it is picked, with progress on the attachment bar, and sent when you send it; with media quality set to worse, every picked video is. The bar says how much smaller it went, such as 50 MB → 6 MB, and the size warning weighs the smaller video, saying so when even that is too big. Stopping it, or a result that came out no smaller, sends the video as it was picked.
- **Voice calls are no longer experimental**: on for everybody, no switch to turn them on. Settings → Notifications → Calls still decides whether a call rings.
- A ringing call plays the ringtone rather than a short beep.
- A call that rings while the screen is off takes the lock screen away, so the call screen stays up.
- A call declined just as the last call's screen was closing still says so.
- **Video calls.** Call with the camera on from a contact's Calls page, or switch it on in any call; their picture fills the screen and yours sits in the top right corner. The button on your picture turns the camera, a tap puts the lines and switches away, and a spinner shows while a picture is on its way.
- **Earpiece and loudspeaker.** A voice call is on the earpiece, as a phone call is, and a video call on the loudspeaker; the switch on the call screen goes between the two. A headset or a Bluetooth device takes the sound by itself. Nothing stays behind: should the app close or crash mid-call, the phone's sound goes back to where it was.
- **The cover shows a call** while one is up: who, with their picture behind, how long, whether your microphone is off or your camera paused, and one quick action to switch the microphone off and on.

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