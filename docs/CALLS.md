# Calls

*How a call works in Piirit, how much of the phone's own call handling
an app can have, and what only a phone can still answer.*

**Voice calls are on for everybody**, as they are in the reference
clients since Delta Chat 2.51: they were experimental, behind a switch
under *Settings → Advanced*, until the call bugs in #119 were fixed.
Whether a call rings here is *Settings → Notifications → Calls*, the
reference clients' own switch. Video calls are
built on the same page, and have not yet been made on a phone. Voice
calls have been made on a phone, Sailfish OS 5.2; the questions at the
end are the ones a source tree cannot answer, and the ones still open
say so.

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
  That timer lives only in the server that heard the call, and says
  `CallEnded` once; a restarted server or an overflowed event channel
  loses it, so a call ringing here also stops by itself after 125
  seconds (`calls.rs`, `RING_LIMIT`), by when the core counts it stale.
- **A ringing call raises no `IncomingMsg`**, and one that rings out
  says only `MsgsChanged` and `CallEnded`. A missed call's notification
  is the client's to raise; the chat list's message notifications never
  see one.
- **One-to-one only**, and encrypted: the core refuses a group ("Can
  only place calls in single chats") and Saved messages ("Cannot call
  self"), and a chat without the other end's key cannot send.
- **`who_can_call_me`**, per device, not synced: 0 everybody, 1
  contacts (the default), 2 nobody. A call from somebody who may not ring
  is still stored, and can still be answered from its chat. Not synced,
  but kept in the profile's database, so a backup or a second device
  takes it along: Piirit writes 2 only when the reader switches ringing
  off, and leaves the default otherwise.

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

### Placing one

From the contact: their page's tiles (`MediaKinds.qml`) have a **Calls**
tile while the chat is one a call can be placed in --
one-to-one, encrypted, taking messages, the core's own conditions asked
in advance (`chat.rs`'s `takes_calls`). Behind it, `CallsPage.qml`
lists the chat's calls from the chat's own media index (`chat_media.rs`,
kind `calls`, the core's `Call` view type), newest first -- which way
each went, what became of it, when -- under a first row, *Call Ada*, to
place one from. A tap on a listed call does what a tap on its row in the
chat does. The chat's header has no call button: both ends of that line
are taken, the left by the back gesture and the right by the way to the
contact's page.

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

While a call is up the app's cover is the call's: the other end's
picture behind, faint and enlarged (their avatar instead where they
have none), their name, how long, and a note for a microphone that is
off and for a camera paused while the app is away. Its one quick action
is the microphone: hanging up from the home screen would be too easy
to do by mistake, and the camera is let go in the background anyway.

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
- the app's microphone switch arrives on the same poll, as `mute` or
  `unmute`, and the bridge presses the page's own switch -- found by its
  label -- rather than turning the track off behind it: the page keeps
  whether its microphone is on, and tells the other end;
- the page says nothing about how its connection is doing, so the bridge
  watches the peer connection and reports ICE's own states -- which is
  where *Connected* and the clock come from, and what ends a call whose
  connection has failed;
- what the bridge sends the host goes one request at a time, in the
  order it was said: each reaches the host on a connection of its own,
  and two in flight together were seen taken the wrong way round, which
  for `/video` would leave the call screen with the older report;
- Gecko before 126 has no `RTCIceCandidate.type`, which the page reads
  to send its offer as soon as it has a relay candidate; without it the
  offer would wait for gathering to finish. The bridge reads the type
  out of the candidate line.

The page is opened on `#startCall` or `#acceptCall=<offer>`, with
`?noOutgoingVideoInitially` unless the camera is to start on (Video,
below). Never `?disableVideoCompletely`, which would drop the other
end's picture as well as never opening the camera. Its policy
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

### The screen

The call page (`qml/pages/CallPage.qml`) is laid out as the phone's own
call screen: who, top left, and the time of day, top right; how long, in
hours, minutes and seconds, large, under them; the switches under that;
and *End call* at the foot. The other end's picture, where they have one
of their own, fills the screen behind it, blurred and dimmed. Of the
phone screen's four switches the microphone is the one a call here has:
the keypad and recording are a phone call's, and the loudspeaker is the
earpiece question below.

The page the media runs in draws controls of its own, in English. It is
kept running under the call screen, unseen and behind a catch that takes
every tap, and what its controls did is done from the screen.

### Video

A video call is the same call with the camera on: the same page, the
same peer connection -- the page negotiates a video track for every
call, voice or not -- and the same five methods, the core told
`has_video` when it is placed. The engine encodes and decodes; nothing
of FFmpeg's or anybody else's is linked for it.

The camera is the bridge's (`rust/piirit-shim/src/call_video.js`,
served ahead of `calls.js`):

- **Closed until it is wanted.** The page asks for the camera and the
  microphone once, as the call starts, and would keep the camera open
  behind a disabled track for the whole call. It is handed the
  microphone and, for a voice call, a blank video track -- a canvas never
  asked for a frame -- so its camera switch is still drawn and a voice
  call can become a video call. Switched on, the camera's track replaces
  the blank one on the page's stream and on the peer connection's sender
  (`replaceTrack`, no renegotiation, which the page cannot do); switched
  off, it is stopped and the blank one goes back.
- **The page's switch pressed, not bypassed.** The page tells the other
  end whether its camera is on, over its own data channel, and the other
  end hides a picture that has been switched off rather than freezing
  its last frame. So the bridge presses the page's own *Start camera* or
  *Stop camera*, as it presses the microphone's.
- **One camera at a time.** Turned, the open camera is stopped before
  the other is asked for; a phone opens one at a time. The other is asked
  for by its device id -- the camera whose name says it faces that way,
  or else any that is not the one just open -- because a `facingMode` is
  only a preference, and on the phone Gecko gave the same camera back.
  What the camera's own settings say it faces is what is mirrored and
  reported. Gecko calls every back camera alike, and a phone with a wide
  lens as well may list that one first; so the back camera opened is the
  one at the place of the label nearest `1.0` among those the platform
  camera writes on its back cameras' rings
  (`/apps/jolla-camera/backCameraLabels`, read and never written), the
  first where there are none, and the log says which (`(2 of 2)`). That
  the two list the cameras in the same order is inferred, not checked
  against the platform's sources. If the other will
  not open, the first is opened again; if neither will, the camera is
  reported as failed, with what the engine said -- written to the app's
  log as `piirit: the camera would not open: …` -- and the app switches
  it off.
- **Held to what a phone needs.** Asked for at 640×480, 24 frames, and
  every video sender capped at 700 kbit/s and 24 frames once negotiated.
  Every pixel not captured is battery kept. The camera is only ever
  asked for preferences: a required value (a `max`, a `min`, an `exact`)
  is one some mode must meet or Gecko opens nothing, and it weighs a
  frame rate against the most a mode can do -- a phone camera that runs
  at 30 fails `max: 24` in every mode. The cap is the sender's instead.
- **VP8 first.** On the phone, a call answered here showed the other
  end's picture and a call placed here did not. Sailfish's Gecko turns
  hardware H264 on for WebRTC (`media.webrtc.hw.h264.enabled`), and an
  engine with it lists H264 ahead of VP8 in the offers it makes; the
  other end answers in the offer's order. So a call placed here would be
  sent H264, where a call answered here is sent what the other end's
  offer puts first. The offer the other end is sent lists VP8 first
  (`calls.js`); nothing else in it changes, so the answer is still one
  this end offered. Against an iPhone that is moot: Delta Chat on iOS
  offers and answers H264 alone (seen in a log).
- **An H264 encoder of its own.** Gecko decodes H264 with the phone's
  decoder but encodes it only through a media plugin, a library it loads
  in a process of its own from the directories `MOZ_GMP_PATH` names. On
  Sailfish that is gmp-droid, the phone's hardware encoder, and it is
  not on every phone -- nor installable on every one -- so a call to an
  iPhone sent no frames at all. The package carries one:
  `/usr/share/harbour-piirit/lib/gmp-piirit/1.0/`, x264 behind Gecko's
  plugin interface (`rust/piirit-video/csrc/gmp_h264.cpp`), built from
  the same x264 as the app (docs/BUILDING.md). The app points Gecko at it
  before anything starts (`prepare_media_plugins`, `call_host.rs`), after
  whatever `MOZ_GMP_PATH` already holds -- and not at all when a plugin
  there already encodes H264, which is gmp-droid on a phone whose
  hardware can, and cheaper than software. It is set up to cost little
  for as long as a picture goes out: Constrained Baseline, which every
  receiver decodes, at x264's fastest preset that keeps the deblocking
  filter, one thread and no lookahead, so no picture waits for the next;
  the camera's own buffers read in place; the bit rate Gecko's
  congestion control asks for, made good per second when the camera
  gives fewer pictures than the call was set up for; and a keyframe,
  with SPS and PPS before it, when the far end asks for one. Android
  ends answer VP8 and never start it. The plugin writes to the call log
  itself, named by `PIIRIT_CALL_LOG`: the size and rate it started at,
  its first picture, and every half minute and at the end the pictures,
  keyframes, CPU time each and bit rate sent. `rust/piirit-video/tests/
  gmp_h264.rs` drives it as Gecko does, through `tests/gmp_check.cpp`.
- **Let go in the background.** With the app away the camera is
  stopped -- the other end is told it is off -- and the pictures are not
  drawn; back in front, it is opened again if it was on.
- **Reported.** Whether the camera is live and whether the other end's
  picture is being drawn (a video track with frames, not switched off at
  their end) goes back on `/video`, which is what `Call`'s
  `local_video` and `remote_video` are.
- **Logged.** What only a phone shows about a call goes through `/log`:
  which camera opened and at what size, a camera that would not open or
  turn and why, how long a turn took to open the other camera and to
  draw its first picture, each step of the negotiation, the picture
  sections both ends agreed and their codecs in order, whether the other
  end's picture has frames and what the other end says of its camera,
  and -- a few seconds, then a quarter and three quarters of a minute
  after the call connects or the camera changes -- what goes each way:
  frames, bytes a frame (a black frame is a few dozen, a picture
  thousands), keyframes asked for (a receiver that cannot decode asks over
  and over), which track the sender has, and whether each picture is
  hidden, plays, at what size, and how many of its frames Gecko has
  painted (`mozPaintedFrames`). Every `/video` report is logged as it
  goes, and so are whether the engine holds the page hidden, every press
  of one of its buttons and whether it was a touch or the bridge, and a
  picture put back (below). The page's own console is read too:
  its warnings and errors, failures nothing caught, and what it says of
  the other end's picture and its own commands, but none of its
  candidates or descriptions, which carry addresses. Each line goes to stderr as
  `piirit: call: …`, and to `~/.cache/piirit/piirit/call.log`, begun
  afresh with every call and each line stamped with the seconds since;
  the H264 encoder's lines are in the same file, marked `encoder`.
  The file is what to read on the phone: the launcher sends the app's
  stderr nowhere, and the journal has none of it.

The call screen shows the page while there are pictures -- its pictures
and nothing else of it: the bridge hides every element but the two
videos -- with its own lines and switches over them, on a shade. This
end's picture fills the screen until the other end's comes, then sits
in the corner, mirrored only from the front camera. The reader can drag
it out of the corner: it stays where it is let go, anywhere between the
top line and the switches, whether or not those are shown. The call
screen owns where it is -- an invisible item over it takes the touches
and carries the button that turns the camera -- and tells the bridge
(`corner=LEFT,TOP`, fractions of the screen); a drag is told with
every touch event, as soon as the event is through, and the host keeps
only the last place still waiting, so the page draws where the finger
is rather than where it was. In the corner the picture is placed at the
top left and shifted there with a transform, on a layer of its own
(`will-change: transform`): a move changes the shift and nothing else,
so the engine moves the layer without laying out or painting anything
again. The other end's is
enlarged towards filling the screen too, but by no more than 1.35
times: an upright phone's picture fills a taller screen, and one held
sideways loses some of its bars rather than half of itself. It is laid
out larger, not scaled with a transform, which would be one more layer
for the engine to put together. A picture meant to be seen -- not
hidden, playing, with frames, the page in front -- that Gecko has
painted none of in two seconds is put back: its frames taken off its
stream and put on again, and the stream given to it again, which is
what turning the camera does, and was seen to draw this end's picture
where on a phone nothing had been drawn. Until a picture has painted
its first frame -- since it came on the screen, or was given a stream
it had not had -- it is black, and it is looked at every half second
rather than every two, so one the engine leaves unpainted is put back
within a second of having frames. Three times a call at most, then
logged; where the engine does not count what it paints, once, a look
after it first shows. One put back paused is asked to play until it
does, since the other end's picture carries their voice too. Whether
each picture shows yet goes with every `/video` report
(`localPainted`, `remotePainted`), and the call screen draws a spinner
over a picture that does not: the whole screen, or this end's corner. A
picture given up on -- put back three times, still nothing -- is said
to show, so the spinner does not turn for ever over it. Turning the
camera makes this end's picture black from the press until the other
camera's first frame is drawn, which on a phone can take a second or
two: the spinner shows, and the picture is not looked at while the
other camera is opening -- waited for, not put back or given up on.
A tap puts the lines
away, and they go by themselves once both ends are seen; the time of day
gives way to how long, and the large clock goes. The camera switch is
there in every call, and on the ringing screen of a video call, so it
can be answered without the camera; turning it is a button beside it
while this end's picture fills the screen, and in the picture's lower
corner nearer the middle of the screen once it has its corner, going
with the lines and switches -- either only once the picture shows. On
the picture it is small, a disc a little larger than its icon, with
room round it to press. Pressed, it stays where it is, dimmed and not
pressable, until the other camera's picture shows, so nothing moves
under the finger; ten seconds at most. A video call is answered with the front
camera. This end's picture plays by itself, muted, when the engine lets
it; one that sits paused is asked to play until it does.
While there are pictures the screen is kept lit and the ear shield is
off: a video call is held in front of the face, not to the ear.

Placing one is the contact's Calls page, *Video call Ada*, under *Call
Ada*; a tap on a video call's row calls back with the camera on, as the
reference clients do.

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
| **Ringtone** | ngfd, the daemon that makes every sound the phone makes, plays `voip_ringtone`: the phone's own tone for a call that is not a phone call -- the profile's `voip.alert.tone`, and silent when its `voip.alert.enabled` is off -- at the ringing volume, silenced by the silent profile, repeating until stopped. Should ngfd refuse it, or say it failed, the phone's own `ringtone` with `type` `voip` instead, as voicecall plays it for a call that is not a SIM call | ngfd's system-bus interface, which sailjail opens to every app; not a listed Harbour API |
| **Screen and call state** | mce is told `ringing`, then `active`, then `none`: it lights the screen for a ringing call and treats the phone as being in one. It forgets by itself if the app goes. `ringing` only once ngfd has answered the ringtone's Play, or a second has passed: ngfd follows mce's call state, and the event set's `voip_ringtone` plays a call-waiting beep instead of the ringtone while mce says a call is on, ringing included -- which is what told it first did (#98) | mce's request interface, open to every app; not a listed Harbour API |
| **Lock screen** | once mce says the screen is on, `req_tklock_mode_change("unlocked")` and the app brought forward, so the call's page is what is on screen; raised behind the lock screen, it showed for a moment and was covered again (#99). mce turns the screen on for a ringing call only when it would for a phone call -- not in a pocket -- and puts the lock back when the call is over. A device lock, a code or a fingerprint, is left alone: the call's page is behind it. A Critical notification with Decline and Answer on it besides, which the lock screen shows | mce's request interface; `Nemo.Notifications` |
| **Missed calls** | left in the notification area in the phone's own category for one, `x-nemo.call.missed`, with Call back on it | `Nemo.Notifications` |
| **Staying awake** | the CPU kept up while a call is up: a phone that suspends takes the call's sound with it | `Nemo.KeepAlive` |
| **At the ear** | the proximity sensor puts a black screen that takes no touch over the call while it is held there -- an app cannot switch the display off | `QtSensors` |
| **In the background** | the call's `WebView` is held active once its page is up: an inactive view is a hidden document, and the engine pauses a hidden document's media | `Sailfish.WebView` |
| **Microphone and camera** | granted to the call's own page, for the session, before it asks -- the engine's prompt would otherwise open every call, since the origin is a new port each time. Never engine-wide: that would grant them to every webxdc app too | `Sailfish.WebEngine` |
| **Screen on** | kept lit while a call shows pictures, and while one rings | `Nemo.KeepAlive` |

Left out on purpose:

- **ohm's route preference.** `Route.Manager.Prefer("earpiece")` is open
  to an app with the Audio permission, but it holds for every sound the
  phone makes and ohm does not know who asked for it: a crash mid-call
  would leave the phone on the earpiece until the next phone call or
  restart. The Phone app's own loudspeaker button,
  `Playback.Manager.RequestPrivacyOverride`, is the same kind of global.
  Neither is used; the next section is what is.
- **The `call` resource class**, which is voicecall's, needs the
  Cellular credential and drives the modem's call paths.
  `libaudioresource`, the resource library Harbour lists, offers only
  `player` and `game`, neither of which reaches the earpiece.
- **`Nemo.Ngf`**, which would ring from QML: not on Harbour's list, hence
  the bus.

### Earpiece and loudspeaker

A voice call is on the earpiece, as a phone call is; a video call on the
loudspeaker, as the reference clients have it, being held in front of
the face. A voice call whose pictures come on goes to the loudspeaker
too, unless the reader has switched this call. The loudspeaker switch on
the call page goes between the two at any time, and holds for the rest of
the call. A headset, headphones or a Bluetooth device takes the sound by
itself; the switch is dimmed then, having nothing to move it from, and
also when the policy did not take the call.

How (`CallAudio.qml`): from when a call carries sound -- placed, or
answered -- until it is over, the app holds a resource set of class
`aliencall` with ohm's resource manager (`org.maemo.resource.manager`,
system bus; the Audio permission opens it). `aliencall` is the class
Sailfish has for calls that are not phone calls, the one Android App
Support's VoIP apps get. ohm routes the sound of whichever set owns
audio playback by that set's class: for `aliencall`, a Bluetooth or USB
device, then a wired headset or headphones, then the earpiece
(`earpieceforalien`), then the loudspeaker (`ihfforalien`). The policy
rules out the earpiece for an `aliencall` set that also owns video
playback, so the loudspeaker is the same set with `VideoPlayback` added
-- one `update`, nothing global.

The set is spoken to over the bus in libresource's own message layout
(`register`, `audio`, `acquire`, `update`, `release`, `unregister`;
`dbus-msg.c`), since no library for it is on Harbour's list, and in its
order: one message at a time, each once the last is answered. The
manager needs that. It answers a register only after asking the bus
whose connection sent it, and refuses anything else about the set that
comes in before then, as `EUCLEAN` (ohm's `manager.c`). Sent all at
once, as the first build sent them, the `audio` and `acquire` were
refused on a phone every time, and the set let go of. Its `audio`
message names the app's streams: every stream whose client carries the
app's `policy.application.id` -- the process's start time from
`/proc/self/stat`, in hex, which Sailfish's libpulse puts on every
stream it opens and `Call.audio_id` reads the same way -- joins group
`aliencall`, including streams already playing. Without that the call
would be worse off than with no set at all: while an `aliencall` set is
active the policy corks the catch-all group Gecko's streams otherwise
fall in. So with no id nothing is asked for, and a set the manager
refuses any part of, or that has no manager to answer, is let go of at
once.

What makes it safe is where the set lives. ohm watches each client's
bus connection and drops every set of a client whose connection goes:
a crash, a kill, the OOM killer, or closing the app puts the sound back
where it was with nobody doing anything. Nothing is written that
outlives the app, so there is nothing for a next run to clean up. While
the app runs, the set is held exactly while the call carries sound, and
given back on the way out besides.

Read-only besides: ohm's `Route.Manager.ActiveRoutes` and its
`AudioRouteChanged` signal, for whether the sound is on something
plugged in or paired.

## What only a phone can answer

Ranked by what would sink the feature. None of them needs another line
of Piirit: two phones and an afternoon.

1. **Does a peer connection complete on the device, with the
   microphone?** Yes: calls have gone through on 5.2.
2. **Does the other end's voice keep playing with the app in the
   background?** The view is held active for it; that this is enough is
   the claim.
3. **Do the call's streams join the call group?** The `aliencall` set
   names them by the app's process; that the engine's audio streams are
   opened by the app's own process (ESR 91 runs the engine in it), and
   so carry its id, is the claim. On a phone they do carry it: the
   engine's client is the app's process, with the `policy.application.id`
   `Call.audio_id` reads. Outside a call they are in group `player`:
   some library in the process holds a set of that class for the same
   id *(inferred; which one is not known)*, and the `aliencall` set,
   naming the id the same way, takes its place for the call. Silence in
   a call would say they are not. `pactl list sink-inputs` shows each stream's `policy.group` and
   `application.process.id`; `pactl list sinks` the active port. ESR
   153 moves a page's audio into a process of its own, which will want
   this looked at again.
4. **Does the earpiece route play from the earpiece alone?** Music in the
   Media app plays from the earpiece and the loudspeaker both (#100):
   the phone's loudspeaker output drives the two. `earpieceforalien` is
   the earpiece port by name; that it is quiet at arm's length is the
   claim.
5. **What it sounds like.** Echo on speakerphone, and what a call costs
   in battery.
6. **Does the camera open in the engine, and turn?** Gecko reaches the
   phone's cameras through `gecko-camera`; that `facingMode` picks the
   back one, and that the engine lets one go before the next opens, is
   the claim. What a video call costs in battery and heat at VGA, 24
   frames, is the other half of it.

## What would not change

- **A call rings only while the app is running.** No push notification
  service is open to a third-party client (`docs/PROJECT.md`), so a call
  is heard only if the app is alive to receive its message, minimised
  included, inside the core's 120 seconds. Anything else is a missed
  call's row.
- **The camera's focus.** Gecko opens the phone's camera through
  `gecko-camera`, which sets one parameter on it, `video-size`: no focus
  mode, and no focus search is ever started, so the lens stays wherever
  the camera's own default leaves it. The QR scanner and the in-app
  camera set continuous focus and focus on a tap, through QtMultimedia;
  a page has no such control (Gecko ESR 91 has no focus constraint), and
  `gecko-camera` loads its plugins from a directory fixed when it was
  built, so an app cannot put its own in front. The fix is upstream: a
  `focus-mode` of `continuous-video` where the camera offers one.
- **Harbour.** Calls add no executable, no library off the list and no
  permission that was not already asked for. They add system-bus
  services Harbour does not list -- ngfd, mce, and ohm's resource and
  route managers -- which its validator cannot see and its QA may or may
  not accept; each is a `DBusInterface` in `CallCenter.qml` or
  `CallAudio.qml`, and a call works without any of them, on the
  loudspeaker. And they promote the browser engine, a separate package on
  this platform, from a webxdc feature to the load-bearing part of a
  headline one.

## Next

1. **The phone questions above**, before anything else.
2. **The earpiece**, once it can be released even when the app is not
   there to release it.
3. **The chat list's preview** of a call is still the core's English
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
