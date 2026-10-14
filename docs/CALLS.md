# Calls

**Voice calls on for everybody**, as in reference clients since Delta Chat 2.51 (experimental behind *Settings → Advanced* until #119 call bugs fixed). Whether call rings here: *Settings → Notifications → Calls*, reference clients' own switch. Video calls built on same page, not yet made on phone. Voice calls made on phone, Sailfish OS 5.2. Closing questions = ones source tree can't answer; open ones say so.

Shape: **core does call, Gecko does media, Piirit is glue.** Core places, rings, accepts, ends, times out. Platform's browser engine: peer connection, codecs, echo cancellation. Upstream calls-webapp = page driving them. Written here: page host, call screen, phone side (ringtone, screen, notification).

## What the core does

`deltachat-rpc-server` 2.62, bundled binary (`vendor/deltachat-rpc-server/SOURCE.md`), carries whole call protocol. `rust/deltachat-jsonrpc/tests/real_server.rs` pins app-read parts against binary itself.

- **Five methods.** `place_outgoing_call(account, chat, offer, has_video)` → message id that *is* the call. `accept_incoming_call(account, message, answer)`. `end_call(account, message)` = decline, cancel, hang-up; second one not error. `call_info(account, message)` → `{sdpOffer, hasVideo, state}`, state tagged by `kind`: `Alerting`, `Active`, `Completed` (+ `duration` s), `Missed`, `Declined`, `Canceled`. `ice_servers(account)` → JSON *string* of TURN servers; bridge hands to page as is.
- **Four events**, one trap: `IncomingCall`, `IncomingCallAccepted`, `OutgoingCallAccepted`, `CallEnded` serialised **snake_case** (`msg_id`, `chat_id`, `place_call_info`, `has_video`, `from_this_device`, `accept_call_info`); rest of stream camelCase. Reader asking `msgId` gets nothing, silently. `calls.rs` reads as-is; tests feed that shape.
- **Chat = signalling channel.** Call = `Viewtype::Call` message, offer in header; accept/end = hidden messages quoting it. Core never reads SDP: media is client's problem only.
- **Rings 120 s**, then stale both ends. Core timer ends it: incoming Missed, outgoing Declined. Timer lives only in server that heard call, says `CallEnded` once; restarted server or overflowed event channel loses it. So ringing here also stops itself at 125 s (`calls.rs`, `RING_LIMIT`), by when core counts it stale.
- **Ringing call raises no `IncomingMsg`**; one ringing out says only `MsgsChanged`, `CallEnded`. Missed-call notification = client's; chat list's message notifications never see one.
- **One-to-one only**, encrypted: core refuses group ("Can only place calls in single chats") and Saved messages ("Cannot call self"); chat without other end's key can't send.
- **`who_can_call_me`**, per device, not synced: 0 everybody, 1 contacts (default), 2 nobody. Call from someone not allowed to ring still stored, answerable from chat. Unsynced but in profile DB: backup or second device takes it along. Piirit writes 2 only when reader switches ringing off; else leaves default.

## How a call works here

### The row

- One row (`MessageDelegate.qml`): kind or outcome (*Missed call*, *Declined call*, *Canceled call*), duration, in Delta Chat's words so its translations apply.
- Row reads call state (`calls::summary`, filled by `chat.rs` like webxdc row). Core's own call text = English sentence; shown only when core won't say state.
- Tap on call ringing here takes it up; tap on any other calls back (deltachat-android's rule).

### Placing one

- Contact page tiles (`MediaKinds.qml`) have **Calls** tile while chat can take call: one-to-one, encrypted, taking messages; core's conditions asked in advance (`chat.rs` `takes_calls`).
- Behind it `CallsPage.qml`: chat's calls from chat's media index (`chat_media.rs`, kind `calls`, core's `Call` view type), newest first: direction, outcome, when. First row *Call Ada* places one. Tap on listed call = tap on its chat row.
- No call button in chat header: both ends taken (left = back gesture, right = way to contact page).

### The call

- One `Call` object (`calls.rs`) per window, held by `CallCenter.qml`: call belongs to no page; comes in whatever is on screen; leaving page ≠ hanging up.
- States: `ringing`, `starting`, `calling`, `connecting`, `connected`, `reconnecting`, `ended`. Says why ended: hung up here, declined here, ended there, answered on another device, missed, failed.
- One call at a time, as both reference clients: second rings nowhere, missed once core gives up.
- `CallPage.qml`: who, state, in phone's language; Decline, Answer while ringing; then call's page. Not swipeable away while call up (would take call's sound). Notification tap or cover quick action brings call to front, not pop.
- Cover during call: other end's picture behind, faint, enlarged (avatar if none); name; duration; note for mic off and for camera paused while app away. One quick action = mic: hang-up from home screen too easy by mistake; camera let go in background anyway.

### The page

- Media = upstream [`calls-webapp`][calls-webapp] v0.12.1: WebRTC "for integration into Delta Chat clients"; deltachat-android's call page until it went native in v2.47. One self-contained `.html` release asset, committed (`vendor/calls-webapp/`, `scripts/fetch-calls-webapp.sh`, checked by `ci/vendor-check.sh`), compiled into app.
- `call_host.rs` serves it from own loopback address, as `webxdc_host.rs`: kernel-picked port, `Host:` must be one page was given, everything reaching core behind unguessable token, nothing served after call.
- Page loads classic `calls.js` before own code; host answers with Piirit bridge (`rust/piirit-shim/src/calls.js`). Bridge static, no secret: token in page's address (`?key=`), handed only to app's own WebView. Page elsewhere including script (browsers allow cross-origin) finds no key, reaches nothing.

Bridge:

- `startCall(offer)`, `acceptCall(answer)`, `endCall()` = requests to host: place, accept, end.
- `getIceServers()` asks host for core's servers (not written into script: TURN credentials are account's). `getAvatar()` names other end's picture, served from host.
- Other end's answer arrives as page's own hash command `#onAnswer=`, collected by long poll.
- App mic switch arrives on same poll (`mute`/`unmute`); bridge presses page's own switch (found by label), not track off behind it: page keeps mic state, tells other end.
- Page says nothing of connection health: bridge watches peer connection, reports ICE states. Source of *Connected* and clock; failed connection ends call.
- Bridge → host one request at a time, in order said: each on own connection; two in flight seen taken reversed, which for `/video` would leave call screen with older report.
- Gecko < 126 has no `RTCIceCandidate.type`, which page reads to send offer once it has relay candidate (else offer waits for gathering to finish). Bridge reads type from candidate line.

Page opened on `#startCall` or `#acceptCall=<offer>`, with `?noOutgoingVideoInitially` unless camera starts on (Video, below). Never `?disableVideoCompletely`: drops other end's picture too, not just camera. Page's policy keeps every fetch on own origin; peer connection not a fetch, not governed. Offer goes to core only once page gathered enough ICE. Call hung up while core still placing it ends as soon as it lands. Nothing orders core's reply to "place this" vs other end's answer, so answer arriving before call knows its message is kept, applied once it does (`tests/call_early_answer.rs`).

Same page speaks protocol native Android and iOS reimplemented (same negotiated data channels, same "enough ICE, then trickle"): call from here reaches either.

### The screen

- Call page (`qml/pages/CallPage.qml`) laid out as phone's own call screen: who top left, time of day top right; duration (h:m:s) large under them; switches under that; *End call* at foot. Other end's picture, if they have own, fills screen behind, blurred, dimmed.
- Of phone screen's four switches: mic, loudspeaker ("Earpiece and loudspeaker", below), plus camera. Keypad, recording = phone call's.
- Media page draws own controls, in English. Kept running under call screen, unseen, behind catch taking every tap; its controls' jobs done from call screen.

### Video

Video call = same call with camera on: same page, same peer connection (page negotiates video track every call, voice or not), same five methods, core told `has_video` on place. Engine encodes, decodes; no FFmpeg or other lib linked for it.

Camera = bridge's (`rust/piirit-shim/src/call_video.js`, served ahead of `calls.js`):

- **Closed until wanted.** Page asks camera + mic once at start, would keep camera open behind disabled track whole call. Handed mic and, for voice call, blank video track (canvas never asked for frame): camera switch still drawn, voice call can become video. On: camera track replaces blank on page's stream and on peer connection's sender (`replaceTrack`, no renegotiation, which page can't do). Off: stopped, blank back.
- **Page's switch pressed, not bypassed.** Page tells other end camera on/off over own data channel; other end hides switched-off picture, not freeze last frame. So bridge presses page's *Start camera* / *Stop camera*, as for mic.
- **One camera at a time.** Turn: open camera stopped before other asked (phone opens one at a time). Other asked by device id (camera whose name says it faces that way, else any not just open): `facingMode` only preference; on phone Gecko gave same camera back. Camera's own settings' facing = what's mirrored and reported; through turn old mirroring stays until new camera in place, so last frame doesn't flip first.
- Gecko names every back camera alike; phone with wide lens may list it first. So back camera opened = one at place of label nearest `1.0` among those platform camera writes on back cameras' rings (`/apps/jolla-camera/backCameraLabels`, read, never written); first if none; log says which (`(2 of 2)`). Same order in both lists = inferred, not checked vs platform sources.
- Other won't open → first reopened. Neither → camera reported failed with engine's words, logged `piirit: the camera would not open: …`; app switches it off.
- **Held to phone needs.** Asked 640×480, 24 frames; every video sender capped 700 kbit/s, 24 frames once negotiated. Every pixel not captured = battery kept. Camera only ever asked preferences: required value (`max`, `min`, `exact`) must be met by some mode or Gecko opens nothing, weighed vs mode's most: phone camera running at 30 fails `max: 24` in every mode. Cap is sender's instead.
- **VP8 first.** On phone, call answered here showed other end's picture; call placed here didn't. Sailfish Gecko turns hardware H264 on for WebRTC (`media.webrtc.hw.h264.enabled`); engine with it lists H264 before VP8 in own offers; other end answers in offer order. So placed call would be sent H264; answered call gets what other end's offer puts first. Offer sent lists VP8 first (`calls.js`); nothing else changes, answer still one this end offered. iPhone moot: Delta Chat iOS offers and answers H264 only (seen in log).
- **Own H264 encoder.** Gecko decodes H264 with phone's decoder but encodes only via media plugin, lib loaded in own process from dirs `MOZ_GMP_PATH` names. On Sailfish = gmp-droid (hardware encoder), not on every phone, not installable on every one: call to iPhone sent no frames. Package carries one: `/usr/share/harbour-piirit/lib/gmp-piirit/1.0/`, x264 behind Gecko plugin interface (`rust/piirit-video/csrc/gmp_h264.cpp`), same x264 as app (docs/BUILDING.md).
  - App points Gecko at it before anything starts (`prepare_media_plugins`, `call_host.rs`), after whatever `MOZ_GMP_PATH` holds; not at all when plugin there already encodes H264 (gmp-droid on phone whose hardware can; cheaper than software).
  - Cheap while picture goes out: Constrained Baseline (every receiver decodes), x264 fastest preset keeping deblocking filter, 1 thread, no lookahead (no picture waits for next); camera buffers read in place; bit rate Gecko congestion control asks, made good per second when camera gives fewer pictures than call set up for; keyframe with SPS + PPS when far end asks. Android ends answer VP8, never start it.
  - Plugin writes call log itself, named by `PIIRIT_CALL_LOG`: start size and rate, first picture, every 30 s and at end pictures, keyframes, CPU time each, bit rate sent. `rust/piirit-video/tests/gmp_h264.rs` drives it as Gecko does, via `tests/gmp_check.cpp`.
- **Let go in background.** App away: camera stopped (other end told off), pictures not drawn. Front again: reopened if was on.
- **Reported.** Camera live? Other end's picture drawn (video track with frames, not switched off their end)? Sent on `/video` = `Call`'s `local_video`, `remote_video`.
- **Logged** via `/log`, what only phone shows: which camera opened at what size; camera that wouldn't open or turn, why; turn time to open other camera and to draw first picture; each negotiation step; picture sections both ends agreed, codecs in order; other end's picture has frames?, what other end says of its camera. Few seconds, then 15 s and 45 s after connect or camera change, each way: frames, bytes/frame (black frame few dozen, picture thousands), keyframes asked (receiver that can't decode asks repeatedly), sender's track, each picture hidden?/plays?/size, frames Gecko painted (`mozPaintedFrames`). Also: every `/video` report as sent, whether engine holds page hidden, every press of its buttons and whether touch or bridge, picture put back (below). Page console read too: warnings, errors, uncaught failures, what it says of other end's picture and own commands; none of its candidates or descriptions (carry addresses).
- Each line → stderr `piirit: call: …` and `~/.cache/piirit/piirit/call.log`, fresh each call, stamped with seconds since; H264 encoder lines same file, marked `encoder`. On phone read file: launcher sends app stderr nowhere, journal has none.

Call screen shows page while there are pictures: pictures only (bridge hides every element but two videos), own lines and switches over them on a shade.

- This end's picture fills screen until other end's comes, then top right corner (mirrored only from front camera), stays there, as reference clients. Invisible item on call screen over same corner carries camera-turn button: only one, so nothing turns it while its picture fills screen. Dragging tried, dropped: picture trailed finger on phone, even on own layer.
- Other end's picture enlarged toward filling screen, max 1.35×: upright phone's picture fills taller screen; one held sideways loses some bars, not half itself. Laid out larger, not transform-scaled (one more layer for engine to compose).
- **Put back.** Picture meant seen (not hidden, playing, with frames, page in front) Gecko painted none of in 2 s → put back: frames taken off stream and on again, stream given again (= what turning camera does; seen to draw this end's picture on phone where nothing had). Until first frame painted (since on screen, or given new stream): black, checked every 0.5 s not 2 s, so unpainted one put back within 1 s of having frames. Max three per call, then logged. Where engine doesn't count what it paints: once, one look after it first shows. Paused one put back → asked to play until it does (other end's picture carries their voice too).
- `/video` reports carry whether each picture shows yet (`localPainted`, `remotePainted`); call screen draws spinner over one that doesn't: whole screen or this end's corner. Picture given up on (three put-backs, still nothing) reported as showing, so spinner doesn't turn forever.
- Turning camera: this end's picture black from press until other camera's first frame (1–2 s on phone). Spinner shows; picture not checked while other camera opens: waited for, not put back or given up on.
- Tap puts lines away; they go by themselves once both ends seen; time of day gives way to duration, large clock goes.
- Camera switch in every call, and on ringing screen of video call (answer without camera). Turn = button beside it while this end's picture fills screen; once picture has corner, in its lower corner nearer screen middle, going with lines and switches; either only once picture shows. On picture: small disc, little larger than icon, room round it to press. Pressed: stays put, dimmed, not pressable, until other camera's picture shows (nothing moves under finger); 10 s max.
- Video call answered with front camera. This end's picture plays itself, muted, when engine lets it; one sitting paused asked to play until it does.
- While there are pictures: screen kept lit, ear shield off (video call held in front of face, not to ear).

Placing: contact's Calls page, *Video call Ada* under *Call Ada*. Tap on video call's row calls back camera on, as reference clients.

## What the phone gives a call

Phone call has, app call can't: system call screen, call history, lock-screen call window. On Sailfish = voicecall's; way in = voicecall provider plugin, `.so` in `/usr/lib/voicecall/plugins`: outside anything Harbour lets package install, runs unconfined in voicecall-manager, needs `Phone` permission Harbour disallows. Apps that did it (a WhatsApp client, RooTelegram) ship outside Harbour. `Sailfish.Telephony` = SIM selection only. None here.

Here = rest of it:

| | How | Allowed |
|---|---|---|
| **Ringtone** | ngfd (daemon making every phone sound) plays `voip_ringtone`: phone's own tone for non-phone call (profile `voip.alert.tone`; silent if `voip.alert.enabled` off), ringing volume, silenced by silent profile, repeats until stopped. ngfd refuses or reports failure: phone's own `ringtone` with `type` `voip`, as voicecall plays for non-SIM call | ngfd system-bus interface, sailjail opens to every app; not listed Harbour API |
| **Music held off** | while ringing, resource set class `ringtone` with ohm resource manager, as phone ringing: other apps' music pauses, resumes if unanswered ("Earpiece and loudspeaker") | ohm resource manager, opened by Audio permission; not listed Harbour API |
| **Screen and call state** | mce told `ringing`, `active`, `none`: lights screen for ringing call, treats phone as in call. Forgets by itself if app goes. `ringing` only once ngfd answered ringtone's Play, or 1 s passed: ngfd follows mce call state; event set's `voip_ringtone` plays call-waiting beep instead of ringtone while mce says call on, ringing included. Telling mce first did that (#98) | mce request interface, open to every app; not listed Harbour API |
| **Lock screen** | once mce says screen on: `req_tklock_mode_change("unlocked")`, app brought forward, call page on screen (raised behind lock screen it showed a moment, then covered, #99). mce turns screen on for ringing call only when it would for phone call (not in pocket); restores lock after call. Device lock (code, fingerprint) left alone: call page behind it. Plus Critical notification with Decline, Answer, shown by lock screen | mce request interface; `Nemo.Notifications` |
| **Missed calls** | left in notification area, phone's own category `x-nemo.call.missed`, with Call back | `Nemo.Notifications` |
| **Staying awake** | CPU kept up during call: suspending phone takes call's sound with it | `Nemo.KeepAlive` |
| **At the ear** | proximity sensor puts black, touch-proof screen over call while held there (app can't switch display off) | `QtSensors` |
| **In the background** | call's `WebView` held active once page up: inactive view = hidden document; engine pauses hidden document's media | `Sailfish.WebView` |
| **Microphone and camera** | granted to call's own page, for session, before it asks: else engine prompt every call (origin = new port each time). Never engine-wide: would grant every webxdc app too | `Sailfish.WebEngine` |
| **Screen on** | kept lit while call shows pictures, and while ringing | `Nemo.KeepAlive` |

Left out on purpose:

- **ohm route preference.** `Route.Manager.Prefer("earpiece")` open to app with Audio permission, but holds for every phone sound and ohm doesn't know who asked: crash mid-call leaves phone on earpiece until next phone call or restart. Phone app's loudspeaker button, `Playback.Manager.RequestPrivacyOverride`, same kind of global. Neither used; next section = what is.
- **`call` resource class**: voicecall's, needs Cellular credential, drives modem call paths. `libaudioresource` (Harbour-listed resource lib) offers only `player`, `game`; neither reaches earpiece.
- **`Nemo.Ngf`** (ring from QML): not on Harbour's list, hence bus.

### Earpiece and loudspeaker

- Voice call on earpiece, as phone call. Video call on loudspeaker, as reference clients (held in front of face). Voice call whose pictures come on → loudspeaker too, unless reader switched this call.
- Call page loudspeaker switch toggles any time, holds rest of call. Switch takes ~1 s, this end's mic off meanwhile; switch shows busy until mic back. Headset, headphones or Bluetooth device takes sound itself; switch dimmed then (nothing to move from). Policy doesn't take call → loudspeaker for good, switch dimmed.

How (`CallAudio.qml`): whole call, app holds resource set class `aliencall` with ohm resource manager (`org.maemo.resource.manager`, system bus; Audio permission opens it). `aliencall` = Sailfish class for non-phone calls (Android App Support VoIP apps get it). ohm routes sound of whichever set owns audio playback by set's class; for `aliencall`: Bluetooth or USB device, then wired headset/headphones, then earpiece (`earpieceforalien`). Set also owning video playback kept off earpiece, routed to loudspeaker (`ihfforalien`). So earpiece = set with audio playback alone; loudspeaker = same set + video playback; switching = updating one set. Either way other apps' music pauses. Nothing global touched.

Before that, while ringing, second set, class `ringtone`: policy class for phone ringing, outranks media player's (`resource_classes.pl`), so music pauses for ring as for phone call, resumes if unanswered. Asks audio playback alone, names no streams: ngfd plays ringtone, policy files ringtone streams separately, so set moves nothing of app's; no mic open for move to break. Answered: call's set acquired before ring's given back, one message after other, so music doesn't return in between. Ring set refused by manager → let go; music plays through ring, as before. *(From policy rules; not yet heard on phone.)*

Mic makes this delicate. Moving call's sound while its mic open brings phone's sound server down. Policy's PulseAudio module moves every stream of call's group to new route, asserts each moved (`policy-group.c`, `pa_policy_group_start_move_all`). Engine opens mic on named device; cubeb's PulseAudio backend marks such stream not-to-move (`PA_STREAM_DONT_MOVE`). Move fails, assertion aborts PulseAudio, systemd restarts it. Restarted one forgot which streams were call's → call silent; few aborts in a row → systemd stops restarting, phone has no sound until someone who knows starts it. First builds' loudspeaker did that (#146); so did giving set back at end of earpiece call with mic still open *(inferred from captures: abort at hang-up in them)*. Headset plugged in mid-call does same: nothing in app can stop that one; platform's to fix.

So app never moves sound while its mic may be open:

- **Before it opens.** Set acquired, policy given 0.5 s to act, before call page comes up at all (`CallView.allowed`): ohm answers acquire few ms before telling PulseAudio where sound goes.
- **To switch**, page told to stop mic (host's `microphone-off`). Bridge stops track, takes it off connection's sender; page's own stream keeps it, so page's mute switch still has something to act on. Once PulseAudio itself has had no recording stream of app's for 300 ms, set updated; 0.5 s after policy answers, page told `microphone-on`, bridge opens new track in old one's place, on/off as page last had. Switch unfinished in 5 s → given up: mic back, sound where it was.
- **Once call over**, set given back only after mic shut.

What PulseAudio has open comes from `MicWatch`: watches app's own recording streams via PulseAudio client lib (on Harbour's list), every stream whose client is app's process. On phone, stream that wouldn't move was engine's, from app's own process (`AudioCallbackDriver`, `application.process.binary` `harbour-piirit`). Process id = one inside app's sandbox; another sandboxed app can have it too, its recording then reads as app's. Safe way round: switch gives up, set held until that recording ends. Watch can't tell (PulseAudio down, or not letting app see): switch dimmed while mic open; call's set given back 3 s after page told to stop mic.

Set spoken to over bus in libresource's own message layout (`register`, `audio`, `update`, `acquire`, `release`, `unregister`; `dbus-msg.c`), since no lib for it is on Harbour's list, and in its order: one message at a time, each once last answered. Manager needs that: answers register only after asking bus whose connection sent it; refuses anything else about set arriving before then as `EUCLEAN` (ohm `manager.c`). Sent all at once (first build): `audio`, `acquire` refused on phone every time, set let go.

`audio` message names app's streams: every stream whose client carries app's `policy.application.id` (process start time from `/proc/self/stat`, hex; Sailfish libpulse puts it on every stream it opens; `Call.audio_id` reads it same way) joins group `aliencall`, streams already playing included. Without that call worse off than no set: while `aliencall` set active, policy corks catch-all group Gecko's streams otherwise fall in. So no id → nothing asked; set manager refuses any part of, or with no manager to answer, let go at once.

Safety = where set lives. ohm watches each client's bus connection, drops every set of client whose connection goes: crash, kill, OOM killer, closing app → sound back, nobody acting. Nothing written outlives app; nothing for next run to clean. While app runs: set held exactly while call on, also given back on way out, unless mic may still be open: then nothing sent, set goes with bus connection, after process's streams gone *(inferred: PulseAudio sees client socket close at once; ohm's word comes over bus)*.

Read-only besides: ohm `Route.Manager.ActiveRoutes` and `AudioRouteChanged` signal, for whether sound is on something plugged in or paired.

Volume = other half. While call on (one policy knows, `aliencall` included), volume keys set call volume, not media: volume of streams with role `voip` (nemo `mainvolume`). Engine gives its streams no role; PulseAudio gives role-less stream media role `x-maemo` (`x-maemo-match.table`). So first builds' calls played at whatever media volume was (lower still on earpiece), keys did nothing: quiet at full volume, barely audible on earpiece.

Now, while call on (placed or answered until over; `calls.rs`, `talking`), app's `PULSE_PROP` holds `media.role=voip`; after, as app found it (`call_host::label_call_audio`). PulseAudio client lib adds that variable's props to sound-server connection as opened; server adds them to every stream on connection before table read; table doesn't overwrite role stream already has. Engine opens one connection first time it plays anything, keeps it for run: in run that played nothing before, call's page (comes up only once call on). Variable made before app has second thread, which is what lets it change while others run (comment on `label_call_audio` says why). *(From sources and capture of media role on call's stream; not yet heard on phone.)*

Leaves: engine's connection keeps role it opened with until sound server restarts and engine opens another next time it plays (cubeb `pulse_stream_init`). So webxdc app that played sound before run's first call opened it as media; that run's calls play at media volume, as before. One playing sound after a call plays it at call volume.

## What only a phone can answer

Ranked by what would sink feature. None needs another line of Piirit: two phones and an afternoon.

1. **Peer connection completes on device, with mic?** Yes: calls went through on 5.2.
2. **Other end's voice keeps playing with app in background?** View held active for it; claim: enough.
3. **Call's streams join call group?** `aliencall` set names them by app's process. Claim: engine audio streams opened by app's own process (ESR 91 runs engine in it), so carry its id. On phone they do: engine's client = app's process, with `policy.application.id` `Call.audio_id` reads. Outside call they're in group `player`: some lib in process holds set of that class for same id *(inferred; which unknown)*; `aliencall` set, naming id same way, takes its place for call. Silence in call → they don't. `pactl list sink-inputs` shows each stream's `policy.group`, `application.process.id`; `pactl list sinks` active port. ESR 153 moves page audio into own process: recheck then.
4. **Earpiece route plays from earpiece alone?** Yes: on phone voice call played from earpiece only, Media app's music paused. Music itself plays from earpiece and loudspeaker both (#100): phone's loudspeaker output drives the two. Policy's own loudspeaker for call = PulseAudio aborting over open mic ("Earpiece and loudspeaker"); claim: plays from loudspeaker once mic out of way.
5. **App sees own mic in PulseAudio?** App sandboxed; claim: PulseAudio lists its recording streams to it. If not, loudspeaker switch stays dimmed while mic open.
6. **What it sounds like.** Speakerphone echo; call's battery cost.
7. **Camera opens in engine, and turns?** Gecko reaches phone cameras via `gecko-camera`; claim: `facingMode` picks back one, engine lets one go before next opens. Other half: battery and heat of video call at VGA, 24 frames.

## What would not change

- **Call rings only while app runs.** No push service open to third-party client (`docs/PROJECT.md`): call heard only if app alive (minimised included) to receive its message within core's 120 s. Else missed call's row.
- **Camera focus.** Gecko opens phone camera via `gecko-camera`, which sets one parameter, `video-size`: no focus mode, no focus search ever started; lens stays at camera default. QR scanner and in-app camera set continuous focus and tap focus via QtMultimedia; page has no such control (Gecko ESR 91 has no focus constraint); `gecko-camera` loads plugins from dir fixed at its build, so app can't put own in front. Fix upstream: `focus-mode` `continuous-video` where camera offers one.
- **Harbour.** Calls add no executable, no off-list library, no permission not already asked. They add system-bus services Harbour doesn't list (ngfd, mce, ohm resource and route managers): validator can't see; QA may or may not accept. Each a `DBusInterface` in `CallCenter.qml` or `CallAudio.qml`; call works without any, on loudspeaker. PulseAudio client lib (mic watch) is on list. And browser engine, separate package here, promoted from webxdc feature to load-bearing part of headline one.

## Next

1. **Phone questions above**, before anything else.
2. **Chat list preview** of call still core's English sentence (`📞 Outgoing audio call`). Reference clients hand core translations of its call strings (`set_stock_strings`); Piirit hands none of any kind yet.

## Sources

Read, not remembered, at these versions:

- [`chatmail/core` v2.62.0][core]: `src/calls.rs`, `deltachat-jsonrpc/src/api.rs`, `.../api/types/calls.rs`, `.../api/types/events.rs`.
- [`deltachat/calls-webapp` v0.12.1][calls-webapp]: `src/lib/calls.ts`, `src/vite-env.d.ts`, `calls.js`, its CI, its licence.
- [`deltachat/deltachat-android`][android]: v2.43.0 `CallActivity`, `CallUtil` (last WebView-based client); master `CallCoordinator`, `CallItemView`, strings, for behaviour and words.
- [`deltachat/deltachat-ios`][ios]: `CallManager`, `CallViewController`.
- Sailfish OS: [`ngfd`][ngfd] and [`mce`][mce] (D-Bus interfaces), [`lipstick`][lipstick] (`notificationfeedbackplayer.cpp`), [`nemo-qml-plugin-notifications`][notifications], [`sailjail-permissions`][sailjail] (`Base`, `Audio`, `Phone`), [`voicecall`][voicecall], [`ohm-plugins-misc`][ohm] (route manager), [`embedlite-components`][embedlite] (`EmbedLiteWebrtcUI.js`, `ContentPermissionManager.js`), [`sailfish-components-webview`][webview] (`WebView.qml`), `gecko-dev`'s Sailfish patches.
- Two device reports linked above: still only public evidence either way on WebRTC on Sailfish phone.

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
