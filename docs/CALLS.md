# Calls

*How a call works in Piirit, how much of the phone's own call handling
an app can have, and what only a phone can still answer.*

**Voice calls are built, and experimental**: off until *Settings →
Advanced → Enable calls (experimental)* is switched on. Video calls are
not built. Nothing here has run on a phone yet -- every test in this tree
passes without one, and the questions at the end are the ones a source
tree cannot answer.

The shape of it: **the core does the call, Gecko does the media, and
Piirit is the glue.** The core places, rings, accepts and ends a call, and
times one out. The browser engine the platform already ships has the peer
connection, the codecs and the echo cancellation. Upstream's calls-webapp
is the page that drives them. What is written here is the page's host,
the call's screen, and the phone's side of a call: the ringtone, the
screen, the notification.

## What the core does

`deltachat-rpc-server` 2.62, the binary this package bundles
(`vendor/deltachat-rpc-server/SOURCE.md`), carries the whole call
protocol. `rust/deltachat-jsonrpc/tests/real_server.rs` pins the parts
of it the app reads against the binary itself.

- **Five methods.** `place_outgoing_call(account, chat, offer, has_video)`
  answers with the message id that *is* the call. `accept_incoming_call(
  account, message, answer)`. `end_call(account, message)` is decline,
  cancel and hang-up all three, and a second one is not an error.
  `call_info(account, message)` answers `{sdpOffer, hasVideo, state}`,
  the state tagged by `kind`: `Alerting`, `Active`, `Completed` with a
  `duration` in seconds, `Missed`, `Declined`, `Canceled`.
  `ice_servers(account)` answers a JSON *string* holding the TURN
  servers, which the bridge hands to the page as it is.
- **Four events**, and the one trap in them: `IncomingCall`,
  `IncomingCallAccepted`, `OutgoingCallAccepted` and `CallEnded` are
  serialised **snake_case** -- `msg_id`, `chat_id`, `place_call_info`,
  `has_video`, `from_this_device`, `accept_call_info` -- where the rest of
  the stream is camelCase. A reader asking for `msgId` gets nothing and
  says nothing. `calls.rs` reads them as they are, and the tests feed
  them in that shape.
- **The chat is the signalling channel.** A call is a `Viewtype::Call`
  message with the offer in a header; accepting and ending are hidden
  messages quoting it. The core never reads the SDP, which is what makes
  the media the client's problem and nothing else.
- **A call rings for 120 seconds**, then goes stale at both ends. The
  core's own timer ends it: incoming as Missed, outgoing as Declined.
- **A ringing call raises no `IncomingMsg`**, and one that rings out
  says only `MsgsChanged` and `CallEnded`. A missed call's notification
  is the client's to raise; the chat list's message notifications never
  see one.
- **One-to-one only**, and encrypted: the core refuses a group ("Can
  only place calls in single chats") and Saved messages ("Cannot call
  self"), and a chat without the other end's key cannot send.
- **`who_can_call_me`**, per device, not synced: 0 everybody, 1
  contacts (the default), 2 nobody. A call from somebody who may not ring
  is still stored, and can still be answered from its chat.

## How a call works here

### The row

A call is drawn as one (`MessageDelegate.qml`): what kind, or what
became of it -- *Missed call*, *Declined call*, *Canceled call* -- and
how long it lasted, in Delta Chat's own words so its translations apply.
The row reads the call's state (`calls::summary`, filled in by
`chat.rs` the way a webxdc row is); the core's own text for a call is an
English sentence, and is shown only when the core will not say what
state a call is in. A tap on a call still ringing here takes it up; a tap
on any other calls back -- deltachat-android's rule.

The drawing is not behind the switch -- it is what a call from another
client looks like whether or not calls are on -- but the tap is: with
calls off, a call's row does nothing.

### The call

One `Call` object (`calls.rs`) for the whole window, held by
`CallCenter.qml`, because a call belongs to no page: one comes in
whatever is on screen, and a page the reader leaves is not a call they
have hung up. It moves through `ringing`, `starting`, `calling`,
`connecting`, `connected`, `reconnecting` and `ended`, and says why a
call ended: hung up here, declined here, ended there, answered on
another device, missed, failed. One call at a time, as both reference
clients have it: a second rings nowhere and is a missed call once the
core gives up on it.

`CallPage.qml` shows it: who, and how it stands, in the phone's language;
Decline and Answer while it rings; then the call's page. The page cannot
be swiped away while a call is up -- that would take the call's sound
with it -- and a tap on a notification or a cover quick action brings
the call back to the front rather than popping it.

### The page

The media is upstream's [`calls-webapp`][calls-webapp] v0.12.1: WebRTC
"for integration into Delta Chat clients", in its own words, and the page
deltachat-android ran its calls in until it went native in v2.47. It is
one self-contained `.html` release asset, committed
(`vendor/calls-webapp/`, `scripts/fetch-calls-webapp.sh`, checked by
`ci/vendor-check.sh`) and compiled into the app.

