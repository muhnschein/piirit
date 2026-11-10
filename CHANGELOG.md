# Changelog

What each release of Piirit brings, newest first. The heading of an entry
is the version and the day it was cut; the section under it is the text
of the GitHub release, which `scripts/release-notes.sh` cuts out of this
file when the release is made (`.github/workflows/rpm.yml`;
docs/BUILDING.md, "Cutting a release").

## Unreleased

- **Settings, one page per topic.** Settings now lists your profile and then Messages, Notifications, Privacy and security, Quick actions, Experimental and About, each on its own page. Blocked contacts sit under Privacy and security, and say how many there are. Voice-message transcription is set up right under its switch in Experimental. The new About page shows the versions of Piirit and the Delta Chat core, copies them for a bug report, links the source code and replays the intro slides; swiping past the last one returns to About.
- **Chats in common.** A contact's page lists, at the bottom, the chats you share with them: your chat with them and each group they are in. The heading says how many; past five, "Show all" opens the full list. Tap a chat to open it.
- Once you have a channel, the chat list gets Chats and Channels buttons beside Unread to show one kind at a time, alone or with Unread. The row of buttons scrolls sideways when it doesn't fit.
- The chat list's Search button shrinks to its magnifier when the buttons beside it wouldn't fit otherwise, such as with long words in your language or large text.
- If the Delta Chat core stops while Piirit waits for news, Piirit notices, says it is reconnecting and starts the core again. Before, it could miss that and keep showing chats that no longer updated until restarted.
- A notification for a voice message shows its text once it is ready: the same notification is updated, no new banner. With notifications set to hide message text, nothing changes.
- With voice messages turned into text, the chat list shows a voice message's text instead of just "Voice message", and so does a reply's quote of it and the reply bar while you answer one. Until the text is there, or with the feature off, they read as before.
- Voice messages can be turned into text on the phone itself (experimental, off by default; Settings, Experimental). Piirit downloads a speech model once; nothing you send or receive leaves the phone. Transcribe one voice message with a tap, or let Piirit do them on its own while charging (the default) or as soon as they arrive. The text shows under the voice message, can be copied from its menu, and a chat's Audio page can transcribe all its voice messages at once. It works in 13 languages so far, and the list says for which of them transcripts are likely to contain more mistakes; other languages can't be picked yet.
- Search finds voice messages by what is said in them, once they are turned into text, and lists them with the text messages as 🎤 and their text, as the chat list shows them.
- The cover shows everyone from the start; right after Piirit opened, some people could be missing from it until a new message came.
- When many messages arrive at once, such as after a while offline, the chat list catches up in one go instead of once per message, and the cover only reads your chats while you look at it, so the app stays quicker to respond.
- The page of a group with hundreds of members opens and slides back smoothly: member rows are made as you scroll to them, not all at once.
- Chats open and scroll more smoothly: each message builds and lays out only the parts it shows.
- The emoji picker opens and searches faster: each emoji is just its picture.
- The camera opens more smoothly: it starts once its page is on screen, on the side and in the mode asked, not while the page slides in. The same goes for the QR scanner opened from the cover.
- Long chats open sooner: Piirit loads one screen of messages before showing the chat, not fifty, and the rest right after.
- The camera shows it is busy while a photo is being saved.
- A second tap on the shutter while a photo is saved no longer takes another photo.
- When adding members to a group, people already in it are greyed out even if the group's member list arrives after the picker opened.
- Tapping a message or missed-call notification opens its chat again after you have opened Archived chats or switched profiles. Before, such taps did nothing until Piirit was restarted.
- Emoji next to words have room on either side, in messages, chat names and previews alike, so they no longer run into the text.
- Bundles deltachat-rpc-server 2.69.0 (was 2.65.0). Search finds words anywhere in a long message, not just in the part the chat shows, and resending a long message sends all of it; this holds for messages sent or received from now on. A webxdc app whose icon is broken now shows the standard webxdc icon instead of a plain grid. On a profile with several relays, the profiles list and the profile page now name the address of the relay it was first set up on, which Delta Chat now takes as the profile's own.
- A profile without a picture shows its own colour on the profile page, as in the profiles list, not the theme's.
- The big picture on the profile, contact and group pages shows its letter or emoji at a size that fits it, not list size.
- A chat whose name starts with an emoji shows that emoji as its picture, not a "?".
- Emoji are sharp at every size, in messages, reactions and the emoji picker.
- A tap on a reply's quote goes to the quoted message and lights it up, however far up the chat it is. A quote of a message in another chat, such as a private reply to a group message, opens that chat at it. If the original is gone, a note says so.
- A message of nothing but emoji, up to five of them, shows them big.
- Coming back to a chat from another page no longer leaves it blank or at the top when it had lost its place meanwhile.
- "Delete messages from device" in Settings keeps the period you pick; it no longer goes back to "Never".
- **Mute a chat for a while.** Mute in a chat's long-press menu in the chat list now asks for how long: 1 hour, 8 hours, 1 day, 7 days or forever, as in the other Delta Chat apps; Unmute there works at once. A contact's or group's page has a Notifications row that says On or Muted and offers the same choices, plus Unmute. In a group, a note says that replies to you still notify while Mentions is on in the notification settings.
- A setting changed twice in quick succession keeps the second choice; the first one could win before.
- Emoji in names show as pictures, as they do in messages, with their colours muted: in the chat list, the chat header, contact and group pages, search results, contact lists and calls. In message previews and the reply bar they are grey like the text, so lists stay calm.
- Search results show the text you searched for in bold, in the highlight colour, in every list you can search.
- The description under Settings → Notifications → Mentions says what the switch does: in a muted group, replies to your messages still notify. It no longer promises reactions, which never notify.
- **Join a channel** by scanning its QR code or pasting its invite link on the QR code page, as for a contact or a group. The channel opens right away; it is read-only, and new posts arrive in it from then on.
- **Make your own channel.** "New" in the chat list's pull-down menu opens a dialog to pick a new chat, group or channel, each with a line on what it is; back from the page you picked returns to it. A channel has a name and a picture but no members: from its page, "QR invite code" shows the code and link others scan to join it. Only you post in it.
- Turning the unread filter off no longer flashes "No chats yet" before the chats come back.
- After backing up a profile or handing it to a second device, swiping on to your chats ends there: swiping back no longer reopens the finished page.
- "Add profile" under the profiles list opens a dialog to pick creating a profile, restoring a backup or adding this phone as a second device, each with a line on what it means; back from the page you picked returns to it.
- **Share your location**, experimental as in Delta Chat: turn on Enable location streaming in Settings. Then share from the button beside the message field: once, or live for 30 minutes up to 24 hours. Sharing waits until the phone has found where it is, and says so when location is off in Settings. A message with a location shows just "Location": tap it to see the place on a map, the same map app as Delta Chat's, with each marker named after its sender. A chat's map is under Map on its contact or group page; all chats' locations are under All locations in the chat list's pull-down menu. The map opens framing every location of the last 24 hours, or says there are none. While you share live, Piirit keeps sending your position in the background; a line above the field shows the time left, warns when no position goes out, and stops sharing.

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