# Calls

*Whether postivene can carry a voice or video call, and what it would
take.*

Nothing here is built. This document is the survey that was missing when
the answer to "why no calls?" was "for the time being" -- what the
bundled core already does, where the media would have to come from, and
which of the remaining questions a source tree can answer and which need
a phone.

The short of it: **the protocol half is already in the package, and the
media half exists on the platform but has never been proved on a
device.** Audio-only is a plausible first release; video is the half
that could still turn out not to work.

## What the core already does

`deltachat-rpc-server` 2.60, the binary this package already bundles
(`vendor/deltachat-rpc-server/SOURCE.md`), carries the whole call
protocol. Nothing about signalling, ringing, timeouts or ICE discovery
would be written here -- which is the same bargain as the rest of the
app, and the reason calls are worth considering at all.

- **Five methods.** `place_outgoing_call(account_id, chat_id,
  place_call_info, has_video)`, which returns the message id that *is*
  the call id; `accept_incoming_call(account_id, msg_id,
  accept_call_info)`; `end_call(account_id, msg_id)`, which is decline,
  cancel and hang up all three; `call_info(account_id, msg_id)`, which
  answers the offer, whether it started with video, and a state --
  Alerting, Active, Completed with a duration, Missed, Declined,
  Canceled; and `ice_servers(account_id)`.
- **Four events.** `IncomingCall`, `IncomingCallAccepted`,
  `OutgoingCallAccepted`, `CallEnded`. All four already arrive here and
  are dropped on the floor: every event reaches QML as `core_event`
  (`rust/postivene-shim/src/core.rs:676`), and nothing listens for
  these.
- **The chat is the signalling channel.** A call is a message --
  `Viewtype::Call`, with the SDP offer in its `WebrtcRoom` param -- and
  the acceptance is a hidden system message quoting it, carrying the
  answer in `WebrtcAccepted`. The core never looks inside either
  string: `place_call_info` and `accept_call_info` are opaque to it,
  which is exactly why the media stack is the client's problem and the
  protocol is not.
- **ICE comes from the relay.** A chatmail relay advertises a TURN
  server with time-limited credentials over IMAP metadata; where it
  does not, the core falls back to `nine.testrun.org` for STUN and
  `turn.delta.chat` with a public credential. Either way the core
  resolves the names to addresses before handing them over, because the
  desktop client cannot resolve them itself.
- **A call rings for 120 seconds** and then goes stale on both ends, on
  the caller's side too. Whatever rings here has that long to be
  answered.
- **Calls are one-to-one.** `place_outgoing_call` refuses a group and
  refuses "Saved messages".
- **There is a `who_can_call_me` config** -- everybody, contacts only
  (the default), or nobody -- which is a settings row and nothing more.

One thing to know before reading any of the above into code: the four
call events are serialised **snake_case** -- `msg_id`, `chat_id`,
`place_call_info`, `has_video`, `from_this_device`, `accept_call_info` --
where the rest of the event stream is camelCase. The variants simply
lack the `rename_all` attribute their neighbours carry (checked against
v2.60.0). A reader that asks for `msgId`, as the rest of this tree
rightly does, gets nothing and reports no error.

## What the app does with a call today

It shows it as a sentence. Somebody calling from deltachat-android
lands here as a `Viewtype::Call` message whose text is the core's own
"Incoming call" stock string, drawn as a plain bubble because the
message has text and no file. It cannot be answered, it cannot be
declined, and it never turns into "Missed call", because the text is
rewritten in the database when the call ends and nothing here is
listening for the event that says so.

That is worth fixing whether or not the rest of this document is ever
acted on. It costs no media stack at all.

## Where the media would come from

The core hands over an SDP offer and expects an answer. Producing one
means a WebRTC stack: ICE, DTLS-SRTP, Opus, a jitter buffer, echo
cancellation, and for video an encoder. There are two places to get
one, and only one of them is realistic.

### The browser engine, which already has it

Sailfish's embedded Gecko does WebRTC, and not vestigially:

- `embedlite-components`, the engine's own chrome, ships
  `jscomps/EmbedLiteWebrtcUI.js` (Open Mobile Platform, 2021, still in
  HEAD). It observes `getUserMedia:request` and `PeerConnection:request`,
  enumerates the audio and video devices, allows peer connections
  outright, and asks the embedding application about devices over an
  `embed:webrtcrequest` message.