`call_host.rs` serves it from a loopback address of its own -- the
arrangement `webxdc_host.rs` is: a port the kernel picks, a `Host:`
that must be the one the page was given, everything that reaches the
core behind an unguessable token, nothing served once the call is over.
The page loads a classic `calls.js` before its own code, and the host
answers with Piirit's bridge (`rust/piirit-shim/src/calls.js`). The
bridge is static and holds no secret: the token is in the page's own
address (`?key=`), which the app hands its own WebView and nothing else,
so a page elsewhere that includes the script -- which a browser allows
across origins -- finds no key and reaches nothing. What the bridge does:

- `startCall(offer)`, `acceptCall(answer)` and `endCall()` are requests
  back to the host, which places, accepts or ends the call;
- `getIceServers()` asks the host for the core's servers -- asked for,
  not written into the script, because a TURN server's credentials are
  the account's -- and `getAvatar()` names the other end's picture,
  served from the host;
- the other end's answer arrives as the page's own hash command,
  `#onAnswer=`, which the bridge collects with a long poll;
- the page says nothing about how its connection is doing, so the bridge
  watches the peer connection and reports ICE's own states -- which is
  where *Connected* and the clock come from, and what ends a call whose
  connection has failed;
- Gecko before 126 has no `RTCIceCandidate.type`, which the page reads
  to send its offer as soon as it has a relay candidate; without it the
  offer would wait for gathering to finish. The bridge reads the type
  out of the candidate line.

The page is opened on `#startCall` or `#acceptCall=<offer>`, with
`?disableVideoCompletely`: audio only, no camera asked for. Its policy
keeps every fetch on its own origin; the peer connection is not a fetch,
and is not governed by one. The offer goes to the core only once the page
has gathered enough ICE, and a call hung up while the core was still
placing it is ended as soon as it lands. Nothing orders the core's
answer to "place this" against the other end's answer to the call, so
an answer that arrives before the call knows its message is kept and
applied once it does (`tests/call_early_answer.rs`).

The same page speaks the protocol the native Android and iOS clients
reimplemented -- the same negotiated data channels, the same "enough ICE,
then trickle" -- so a call from here reaches either of them.

## What the phone gives a call

What a phone call has that an app's call cannot is the system call screen,
the call history and the lock-screen call window. On Sailfish those are
voicecall's, and the way in is a voicecall provider plugin -- a `.so` in
`/usr/lib/voicecall/plugins`, outside anything Harbour lets a package
install, running unconfined inside voicecall-manager, with the `Phone`
permission Harbour does not allow. The apps that have done it (a WhatsApp
client, RooTelegram) ship outside Harbour. `Sailfish.Telephony` is SIM
selection and nothing else. So none of that is here.

What is here is the rest of it:

| | How | Allowed |
|---|---|---|
| **Ringtone** | ngfd, the daemon that makes every sound the phone makes, plays `voip_ringtone`: the phone's own tone for a call that is not a phone call, at the ringing volume, silenced by the silent profile, repeating until stopped | ngfd's system-bus interface, which sailjail opens to every app; not a listed Harbour API |
| **Screen and call state** | mce is told `ringing`, then `active`, then `none`: it lights the screen for a ringing call and treats the phone as being in one. It forgets by itself if the app goes | mce's request interface, open to every app; not a listed Harbour API |
| **Lock screen** | a Critical notification with Decline and Answer on it, and the app brought forward | `Nemo.Notifications` |
| **Missed calls** | left in the notification area in the phone's own category for one, `x-nemo.call.missed`, with Call back on it | `Nemo.Notifications` |
| **Staying awake** | the CPU kept up while a call is up: a phone that suspends takes the call's sound with it | `Nemo.KeepAlive` |
| **At the ear** | the proximity sensor puts a black screen that takes no touch over the call while it is held there -- an app cannot switch the display off | `QtSensors` |
| **In the background** | the call's `WebView` is held active once its page is up: an inactive view is a hidden document, and the engine pauses a hidden document's media | `Sailfish.WebView` |
| **Microphone** | granted to the call's own page, for the session, before it asks -- the engine's prompt would otherwise open every call, since the origin is a new port each time. Never engine-wide: that would grant it to every webxdc app too | `Sailfish.WebEngine` |

Left out on purpose:

- **The earpiece.** ohm's `Route.Manager.Prefer("earpiece")` is open to an
  app with the Audio permission, and would make a voice call sound like
  one. It is also kept after the app has gone: a crash mid-call would
  leave every sound the phone makes on the earpiece. Calls are on the
  speaker until that can be made safe on a device.
- **Being a call to the audio policy.** The `call` resource class is
  voicecall's. `libaudioresource`, which Harbour allows, would put the
  app's streams in the `player` class -- pausing music, and keeping the
  call from being corked while something else plays -- and would set the
  whole process's `media.role` doing it. Whether Gecko's streams need it
  is the third question below.
- **`Nemo.Ngf`**, which would ring from QML: not on Harbour's list, hence
  the bus.

