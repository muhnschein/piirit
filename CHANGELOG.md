# Changelog

What each release of Piirit brings, newest first. The heading of an entry
is the version and the day it was cut; the section under it is the text
of the GitHub release, which `scripts/release-notes.sh` cuts out of this
file when the release is made (`.github/workflows/rpm.yml`;
docs/BUILDING.md, "Cutting a release").

## Unreleased

- "Delete messages from device" in Settings keeps the period you pick; it no longer goes back to "Never".
- Emoji in the chat list, search results and the reply bar show as pictures, as they do in messages, with their colours toned down so the list stays calm.

## 2.2.0 — 2026-10-23

- **Swipe through a chat's pictures and videos**, in the style of Jolla's Gallery. A tap on a picture or video, in a chat or in a contact's or group's Gallery, opens it full screen; swipe sideways for the next one, through every picture, GIF and video of that chat, and swipe up or down to close, or tap the X in the top right. From a chat newer ones are to the right, as they are further down the chat; from the Gallery the order is the grid's, newest first. The top line says who sent it and the day; at the bottom Share hands it to another app and Save to device copies it to Downloads. A tap fades all of that away and another brings it back. A video waits for its play button and goes full screen once it plays, with a seek bar under it when the controls are up; a double tap skips 10 seconds on, and swiping away stops it. A double tap or a pinch zooms a picture in, a drag then pans it, and a tap zooms it out again. The viewer turns with the phone, so landscape pictures and videos fill the screen. Pictures and videos not downloaded yet are there too: their slide says what their message says (such as "Image – 2.4 MiB") and has a Download button, and once downloaded the picture takes the slide's place. In the Gallery grid they show the download arrow and their size.
- **Contact requests stand out.** A chat from someone you haven't written to carries a "Request" tag in the chat list. Opening it shows the messages with Accept and Block where the message field would be, or Accept and Delete for a group you were added to; accepting brings the field back, and blocking asks first. Reading a request sends no read receipt.
- **Photo and video quick actions.** The cover's quick actions can open the camera for a photo, a video, a selfie or a selfie video; once it is taken, pick the chat to send it to. The chat opens with it ready for a caption, or, with "Send right away" turned on for that action, it goes as soon as the chat is picked. Turning that on asks first, since a chat tapped by mistake gets it too, and it is off again whenever the action is chosen anew. The choice of action is Silica's own drop-down again, which lists the kinds on a page of their own.
- **A camera like the system's.** The in-app camera now has the system camera's layout and controls: pinch to zoom, a ring per back camera on phones with more than one (labelled 1.0, 0.5 and so on, as in the system camera), front/back switch, flash and self-timer, white balance, exposure, grid, and a focus ring where you tap. It turns to landscape with the phone.
- The QR scanner can zoom with a pinch and light the torch, which sits beside the invite link button.
- Bundles deltachat-rpc-server 2.65.0 (was 2.62.0). When someone unpins a message from another Delta Chat app, the chat now shows a line saying so, as it already did for pinning; that line makes no notification and doesn't move the chat up the list.
- Enter in the chat search closes the keyboard and keeps the search.
- A tap on a contact's or a group's picture shows it full screen. A group's picture is changed from the pull-down menu.
- **Smaller videos.** A picked video close to the relay's recommended size is made smaller as soon as it is picked, with progress on the attachment bar, and sent when you send it; with media quality set to worse, every picked video is. The bar says how much smaller it went, such as 50 MB → 6 MB, and the size warning weighs the smaller video, saying so when even that is too big. Stopping it, or a result that came out no smaller, sends the video as it was picked.
- **Voice calls are no longer experimental**: on for everybody, no switch to turn them on. Settings → Notifications → Calls still decides whether a call rings.
- A ringing call plays the ringtone rather than a short beep, and other apps' music pauses as soon as it rings, as it does for a phone call. It plays on again if you don't answer.
- A call that rings while the screen is off takes the lock screen away, so the call screen stays up.
- A call declined just as the last call's screen was closing still says so.
- **Video calls.** Call with the camera on from a contact's Calls page, or switch it on in any call; their picture fills the screen and yours sits in the top right corner. The button on your picture turns the camera, a tap puts the lines and switches away, and a spinner shows while a picture is on its way.
- **Earpiece and loudspeaker.** A voice call is on the earpiece, as a phone call is, and a video call on the loudspeaker; the switch on the call screen goes between the two, with your microphone off for about a second while it does. The volume keys set the call's volume, as they do in a phone call. A headset or a Bluetooth device takes the sound by itself. Nothing stays behind: should the app close or crash mid-call, the phone's sound goes back to where it was.
- **The cover shows a call** while one is up: who, with their picture behind, how long, whether your microphone is off or your camera paused, and one quick action to switch the microphone off and on.
- Emoji such as ❤ ☺ ⭐ ✅ ✨ ❌ ⚡ ☕ ☀ ✌ in a message are drawn as Twemoji like the rest, rather than in Sailfish's own glyph beside the pictures. Signs such as ©, ™ and arrows are still text. A 👍 picked from the emoji picker is the same reaction as the quick row's.
- A message or a quote that begins with an emoji no longer shows it twice, as its picture and then Sailfish's own glyph.
- Faces on the cover no longer turn into white squares after the app has been in the background.
- **Faster all round.** The chat list moves a chat to its new place rather than being rebuilt, the open chat and its message rows are quicker to build, Markdown and lines full of links are drawn in one pass, and the core's events are filtered and batched. In the background the app stops its cameras, call clocks and webxdc polling, and photos taken, recoded videos and QR images no longer pile up in the cache.
- Making a video smaller uses half the phone's cores at a lower priority, so the rest of the phone stays responsive meanwhile.
- A received video in an unusual MP4 layout no longer hangs the app, and a full message whose HTML has an emoji right after a "<" no longer stays loading. A webxdc app whose page holds certain non-English letters, and an unusual quota report from a relay, no longer make them fail.
- **Reproducible builds.** Each release RPM from this one on rebuilds from its tag to the same file, byte for byte. `scripts/verify-release.sh` checks a release for you (docs/VERIFYING.md).

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