- `sailfish-components-webview` answers that message. Its
  `import/popups/PopupOpener.qml` maps the topic to
  `WebrtcPermissionDialog.qml`, and the `WebView` this tree already uses
  wires that opener up itself. The camera-and-microphone prompt is
  therefore *already there* in a third-party app, in the platform's own
  shape, translated, with a remember-this checkbox.

So the stack is on the phone, it is maintained by the platform vendor,
and it comes with the acoustic echo cancellation and the codec tuning
that are the genuinely hard parts. What postivene would add is a page
to run in it and a way for that page to reach the core.

Both of those this tree has already built once. `webxdc_host.rs` serves
one page from a loopback address the kernel picks a port for, refuses a
request whose `Host:` is not the address it handed out, and puts the
account-touching API behind an unguessable token; `WebxdcPage.qml`
points a `WebView` at it and keeps it from wandering off. A call page
is the same arrangement with a different API behind the token.
`harbour-postivene.desktop:151` already asks for `Audio`, `Microphone`,
`Camera` and `WebView`.

### The page itself

Upstream maintains one: [`deltachat/calls-webapp`][calls-webapp], the
page deltachat-desktop, -android and -ios all run. It is GPL-3.0, which
this project's own terms accept, and it builds to a single optimised
`.html` attached to its GitHub releases -- so it can be vendored the way
the rpc-server binary is vendored, fetched and checksum-pinned by a
script, with a `SOURCE.md` beside it, and no node toolchain anywhere
near this build.

Its integration surface is five functions on `window.calls`:
`startCall(offer)`, `acceptCall(answer)`, `endCall()`, `getIceServers()`
returning the JSON the core's `ice_servers` already returns, and
`getAvatar()`. It is driven by the URL hash -- `#startCall`,
`#offerIncomingCall=PAYLOAD`, `#acceptCall=PAYLOAD`,
`#onAnswer=PAYLOAD`, each payload base64 then URL-encoded, the `=`
padding included, because `=` is also the delimiter. Audio-only is a
query string: `?disableVideoCompletely`, or
`?noOutgoingVideoInitially` for a call that starts muted but can turn
the camera on.

Writing a page instead is possible and roughly 500 lines by upstream's
measure, but it buys nothing except a second implementation to keep
interoperating with the first.

One thing does *not* carry over from the webxdc host: its
content-security-policy. `default-src 'self'` is exactly right for an
app that must not phone home and exactly wrong for a page whose whole
job is to reach a TURN server. Whether Gecko applies `connect-src` to
ICE at all is a question for the engine on the phone rather than for
the specification, and the call page needs a policy written for it
either way.

### Why not a native stack

Because it is a project rather than a feature, and this tree is the
wrong place for it:

- **Nothing to link against.** `ci/harbour/allowed_libraries.conf` has
  no GStreamer, no libnice, no libopus, no libsrtp. Everything would
  have to be statically linked into the shim, which Harbour permits and
  cannot see -- but that means owning the build of a media stack for two
  device architectures.
- **The toolchain floor.** The device builds against Rust 1.75
  (`docs/BUILDING.md`); the Rust WebRTC crates do not.
- **The hard parts are the invisible ones.** Echo cancellation, gain
  control, jitter buffering and packet-loss concealment are what make a
  call sound like a call, and Gecko has all four already tuned for this
  hardware. Hand-rolling them is how a call ends up technically
  connected and unusable.
- **Video would need the hardware encoder**, which on this platform
  means the droid GStreamer plugins, which are not reachable from a
  Harbour package at all.

## The shape it would take here

Roughly, and in the order the pieces depend on each other:

1. **`calls.rs` in the shim** -- a model over the five methods, and
   typed signals for the four events off `relay()`, the way
   `configure_progress` is already lifted out of the generic
   `core_event` stream. Mind the snake_case keys.
2. **`call_host.rs`** -- `webxdc_host.rs`'s arrangement a second time:
   the page, a `/call-api/<token>/` the bridge talks to, its own
   policy. The bridge is small -- five calls out, and one poll or long
   poll back for "the other end answered" and "the call ended", which
   is the same shape as the webxdc update poll.
3. **A call row for `Viewtype::Call`** -- incoming, outgoing, missed,
   declined, and a duration when there was one, following the state the
   core keeps rather than the message text.
4. **`CallPage.qml`** -- the `WebView`, Silica chrome over it for
   answer, decline and hang up, and the same `Sailfish.WebView`-is-not-
   always-there caution the webxdc pages carry.
5. **Ringing** -- `Notifier.qml` extended. Which lipstick notification
   category a Harbour application may use for something that rings is
   unanswered here; `x-nemo.messaging.im`, what the app uses now, is
   the wrong one.
