# Calls

- **Voice calls on for everybody**, as reference clients since Delta Chat 2.51 (experimental behind *Settings → Advanced* until #119 fixed). Ringing here: *Settings → Notifications → Calls*, reference clients' switch.
- Voice calls made on phone (Sailfish OS 5.2). Video: same page, not yet made on phone.
- **Core does call, Gecko does media, Piirit = glue.** Core: place, ring, accept, end, time out. Platform browser engine: peer connection, codecs, echo cancellation. Upstream calls-webapp: page driving them. Here: page host, call screen, phone side (ringtone, screen, notification).

## What the core does

`deltachat-rpc-server` 2.62 (bundled, `vendor/deltachat-rpc-server/SOURCE.md`) carries whole protocol; `rust/deltachat-jsonrpc/tests/real_server.rs` pins app-read parts vs binary.

- **Five methods.** `place_outgoing_call(account, chat, offer, has_video)` → message id = call. `accept_incoming_call(account, message, answer)`. `end_call(account, message)`: decline, cancel, hang-up; repeat not error. `call_info(account, message)` → `{sdpOffer, hasVideo, state}`, `kind`: `Alerting`, `Active`, `Completed` (+ `duration` s), `Missed`, `Declined`, `Canceled`. `ice_servers(account)` → JSON *string* of TURN servers, passed to page as is.
- **Four events** `IncomingCall`, `IncomingCallAccepted`, `OutgoingCallAccepted`, `CallEnded`: **snake_case** (`msg_id`, `chat_id`, `place_call_info`, `has_video`, `from_this_device`, `accept_call_info`); rest of stream camelCase. `msgId` reads nothing, silently. `calls.rs`, tests use real shape.
- **Chat = signalling.** Call = `Viewtype::Call` message, offer in header; accept/end = hidden messages quoting it. Core never reads SDP: media client's alone.
- **Rings 120 s**, then stale both ends. Core timer ends it (incoming Missed, outgoing Declined), one `CallEnded`, only in server that heard call: lost on restart or event overflow. So ringing here self-stops at 125 s (`calls.rs`, `RING_LIMIT`).
- **Ringing call: no `IncomingMsg`**; rung-out one only `MsgsChanged`, `CallEnded`. Missed-call notification = client's.
- **One-to-one, encrypted only**: refuses group ("Can only place calls in single chats"), Saved messages ("Cannot call self"); no peer key = can't send.
- **`who_can_call_me`**: 0 everybody, 1 contacts (default), 2 nobody. Per device, unsynced, but in profile DB (backup, second device take it). Disallowed call still stored, answerable from chat. Piirit writes 2 only when ringing switched off.

## How a call works here

### The row

- `MessageDelegate.qml`: kind/outcome (*Missed call*, *Declined call*, *Canceled call*), duration, Delta Chat's words (its translations apply). State via `calls::summary` (filled by `chat.rs` like webxdc row); core's English text only when core won't give state.
- Tap: ringing here → take up; else call back (deltachat-android rule).

### Placing one

- Contact page **Calls** tile (`MediaKinds.qml`) while chat takes calls: one-to-one, encrypted, taking messages (core's conditions, `chat.rs` `takes_calls`).
- `CallsPage.qml`: chat's calls from media index (`chat_media.rs`, kind `calls`, core `Call` view type), newest first: direction, outcome, when. First row *Call Ada* places one; tap on listed call = tap on chat row.
- No call button in chat header: left = back gesture, right = contact page.

### The call

- One `Call` (`calls.rs`) per window, in `CallCenter.qml`: belongs to no page; leaving page ≠ hang-up.
- States `ringing`, `starting`, `calling`, `connecting`, `connected`, `reconnecting`, `ended`. End reason: hung up here, declined here, ended there, answered on another device, missed, failed.
- One call at a time (as reference clients): second rings nowhere, missed once core gives up.
- `CallPage.qml`: who, state, phone's language; Decline, Answer while ringing; then call's page. Not swipeable away while up (takes sound). Notification tap, cover action → call to front, not pop.
- Cover: other end's picture faint, enlarged behind (avatar if none); name; duration; mic-off, camera-paused notes. One quick action = mic: hang-up too easy by mistake; camera released in background anyway.

### The page

- Media = upstream [`calls-webapp`][calls-webapp] v0.12.1: WebRTC "for integration into Delta Chat clients"; deltachat-android's until native in v2.47. One self-contained `.html` release asset, committed (`vendor/calls-webapp/`, `scripts/fetch-calls-webapp.sh`, `ci/vendor-check.sh`), compiled in.
- `call_host.rs` serves it on own loopback, as `webxdc_host.rs`: kernel port, `Host:` must match, core behind unguessable token, nothing after call.
- Page loads classic `calls.js` first; host answers with bridge (`rust/piirit-shim/src/calls.js`). Bridge static, secretless: token in page address (`?key=`), only app's WebView gets it; cross-origin includer finds no key.

Bridge:

- `startCall(offer)`, `acceptCall(answer)`, `endCall()` → host places, accepts, ends.
- `getIceServers()` asks host (TURN credentials = account's, not in script). `getAvatar()` names other end's picture, host-served.
- Long poll brings other end's answer (page hash command `#onAnswer=`) and app mic switch (`mute`/`unmute`): bridge presses page's switch (by label), not track: page keeps state, tells other end.
- Page reports no connection health: bridge watches peer connection, reports ICE states → *Connected*, clock; failure ends call.
- Requests to host one at a time, in order: own connection each, two in flight seen reversed (stale `/video`).
- Gecko < 126 lacks `RTCIceCandidate.type` (page sends offer at first relay candidate, else after gathering): bridge parses candidate line.

Page opened on `#startCall` or `#acceptCall=<offer>`, `?noOutgoingVideoInitially` unless camera starts on; never `?disableVideoCompletely` (drops other end's picture too). Page policy keeps fetches same-origin; peer connection not a fetch. Offer to core only after enough ICE. Hung up while core placing → ended on landing. Answer before call knows its message (unordered): kept, applied after (`tests/call_early_answer.rs`). Native Android, iOS reimplement same protocol (negotiated data channels, "enough ICE, then trickle"): interoperable.

### The screen

- `qml/pages/CallPage.qml` = phone call screen: who top left, time of day top right; duration (h:m:s) large below; switches; *End call* at foot. Other end's own picture behind, blurred, dimmed.
- Switches: mic, loudspeaker ("Earpiece and loudspeaker"), camera. Keypad, recording = phone call's.
- Media page's English controls run unseen under call screen, behind tap-catcher; call screen does their jobs.

### Video

Same call, camera on: same page, peer connection (video track negotiated every call), five methods; core told `has_video` on place. Engine encodes, decodes; no FFmpeg or other lib linked for it.

Camera = bridge's (`rust/piirit-shim/src/call_video.js`, before `calls.js`):

- **Closed until wanted.** Page would hold camera open all call. Gets mic plus, voice call, blank canvas track (no frames asked): switch still drawn, voice → video possible. On: camera track replaces blank on stream and sender (`replaceTrack`; page can't renegotiate). Off: stopped, blank back.
- **Page's switch pressed** (*Start camera* / *Stop camera*), as mic: page tells other end via data channel; other end hides picture, no frozen frame.
- **One camera at a time.** Turn: stop open one, ask other by device id (name says facing, else any other): `facingMode` mere preference, Gecko returned same camera. Mirror, report per camera's own facing; old mirroring kept till new camera in (last frame doesn't flip).
- Gecko names all back cameras alike; wide lens may list first. Back = place of label nearest `1.0` in `/apps/jolla-camera/backCameraLabels` (read only); first if none; logged (`(2 of 2)`). Same order both lists: inferred.
- Other won't open → first reopened; neither → failed with engine's words, logged `piirit: the camera would not open: …`, switched off.
- **Phone-sized.** Asked 640×480, 24 fps; every video sender capped 700 kbit/s, 24 fps once negotiated (pixels not captured = battery). Preferences only: required (`max`, `min`, `exact`) no mode meets = Gecko opens nothing, vs mode max (30 fps camera fails `max: 24`). Cap on sender instead.
- **VP8 first.** Seen: answered call showed picture, placed didn't. Sailfish Gecko enables hardware H264 (`media.webrtc.hw.h264.enabled`) → offers H264 before VP8; answers follow offer order. Sent offer lists VP8 first (`calls.js`), else unchanged. Moot for iOS Delta Chat: H264 only (seen in log).
- **Own H264 encoder.** Gecko decodes H264 via phone decoder, encodes only via media plugin (own process, dirs in `MOZ_GMP_PATH`). Sailfish's = gmp-droid (hardware), not on/installable everywhere: call to iPhone sent no frames. Package ships x264 behind Gecko plugin API at `/usr/share/harbour-piirit/lib/gmp-piirit/1.0/` (`rust/piirit-video/csrc/gmp_h264.cpp`; same x264 as app, docs/BUILDING.md).
  - `prepare_media_plugins` (`call_host.rs`) appends to `MOZ_GMP_PATH` before start; skipped if plugin there already encodes H264 (gmp-droid on capable hardware, cheaper).
  - Cheap: Constrained Baseline (all decode), fastest x264 preset keeping deblocking, 1 thread, no lookahead; camera buffers read in place; bit rate per Gecko congestion control, made good per second on fewer camera frames; keyframe + SPS/PPS on request. Android ends answer VP8, never start it.
  - Logs to `PIIRIT_CALL_LOG`: start size, rate; first picture; every 30 s and at end pictures, keyframes, CPU each, bit rate. `rust/piirit-video/tests/gmp_h264.rs` drives it as Gecko, via `tests/gmp_check.cpp`.
- **Background**: camera stopped (other end told off), pictures not drawn; reopened on return if on.
- **Reported** on `/video` → `Call`'s `local_video` (camera live), `remote_video` (other picture drawn: track with frames, not switched off).
- **Logged** via `/log`: camera opened, size; open/turn failure, why; turn time to open, to first picture; negotiation steps; agreed sections, codecs in order; other picture's frames, its camera state. Few s, 15 s, 45 s after connect or camera change, each way: frames, bytes/frame (black = dozens, picture = thousands), keyframes asked (repeat = can't decode), sender track, each picture hidden/playing/size, `mozPaintedFrames`. Also `/video` reports, engine hiding page, button presses (touch or bridge), put-backs. Page console: warnings, errors, uncaught failures, words on other picture, own commands; never candidates, descriptions (addresses).
- Lines → stderr `piirit: call: …` and `~/.cache/piirit/piirit/call.log` (fresh per call, seconds-since stamps; encoder lines `encoder`). On phone read file: launcher drops stderr, journal empty.

Call screen shows page's two videos only (bridge hides rest), own lines, switches over on shade.

- Own picture fills screen until other's comes, then top right (mirrored only from front camera), stays (as reference clients). Invisible item over corner holds sole turn button: no turn while own fills screen. Dragging dropped: lagged finger on phone, even on own layer.
- Other picture enlarged toward fill, max 1.35× (upright fills tall screen; sideways loses some bars, not half). Laid out larger, not transform-scaled (extra layer).
- **Put back**: picture meant seen (not hidden, playing, has frames, page front), none painted in 2 s → frames off and on, stream given again (as camera turn; drew own picture on phone where none was). Before first paint (since shown or new stream): black, checked every 0.5 s → put back within 1 s of frames. Max 3 per call, then logged. Engine not counting paints: once, one look after shown. Paused → asked to play until it does (other picture carries voice).
- `/video` carries `localPainted`, `remotePainted`; spinner over unpainted picture (whole screen or corner). Given up (3 put-backs) reported showing: spinner stops.
- Turn: own picture black till new camera's first frame (1–2 s on phone); spinner; not checked meanwhile.
- Tap hides lines; auto-hide once both ends seen; time of day → duration, large clock goes.
- Camera switch every call, and on video call's ringing screen (answer without camera). Turn button beside it while own picture fills screen; then picture's lower corner nearer screen middle, hiding with lines; only once picture shows. On picture: small disc, little over icon size, room to press. Pressed: stays, dimmed, inert until other camera shows (nothing moves under finger); max 10 s.
- Video call answered with front camera. Own picture plays muted when engine lets; paused one asked to play until it does.
- Pictures up: screen lit, ear shield off (held in front of face).

Placing: Calls page, *Video call Ada* under *Call Ada*. Tap on video call's row calls back camera on (as reference clients).

## What the phone gives a call

System call screen, history, lock-screen call window = voicecall's; way in = provider plugin `.so` in `/usr/lib/voicecall/plugins`: outside Harbour paths, unconfined in voicecall-manager, needs disallowed `Phone` permission. Apps doing it (a WhatsApp client, RooTelegram) ship outside Harbour. `Sailfish.Telephony` = SIM selection only. None here. Here:

| | How | Allowed |
|---|---|---|
| **Ringtone** | ngfd (every phone sound) plays `voip_ringtone`: non-phone tone (profile `voip.alert.tone`; silent if `voip.alert.enabled` off), ring volume, obeys silent profile, repeats. ngfd fails → `ringtone` with `type` `voip`, as voicecall for non-SIM call | ngfd system bus, sailjail opens to all; not Harbour-listed |
| **Music held off** | ringing: ohm set class `ringtone`, as phone: music pauses, resumes if unanswered | ohm resource manager, Audio permission; not Harbour-listed |
| **Screen and call state** | mce told `ringing`, `active`, `none`: lights screen, phone in call; forgets if app goes. `ringing` only after ngfd answers ringtone Play, or 1 s: else ngfd (follows mce) plays call-waiting beep for `voip_ringtone` (#98) | mce request interface, open to all; not Harbour-listed |
| **Lock screen** | mce says screen on → `req_tklock_mode_change("unlocked")`, app forward, call page shown (behind lock it got covered, #99). mce lights for ringing only as for phone call (not in pocket), relocks after. Device lock (code, fingerprint) kept, page behind. Plus Critical notification, Decline/Answer, on lock screen | mce request interface; `Nemo.Notifications` |
| **Missed calls** | category `x-nemo.call.missed`, Call back | `Nemo.Notifications` |
| **Staying awake** | CPU up in call (suspend kills sound) | `Nemo.KeepAlive` |
| **At the ear** | proximity → black touch-proof cover (app can't blank display) | `QtSensors` |
| **In the background** | `WebView` held active once page up (inactive = hidden document, media paused) | `Sailfish.WebView` |
| **Microphone and camera** | granted to call page for session up front (else prompt every call: new port = new origin). Never engine-wide (webxdc apps too) | `Sailfish.WebEngine` |
| **Screen on** | lit while pictures show, while ringing | `Nemo.KeepAlive` |

Left out on purpose:

- **ohm route preference** `Route.Manager.Prefer("earpiece")` (Audio permission): global, ohm doesn't know requester: crash mid-call leaves earpiece until next phone call/restart. Phone app's loudspeaker (`Playback.Manager.RequestPrivacyOverride`) equally global.
- **`call` resource class**: voicecall's, needs Cellular credential, drives modem paths. Harbour-listed `libaudioresource`: only `player`, `game`; neither reaches earpiece.
- **`Nemo.Ngf`** (QML ringing): not Harbour-listed, hence bus.

### Earpiece and loudspeaker

- Voice → earpiece (as phone). Video → loudspeaker (as reference clients; held in front of face); voice call gaining pictures too, unless reader switched.
- Switch any time, holds rest of call; ~1 s, own mic off meanwhile, switch busy till mic back. Headset, headphones, Bluetooth take sound: switch dimmed. Policy not taking call → loudspeaker for good, dimmed.

How (`CallAudio.qml`):

- Whole call: set class `aliencall` with ohm resource manager (`org.maemo.resource.manager`, system bus, Audio permission); Sailfish's non-phone call class (Android App Support VoIP apps).
- ohm routes audio-playback owner by class; `aliencall`: Bluetooth/USB, wired headset/headphones, earpiece (`earpieceforalien`). Also owning video playback → loudspeaker (`ihfforalien`). Switch = update one set. Music paused either way. Nothing global.
- Ringing: second set, class `ringtone` (phone ringing's, outranks media player, `resource_classes.pl`): music pauses, resumes if unanswered. Audio playback only, no streams named (ngfd plays, policy files ring streams apart): moves nothing of app's, no mic open. Answer: call set acquired, then ring set released, back to back (no music between). Refused → let go, music plays through. *(From policy rules; not yet heard on phone.)*

Mic = hazard: moving sound with mic open kills PulseAudio. Policy module moves every group stream, asserts each moved (`policy-group.c`, `pa_policy_group_start_move_all`); engine opens mic on named device, cubeb marks `PA_STREAM_DONT_MOVE`. Assert aborts, systemd restarts; restarted one forgets call streams → silent call; few aborts → no restart, no sound until manual start. First loudspeaker did it (#146); so did releasing set after earpiece call, mic open *(inferred: abort at hang-up in captures)*. Headset plugged mid-call same; platform's to fix.

So never move sound while mic may be open:

- **Before it opens.** Set acquired, policy given 0.5 s, before call page (`CallView.allowed`): ohm answers acquire ms before routing PulseAudio.
- **To switch**: page told `microphone-off`; bridge stops track, off sender (page stream keeps it for mute switch). PulseAudio shows no app recording 300 ms → set updated; 0.5 s after policy answers, `microphone-on`: new track, on/off as page had. Not done in 5 s → abandoned: mic back, sound unmoved.
- **Once call over**, set released only after mic shut.

- `MicWatch`: app's recording streams via PulseAudio client lib (Harbour-listed), client = app process. On phone, unmovable stream = engine's, in app process (`AudioCallbackDriver`, `application.process.binary` `harbour-piirit`). Pid is sandbox's; other sandboxed app may share it, its recording counts as app's: safe side (switch gives up, set held). Watch blind (PulseAudio down/hidden): switch dimmed while mic open; set released 3 s after mic stop told.
- Bus messages in libresource layout (`register`, `audio`, `update`, `acquire`, `release`, `unregister`; `dbus-msg.c`), no Harbour-listed lib; one at a time, each after last answered: manager answers register only after asking bus who sent it, refuses earlier ones as `EUCLEAN` (ohm `manager.c`). Sent at once (first build): `audio`, `acquire` always refused.
- `audio`: streams whose client carries app's `policy.application.id` (process start time from `/proc/self/stat`, hex; Sailfish libpulse sets on every stream; `Call.audio_id` reads same) join `aliencall`, playing ones too. Else worse than no set: active `aliencall` corks catch-all group Gecko's streams fall in. No id → nothing asked; any refusal or no manager → let go.
- Safety: ohm drops sets of client whose bus connection goes (crash, kill, OOM, close) → sound restored unaided; nothing persisted to clean. Set held exactly while call on, released on exit, unless mic may be open: nothing sent, set dies with bus connection, after streams *(inferred: PulseAudio sees socket close at once; ohm hears via bus)*.
- Read-only: ohm `Route.Manager.ActiveRoutes`, `AudioRouteChanged` (plugged/paired device?).

Volume:

- Call on (policy-known, `aliencall` included): keys set call volume = role `voip` streams (nemo `mainvolume`). Engine sets no role; PulseAudio gives role-less stream media role `x-maemo` (`x-maemo-match.table`). First builds: calls at media volume (lower on earpiece), keys dead.
- Now while call on (placed/answered till over; `calls.rs`, `talking`), `PULSE_PROP` holds `media.role=voip`; restored after (`call_host::label_call_audio`). Client lib adds it to connection on open; server stamps streams before table, table keeps existing role. Engine opens one connection at first sound, keeps it for run: in silent run = call's page (up only once call on). Variable created before second thread, so changeable later (comment on `label_call_audio`). *(From sources, capture of call stream role; not yet heard on phone.)*
- Connection keeps opening role until sound server restart and engine reopens (cubeb `pulse_stream_init`): webxdc sound before run's first call → its calls at media volume; sound after a call → call volume.

## What only a phone can answer

Ranked by what would sink feature. None needs more code: two phones, an afternoon.

1. **Peer connection completes on device, with mic?** Yes: calls through on 5.2.
2. **Other voice plays with app in background?** View held active; claim: enough.
3. **Call's streams join call group?** Claim: engine streams opened in app process (ESR 91), carry its id. On phone: yes, client = app process, `policy.application.id` as `Call.audio_id`. Outside call: group `player` (some lib holds set of that class for same id *(inferred; which unknown)*); `aliencall` set replaces it in call. Silence in call → not joined. `pactl list sink-inputs`: `policy.group`, `application.process.id`; `pactl list sinks`: active port. ESR 153 moves page audio to own process: recheck.
4. **Earpiece route = earpiece alone?** Yes: voice from earpiece only, Media app music paused. Music itself plays from earpiece + loudspeaker (#100): loudspeaker output drives both. Policy loudspeaker = PulseAudio abort over open mic; claim: plays from loudspeaker once mic out of way.
5. **App sees own mic in PulseAudio?** Sandboxed; claim: yes. Else loudspeaker switch dimmed while mic open.
6. **Sound**: speakerphone echo; battery cost.
7. **Camera opens in engine, turns?** Via `gecko-camera`; claim: `facingMode` picks back, one released before next opens. Plus battery, heat at VGA 24 fps.

## What would not change

- **Rings only while app runs** (minimised included): no third-party push (`docs/PROJECT.md`); message must arrive within core's 120 s, else missed-call row.
- **Camera focus.** `gecko-camera` sets only `video-size`: no focus mode, no focus search; lens at default. QR scanner, in-app camera: continuous + tap focus via QtMultimedia; page can't (Gecko ESR 91: no focus constraint); `gecko-camera` plugin dir fixed at build. Upstream fix: `focus-mode` `continuous-video` where offered.
- **Harbour.** No new executable, off-list lib, permission. Unlisted system-bus services (ngfd, mce, ohm resource, route managers): validator blind, QA may or may not accept; each a `DBusInterface` in `CallCenter.qml` or `CallAudio.qml`, call works without any (loudspeaker). Mic-watch PulseAudio lib listed. Browser engine (separate package): webxdc extra → load-bearing.

## Next

1. **Phone questions above**, first.
2. **Chat list preview** still core's English (`📞 Outgoing audio call`). Reference clients pass translations (`set_stock_strings`); Piirit none yet.

## Sources

Read, not remembered, at these versions:

- [`chatmail/core` v2.62.0][core]: `src/calls.rs`, `deltachat-jsonrpc/src/api.rs`, `.../api/types/calls.rs`, `.../api/types/events.rs`.
- [`deltachat/calls-webapp` v0.12.1][calls-webapp]: `src/lib/calls.ts`, `src/vite-env.d.ts`, `calls.js`, CI, licence.
- [`deltachat/deltachat-android`][android]: v2.43.0 `CallActivity`, `CallUtil` (last WebView client); master `CallCoordinator`, `CallItemView`, strings.
- [`deltachat/deltachat-ios`][ios]: `CallManager`, `CallViewController`.
- Sailfish OS: [`ngfd`][ngfd], [`mce`][mce] (D-Bus), [`lipstick`][lipstick] (`notificationfeedbackplayer.cpp`), [`nemo-qml-plugin-notifications`][notifications], [`sailjail-permissions`][sailjail] (`Base`, `Audio`, `Phone`), [`voicecall`][voicecall], [`ohm-plugins-misc`][ohm] (route manager), [`embedlite-components`][embedlite] (`EmbedLiteWebrtcUI.js`, `ContentPermissionManager.js`), [`sailfish-components-webview`][webview] (`WebView.qml`), `gecko-dev` Sailfish patches.
- Two device reports linked above: still only public evidence on WebRTC on Sailfish phone.

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
