# Power

*What running Postivene in the background costs, and what is done about it.*

Sailfish has no push service a third-party client may use, and Harbour
allows no background daemon. The only way a message reaches the phone is
the app itself, minimised, holding a connection. That is the whole design
constraint: the app cannot stop receiving to save power, because not
receiving is the same as not working.

What is left is making the receiving cheap.

## Where the power goes

On a phone in a pocket with the screen off, three things draw, and they
are not close to equal:

1. **Radio wakeups.** The modem or wi-fi chip leaving its low-power state,
   and then holding a high-power state for seconds afterwards. One packet
   costs about what twenty cost. This dominates everything else.
2. **CPU wakeups.** The chip leaving suspend. Any timer, any arriving
   event.
3. **The screen.** Off. Nothing.

So the thing to count is wakeups, not cycles.

## What the core already does right

Checked against the bundled `deltachat-rpc-server` (v2.60.0) rather than
assumed, by asking it for `sys.config_keys` and reading the defaults back:

- **One IMAP connection.** The keys that used to open a second and third
  one -- `sentbox_watch`, `mvbox_move`, `only_fetch_mvbox` -- do not exist
  in this core any more. There is one folder and one connection, and no
  setting can add another. The usual Delta Chat power advice is about a
  core that no longer ships.
- **`disable_idle` is 0.** IMAP IDLE rather than polling, which is the
  cheaper of the two by a wide margin.
- **`bcc_self` is 0.** No second copy of every sent message going out over
  SMTP and coming back over IMAP.
- **Webxdc realtime is never used.** The core can open a peer-to-peer
  channel for a webxdc app, which would mean a second always-on socket and
  NAT keepalives. Postivene implements none of that side of the webxdc
  API, so the peer-to-peer stack never starts.

And on the app's side:

- **The event stream is a long poll, not a poll loop.**
  `get_next_event_batch` blocks in the server until there is something to
  say. The 50 ms retry in `rust/deltachat-jsonrpc/src/events.rs` is the
  error path only.
- **No repeating timer runs while the app is in the background.** The one
  endless animation is the voice recorder's, which only runs while
  recording.
- **Attachments stop at 1 MiB by default**, so a photo arrives and a video
  waits to be asked for.

## What the app does about the rest

None of these gives up a message:

### Stopping IO while there is no network at all

When the phone has no connection, the core keeps trying to make one:
a name lookup, a TCP connect, a failure, a wait, again. Every attempt
wakes the radio and none of them can succeed. A tunnel, a basement, a lift,
a flight -- an hour of that is dozens of wakeups bought for nothing.

So `NetworkWatch` listens for connman saying the network is gone, and after
it has stayed gone for a while the window stops the core's IO. When connman
says the network is back, IO starts again and the core is asked to look at
it (`maybe_network`).

This is the opposite of a change that was considered and rejected: stopping
IO when the app is *backgrounded* would destroy the only path by which
messages arrive, and is never done. Stopping IO when there is *no network*
gives up nothing, because nothing can arrive over a network that is not
there.

It is deliberately timid about it. IO is only ever stopped after connman has
positively said the network is gone and stayed saying it -- an unknown state
never stops anything -- and it is started again by any of three separate
things: connman saying the network is back, the app being brought to the
front, or the core being restarted under it. A watcher that got it wrong
costs a reconnection; a watcher that got it wrong and had only one way back
would cost the messages.

### Not handing out events nobody is listening for

Every event from the core used to be serialised to JSON and handed to every
listener in the app -- each page still on the stack, and one list per
profile on the cover -- whether or not anything wanted that kind of event.
The core is chatty, so a sync of a few hundred messages meant a great deal
of string work with the screen off.

The events any part of the app actually reads are a known set, so the rest
are dropped before they are serialised.

## Measuring it

None of the above is worth believing without a number, and the number is
hard to get honestly: night-to-night variance on a phone is large, and cell
signal strength alone can move idle draw by a factor of two.

`scripts/battery-probe.sh` samples the battery over a long idle stretch and
records the conditions it ran under, so a pair of runs can be checked for
comparability afterwards rather than taken on trust. Copy it to the phone
and run it; it is not part of the package.

```
scp scripts/battery-probe.sh nemo@phone:
ssh nemo@phone ./battery-probe.sh --label with-app
# another night
ssh nemo@phone ./battery-probe.sh --label without-app
ssh nemo@phone ./battery-probe.sh --compare postivene-power/with-app-*.log \
                                            postivene-power/without-app-*.log
```

### Conditions

The comparison is only worth anything if one thing differs. Everything in
this list has to be the same on both nights.

**The one thing that differs**

- Night A: Postivene minimised (open it, then swipe to the home screen --
  do not close it). Night B: Postivene closed (swipe it away, and check
  with `pgrep -f harbour-postivene` that nothing is left).

**Radios** -- the largest source of noise, so pin all of them.

- Flight mode **off**. This measures a connected phone.
- Wi-fi **on and connected**, to the same network both nights.
- Mobile data: pick one and keep it. **Off** gives a quieter number,
  because cell signal drift is the biggest single source of variance.
  **On** is more realistic. Do not mix.
- Bluetooth **off**.
- Location/GPS **off**.
- No VPN, no wireguard, no tunnel of any kind. The script records
  `tunnel devices` so a forgotten one is visible afterwards.

**The phone**

- Same phone, same SIM, same SD card.
- **Not charging**, and unplugged for at least an hour before starting.
  The script refuses to start on a charger and stops if one appears.
- Start both nights at a similar charge, ideally 80-90 %. Discharge rate is
  not linear in state of charge, so 90->80 and 40->30 are not the same
  measurement.
- **The same physical place.** Signal strength dominates radio power, so
  the same spot on the same shelf, not "somewhere in the bedroom".
- Screen off, face down, untouched. No alarms, no incoming calls.

**Everything else on the phone**

- The same other apps running -- ideally none. Close everything.
- The same accounts syncing: email, calendar, contacts. Do not add or
  remove one between the nights.
- The same notification settings.

**The run**

- At least six hours, ideally eight.
- **Alternate and repeat**: A, B, A, B, over four nights. Two single nights
  are not enough to tell a real difference from a quiet night, and the
  effect being looked for is smaller than the night-to-night variance.
- Roughly the same amount of traffic each night. A night with two hundred
  messages is not comparable to a night with three; the app's own chat list
  will say how many arrived.

### Reading the result

The difference between the two conditions is the entire cost of running
Postivene in the background, and the ceiling on what any optimisation in
this document can ever save. If that difference is small, the work is done
and anything further is not worth doing.

Run under `devel-su` and the log also carries the kernel's wakeup counters,
which say *what* woke the chip rather than only how much it cost. That is
the number to look at if the cost turns out to be worth chasing.