6. **Staying awake and lit** -- `Nemo.KeepAlive` (allowed, 1.2) to stop
   the display blanking mid-call, and QtSensors' proximity sensor to
   blank it when the phone is at an ear.
7. **A settings row** for `who_can_call_me`, and an experimental gate
   like the webxdc one for as long as it needs one.
8. **Tests** -- `fake_core_server.rs` gains the five methods and can
   emit the four events; a `qml_calls.rs` in the shape of
   `qml_webxdc_page.rs`; a real-core round trip for placing and ending
   a call, which is testable without any media at all.

## What a source tree cannot answer

Ranked by what would sink the feature, and every one of them is a
phone-shaped question:

1. **Does a peer connection actually complete on the device, with the
   camera and the microphone?** The permission plumbing being present
   is not the same as the pipeline working. What is known: camera
   frames do reach a page -- there is a standing report of
   [video from the camera arriving blue-tinted][blue-tint] on
   `meet.jit.si` in `Sailfish.WebView`, reproducible on 4.4 and 4.5,
   which is a colour-format mismatch and therefore evidence that
   capture works at all. Also known: [the browser crashing when joining
   a video conference][conf-crash] on 4.5.0.19. Jitsi is a far heavier
   page than a two-party call, and the baseline here is a newer engine
   than either report, but nothing about that is an argument -- it is a
   reason to run the test.
2. **Where the audio comes out.** Routing to the earpiece rather than
   the loudspeaker is a policy decision this app does not make, and
   there is no telephony integration open to a Harbour package.
   Speakerphone by default is survivable and is not good.
3. **Whether the engine keeps running when the app is not in front.**
   Silica's `WebView` decides when the engine renders from whether the
   application is active -- this tree already learnt not to override
   that (`docs/PROJECT.md`) -- and a call whose audio stops when the
   reader looks at something else is not a call.
4. **What it costs.** Software video encode on this hardware, in
   frames and in battery.

None of these needs a written line of postivene: two phones, the
upstream page served off a laptop, and an afternoon.

## What would not change

Two limits are structural, and no amount of work here moves them:

- **Calls ring only while the app is running.** There is no push
  notification service open to a third-party client -- the reason is in
  `docs/PROJECT.md`'s non-goals -- so an incoming call is heard only if
  the app is alive to receive the message, minimised included, inside
  the core's 120-second window. Everything else is a missed call, which
  at least becomes a legible row once step 0 below is done.
- **Harbour.** Calls break no rule that is not already broken: no
  second executable, no library off the list, no permission that is not
  already asked for. What they do is promote the browser engine from an
  off-by-default experiment to the load-bearing dependency of a
  headline feature, on a platform where that engine ships as a separate
  package. That is a decision about what this app is, not a packaging
  question.

## Ship order

0. **Call messages, and decline.** Render `Viewtype::Call` properly and
   wire `end_call` to it. No media, no `WebView`, no new dependency --
   and it fixes what is already wrong for every reader whose contacts
   use another client.
1. **Audio only**, behind the experimental gate. `?disableVideoCompletely`
   is one query parameter, and it steps around the camera colour
   question and the encode cost in one move.
2. **Video**, once audio has held up on a real phone for a while.

## Sources

Everything above was read rather than remembered, at these versions:

- [`chatmail/core` v2.60.0][core] -- `src/calls.rs`,
  `deltachat-jsonrpc/src/api.rs`, `.../api/types/calls.rs`,
  `.../api/types/events.rs`. The version this package bundles.
- [`deltachat/calls-webapp`][calls-webapp] -- `README.md`, and its
  licence.
- [`sailfishos/embedlite-components`][embedlite] --
  `jscomps/EmbedLiteWebrtcUI.js`.
- [`sailfishos/sailfish-components-webview`][webview] --
  `import/popups/PopupOpener.qml`, `import/popups/WebrtcPermissionDialog.qml`,
  `import/webview/WebView.qml`.
- The two device reports linked above, which are the only public
  evidence found either way about WebRTC on a Sailfish phone.

[core]: https://github.com/chatmail/core
[calls-webapp]: https://github.com/deltachat/calls-webapp
[embedlite]: https://github.com/sailfishos/embedlite-components
[webview]: https://github.com/sailfishos/sailfish-components-webview
[blue-tint]: https://forum.sailfishos.org/t/browser-webview-camera-video-has-a-blue-tint/14878
[conf-crash]: https://forum.sailfishos.org/t/xperia-x-4-5-0-19-browser-crashes-when-connecting-to-video-conference/15456