## What only a phone can answer

Ranked by what would sink the feature. None of them needs another line
of Piirit: two phones, a calls-enabled build, and an afternoon.

1. **Does a peer connection complete on the device, with the
   microphone?** WebRTC is built into the engine and on by default, and
   camera frames are known to reach a page ([blue-tinted][blue-tint], on
   4.4 and 4.5). A voice call is a lighter page than the video conference
   that has been seen to crash the browser ([4.5.0.19][conf-crash]), and
   the engine is newer than either report. That is a reason to try, not
   an answer.
2. **Does the other end's voice keep playing with the app in the
   background?** The view is held active for it; that this is enough is
   the claim.
3. **Is the call corked?** Gecko's streams carry no media role, which
   puts them in the audio policy's catch-all class -- corked while a
   player, an event or a call holds audio. If a call goes silent when
   music is playing, or when the microphone starts, `libaudioresource`
   is the fix above.
4. **Does `voip_ringtone` exist in the phone's event set**, and does mce
   blank the screen at the ear with the call on the speaker?
5. **What it sounds like.** Echo on speakerphone, and what a call costs
   in battery.

## What would not change

- **A call rings only while the app is running.** No push notification
  service is open to a third-party client (`docs/PROJECT.md`), so a call
  is heard only if the app is alive to receive its message, minimised
  included, inside the core's 120 seconds. Anything else is a missed
  call's row.
- **Harbour.** Calls add no executable, no library off the list and no
  permission that was not already asked for. They add two system-bus
  services Harbour does not list -- ngfd and mce -- which its validator
  cannot see and its QA may or may not accept; both are one
  `DBusInterface` each in `CallCenter.qml`, and a call works without
  either. And they promote the browser engine, a separate package on
  this platform, from a webxdc feature to the load-bearing part of a
  headline one.

## Next

1. **The phone questions above**, before anything else.
2. **Video**: `?noOutgoingVideoInitially`, the camera granted as the
   microphone is, and the page's own camera button.
3. **The earpiece**, once it can be released even when the app is not
   there to release it.
4. **The chat list's preview** of a call is still the core's English
   sentence (`📞 Outgoing audio call`). The reference clients hand the
   core translations of its call strings (`set_stock_strings`); Piirit
   hands it none of any kind yet.

## Sources

Read rather than remembered, at these versions:

- [`chatmail/core` v2.62.0][core] -- `src/calls.rs`,
  `deltachat-jsonrpc/src/api.rs`, `.../api/types/calls.rs`,
  `.../api/types/events.rs`.
- [`deltachat/calls-webapp` v0.12.1][calls-webapp] -- `src/lib/calls.ts`,
  `src/vite-env.d.ts`, `calls.js`, its CI and its licence.
- [`deltachat/deltachat-android`][android] -- v2.43.0's `CallActivity`
  and `CallUtil`, the last WebView-based client; master's
  `CallCoordinator`, `CallItemView` and strings, for the behaviour and
  the words.
- [`deltachat/deltachat-ios`][ios] -- `CallManager`, `CallViewController`.
- Sailfish OS: [`ngfd`][ngfd] and [`mce`][mce] (their D-Bus interfaces),
  [`lipstick`][lipstick] (`notificationfeedbackplayer.cpp`),
  [`nemo-qml-plugin-notifications`][notifications],
  [`sailjail-permissions`][sailjail] (`Base`, `Audio`, `Phone`),
  [`voicecall`][voicecall], [`ohm-plugins-misc`][ohm] (the route
  manager), [`embedlite-components`][embedlite]
  (`EmbedLiteWebrtcUI.js`, `ContentPermissionManager.js`),
  [`sailfish-components-webview`][webview] (`WebView.qml`) and
  `gecko-dev`'s Sailfish patches.
- The two device reports linked above, still the only public evidence
  either way about WebRTC on a Sailfish phone.

[core]: https://github.com/chatmail/core
[calls-webapp]: https://github.com/deltachat/calls-webapp
[android]: https://github.com/deltachat/deltachat-android
[ios]: https://github.com/deltachat/deltachat-ios
[ngfd]: https://github.com/sailfishos/ngfd
[mce]: https://github.com/sailfishos/mce
[lipstick]: https://github.com/sailfishos/lipstick
[notifications]: https://github.com/sailfishos/nemo-qml-plugin-notifications
[sailjail]: https://github.com/sailfishos/sailjail-permissions
[voicecall]: https://github.com/sailfishos/voicecall
[ohm]: https://github.com/sailfishos/ohm-plugins-misc
[embedlite]: https://github.com/sailfishos/embedlite-components
[webview]: https://github.com/sailfishos/sailfish-components-webview
[blue-tint]: https://forum.sailfishos.org/t/browser-webview-camera-video-has-a-blue-tint/14878
[conf-crash]: https://forum.sailfishos.org/t/xperia-x-4-5-0-19-browser-crashes-when-connecting-to-video-conference/15456
