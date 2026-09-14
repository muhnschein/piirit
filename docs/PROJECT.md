# Postivene

*A native SailfishOS client for [Delta Chat](https://delta.chat).*

## What this is

Delta Chat is a chatmail messenger with end-to-end encryption, no phone
number and no central operator.

The thesis is narrow: **do not build a messenger, build a SailfishOS UI on
top of one.** Protocol, cryptography and storage are the upstream core's.
Postivene contributes the presentation layer, the platform integration and
the packaging, and aspires to ships to Jolla's Harbour app store.

## What this isn't

- **Reimplementing any protocol logic** — no IMAP/SMTP/MIME, no Autocrypt,
  no encryption. If protocol code is being written, the core dependency is
  being misused. This is the most important boundary in the project.
- **Hand-written C FFI bindings.** The CFFI exists; JSON-RPC is the
  sanctioned, lower-maintenance path.
- **A push-notification service.** No Delta Chat push infrastructure is
  available to third-party clients.
- **Plain-email chats.**
- **Multi-protocol bridging**, a **desktop or web build**, and **running a
  chatmail server**. Single-purpose client only.
- **Old Sailfish releases.** One modern baseline, expanded only for future
  Jolla products. [Buy a Jolla Phone 2026](https://commerce.jolla.com/) and
  support European-made alternatives. 👊🇪🇺🔥
- **Shipping via OpenRepos/Chum.** Harbour or nothing: a platform that is
  competitive for regular, non-technical people is one that needs no
  developer mode, no SSH to fix small things, and no community repos.
  That means [dogfooding](https://en.wikipedia.org/wiki/Eating_your_own_dog_food),
  and telling Jolla what the platform is still missing.

## Architecture

```
QML / Silica UI
        |  models / signals
Rust shim (qmetaobject-rs): JSON-RPC client, event loop -> Qt queued
signals, QAbstractListModel adapters for chats/messages/accounts
        |  JSON-RPC over stdio
deltachat-rpc-server (bundled binary, subprocess) = the entire core
```

- **The shim spawns the server as a subprocess.** This keeps the
  integration surface small and stable, and mirrors the desktop client's
  own migration away from CFFI. The OpenRPC spec is the interface
  contract.
- **Core events run off the main thread**, marshalled to the Qt main
  thread via queued signals.
- **A webxdc app is served, not unpacked.** All of it is behind one
  setting, off until asked for: "Enable webxdc apps (experimental)",
  `webxdc_enabled` in dconf. Off, the tray has no app entry, the store is
  unreachable, and a `.xdc` somebody sent is drawn as the file it is and
  handed on by a tap. The gate is three bindings on that one value rather
  than a check at each door, so the tray entry and the row are not there
  to tap and nothing has to be kept in step. It is a switch rather than a
  build flag because running somebody else's code is the reader's
  decision.

  An app is a zip with an `index.html` in it, and the core reads the
  archive (`get_webxdc_blob`). The shim puts one app on a loopback
  address of its own while it is open and answers every request out of the
  core (`webxdc_host.rs`); the `WebView` is pointed there and needs
  nothing else. The app sits at the root, as every other client serves
  one: a bundler's default output asks for `/assets/index-1a2b.js`, so a
  host keeping the files under a prefix draws a blank screen for any app
  not using relative paths. The token guards the chat, not the files --
  the two API paths are under `/webxdc-api/<token>/`, and the files were
  sent to this reader anyway. The same host carries that API
  (`sendUpdate` is a POST, everyone else's updates are a poll), so the
  bridge is not a Gecko frame script and no archive format is parsed here.

  An app can hand a file back. The call is `sendToChat`, but the button an
  app draws over it is a *download* -- `sharer`'s is a download arrow --
  and what a reader means by that is the file, on their phone. So the host
  writes what the app gives it into the cache and raises it on the page,
  and the page saves a copy into Downloads and says so. Neither a chat nor
  a question in front of it: a chooser between opening and keeping is two
  taps ahead of the one thing already asked for. Text with no file goes on
  the clipboard instead.

  The file *is* the request body -- no base64, no JSON around it, its name
  and any words in the query -- copied from the socket into the cache a
  chunk at a time, because wrapping it would hold the whole of it several
  times over at both ends and a file worth exporting is exactly the size
  that cannot afford it. There is no cap: what bounds a file is the disk,
  and a write with no room is a `500` the app can show. That needs the
  cleanup to exist, so the page deletes the cached copy once the file is
  saved, and starting an app empties its outbox -- the one moment none of
  its own handovers can be in flight. (`deltachat-android` has the same
  leak in a worse place: its blobs go to the app's data directory, which
  the system will not reclaim.)

  Two more deliberate things. The app's request is answered *before* the
  page is told, and only if that answer got out. And the page keeps
  serving an app it has opened a page over: leaving is destruction, which
  is what a popped page and a replaced stack both are, while deactivation
  also fires for a picker the app itself asked for. A body this host will
  not take is read and dropped before it is refused, because answering and
  closing on a client still writing resets the connection and a reset
  reads to the app as a host it cannot reach. What has already come off
  the socket is counted, so a half-read body is not waited on twice.

  A new app comes from the store, a website (`WebxdcStorePage.qml`); a tap
  on a link to a `.xdc` is caught before the engine can download it and
  fetched through the core instead (`get_http_response`), which is
  deltachat-android's shape too. It catches the tap where the tap happens:
  `qml/webxdc/catch.js` is a frame script in the engine's own world, which
  stops the click and sends the address back. deltachat-android decides
  every navigation in `shouldOverrideUrlLoading`; this `WebView` has no
  such hook, and watching where the view goes is not one, since a download
  is not a navigation.
- **The app is in the phone's share sheet, and installs nothing to be in
  it.** A picture from the gallery or a link from the browser reaches
  Postivene because the desktop entry says so -- `X-Share-Methods`, and a
  group per method saying what it is called and what it takes -- and
  because a `ShareProvider` of the same name runs in the app
  (`qml/share/ShareTarget.qml`). The older way, a transfer-engine plugin,
  is a `.so` in a system directory and is not open to a Harbour package;
  this is, and `tests/qml_syntax.rs` checks that the names in the two
  files still agree. What arrives goes to the window, which asks which
  chat it is for and opens that chat with the file on its attachment bar
  or the text in its field.
- **The `WebView`'s own bindings are left alone.** Silica's `WebView.qml`
  decides when the engine renders, from the page's status and whether the
  app is in front; overriding `active` leaves the view unactivated by the
  page transition and drawing as a grey rectangle. Where the view is
  pointed is a plain `url:` binding for the same sort of reason.
  `tests/qml_syntax.rs` keeps both that way.
- **A `WebView` that draws nothing says why.** Nothing about the browser
  engine can be tested off a phone, so a failure is put where the app
  would have been: the host answers a refused blob with the core's own
  reason rather than an empty body, and the page keeps that reason on
  screen. The banner clears itself after a few seconds, which is right for
  something that happened and wrong for a view that never drew anything.
- **The header over a conversation is the app's own, and a group's says
  how many are in it.** Silica's `PageHeader` draws its title in a label
  of its own and offers no `textFormat`, so a chat named
  `<img src="https://tracker/p.gif">` would be markup to it and drawing
  the header would fetch the image -- from an app whose whole point is
  that the network cannot watch. `ConversationHeader.qml` is the same
  header laid out by hand, with its one label pinned to plain text. Under
  the name, where `PageHeader` puts a description, a group carries its
  member count: the conversation model reads it with the chat's shape
  (`get_chat_contacts`) so a group opens with the count already on it, and
  re-reads it on the events somebody could have joined or left on -- the
  same ones the name follows. A one-to-one chat carries nothing there,
  because the number would say "2" about a conversation with one other
  person. The header keeps `PageHeader`'s height whatever is in it, since
  what sits below starts where it starts, so the second line is drawn only
  while both fit inside that height.
- **A bubble holds a remark; anything longer gets a page.** A message over
  a dozen lines is folded in the conversation, with View full message
  under it: drawn whole, somebody's to-do document fills the screen and
  leaves a row nobody can scroll past. The fold is a cap on the label's
  lines rather than a cut in the text, so nothing has to slice a rendering
  in half and leave a tag open. A page rather than an unfold in place,
  which would make exactly the row nobody can scroll past and would have
  to put the reader back by hand afterwards.

  Past a length the core does not carry a message whole at all: it cuts
  the body and puts the rest in an HTML part, so the whole of such a
  message is only behind `get_message_html` -- read as words, never
  rendered as markup (`html.rs`), for the reason every label in the app is
  pinned to plain text. A newline in that part is *not* a line break: in
  HTML it is whitespace, and the core writes each line as `line<br/>` with
  a newline after the tag, so counting both puts a blank line between
  every line. Whitespace between the markup is collapsed the way a browser
  collapses it, and the tag is the break; two `<br/>` in a row are still
  two, so a blank line the reader typed survives as one. The fake core's
  fixture is written in that shape so the suite sees it.

  The offer belongs to a body: an attachment with no caption has none to
  read on a page, and a message the core is still holding back has none of
  it here yet. The same rule from the other end is the notice above the
  field while a long message is being written, which is where parla puts
  its own. What the renderer emits for a line break is `<br>` and not a
  newline, since in `Text.StyledText` a newline is whitespace and a body
  joined with newlines draws as one running paragraph that is never long
  enough to fold. `qml_message_lines.rs` measures what Qt makes of each
  shape rather than trusting a reading of it.
- **A page is built before its transition starts, so a page is cheap to
  build.** Silica's `push` loads the QML, instantiates the page and only
  then begins the animation, so everything the page imports is paid for
  between the tap and anything moving. The chat list's pulley therefore
  asks for `animatorPush` where Silica has it, which starts the transition
  and builds the page behind it, and falls back to `push` where it does
  not. The other half is the page itself: the new group page asks the
  contact list for the rows it means (`ContactList.picked_rows`) rather
  than building a row per contact and hiding all but the members, which
  would cost the size of an address book that has nothing to do with the
  group.
- **A wait before something is destroyed belongs to the list, not the
  row.** `ListItem.remorseAction` is Silica's shortcut: it makes a
  `RemorseItem` in the row and hands it the action. Deleting out of a list
  destroys rows -- the first delete lands, the core says so, the row goes
  -- and the chat list is churned harder still, since an arriving message
  reorders it, which is a remove and an insert. A wait living somewhere
  that volatile has to be proved every release; a wait beside the list
  does not.

  So the two halves are kept apart. The *action* is a `PendingRemoval`
  beside each list, emptied as whoever holds it is left -- the
  conversation from its page's `Deactivating`, the other three from their
  own -- for the reason ConversationPage writes its draft there: leaving
  is exactly when a timer has not fired yet. Every id waiting carries its
  own deadline and goes on it, with the timer armed for whichever is
  soonest; one countdown shared between them would have to be restarted on
  each new delete, letting the first message's drawn countdown run out and
  the platform put it back. The two clocks -- the drawn one and the
  deleting one -- are tied by `countdownFor(id)`, the only length a row may
  hand `RemorseItem.execute`, and `qml_syntax.rs` counts that every raised
  countdown asked for its length rather than choosing one.

  The *look* is still Silica's own `RemorseItem`, raised by the row over
  what is going and handed a callback that does nothing: the bar, the
  seconds, "Tap to cancel" and the fade, all of it the platform's, because
  a reader already knows what a countdown looks like here. Raising it
  needs two things Silica's shortcut does for you: the row puts the
  countdown up again when it is rebuilt mid-wait
  (`PendingRemoval.remaining`), and nothing else may fade or hide what the
  countdown covers, since `RemorseItem` does that itself with an
  `opacity: 0.0` on the item it was handed. `qml_syntax.rs` holds every
  list to all of it, and the stub `ListItem` has no `remorseAction` for
  anything to reach for. The profiles list keeps its in-place refresh
  (`core.rs`), which is what stops every row flickering when one profile
  goes.
- **A file is opened elsewhere or kept; reading belongs to messages.** A
  picture and a video have pages of their own and everything else is
  handed to the system. What an attachment needs and a tap cannot give is
  a copy, and that is on the row's menu, beside Open.
- **A message of one's own can be reworded, and the phone can forget old
  ones.** Both are the core's. An edit is `send_edit_request`, which
  changes the text at every end and marks the message `isEdited`, and the
  footer says "Edited" as the reference clients' do. The menu offers Edit
  on what the core would take an edit of, which is the rule
  deltachat-android applies -- a message of one's own, not a notice, not a
  call, with text to change, not one the sending core cut -- and only in a
  chat that takes messages and is encrypted, which the conversation model
  asks the core about beside the chat's kind. Editing is a mode of the
  field: it holds the message's text, the bar above says which message,
  the attach tray steps aside, and send means keep the change. The
  reader's unsent draft is put aside for the edit and put back afterwards;
  the reference clients throw it away, which is a loss with no reason
  behind it.

  Forgetting is the core's `delete_device_after`, one setting for every
  profile like the download limit, applied to every chat whatever that
  chat's own disappearing-messages timer says and never to "Saved
  messages". It deletes the moment it is set, so the settings page asks
  the core how many messages that is (`estimate_auto_deletion_count`) and
  puts the number to the reader on a page of its own, with a switch they
  have to turn before accept means anything -- the checkbox
  deltachat-android and deltachat-ios put on the same question. A picture
  or a video has a page of its own with Open and Save on its pull-down, so
  the message menu offers neither for those.
- **A muted group stays quiet, except for a reply to the reader.** The
  chat list decides what is announced and never announces a muted chat;
  the one exception is what the reference clients call a mention, on by
  default as they have it: a message in a muted *group* that quotes one of
  the account's own messages. The core does not say so on the event, and
  the quote names its message and nothing else about it, so the list reads
  the message and the one it quotes (`chatlist.rs`, `is_mention`). That
  answer lands after the refresh the event started has usually left the
  chat unannounced, so a mention starts a refresh of its own with the chat
  marked to pass the mute once, and the announcement then carries the
  row's preview like every other. A muted one-to-one chat is not a group:
  it was muted with the one person in it in mind.
- **What is made on the phone is made by the platform.** A picture or a
  video comes from QML's `Camera`; a voice message from `QAudioRecorder`,
  which QML on Qt 5.6 does not offer and the shim reaches through the
  tree's second `cpp!` block (`BUILDING.md`). Either waits in the app's
  cache directory until the core has copied it, and is sent as any other
  file -- a voice message with the core's `Voice` view type, the one kind
  the core has to be told.
- **What a chat holds besides words is listed by kind, off the core's own
  index.** The contact's and the group's page carry a row of tiles --
  Gallery, Audio, Files, and Apps where apps are on -- and each opens a
  page of that kind (`ChatMediaPage.qml`), which is where the reference
  clients keep theirs. The index is the core's `get_chat_media`: up to
  three view types in one call, and that limit is what shapes the four
  pages -- pictures, GIFs and videos; music and voice messages; files and
  shared contacts; apps. It answers oldest first and says not to re-sort
  it, so the model (`chat_media.rs`) turns the list round and does nothing
  else to its order.

  The rows are messages in the conversation's own shape, read in the
  conversation's own pages of fifty, but from the top down without waiting
  to be asked: every row stands as a placeholder from the moment the ids
  are in, and the first screen is filled before the rest have been read.
  What has been read is kept across a reload, so a picture arriving while
  the page is open is one row fetched and nothing moved. The gallery's
  tiles are the platform thumbnailer's -- what the gallery app scrolls
  through, drawn once to the cell's size and kept -- rather than a decode
  of every picture; the other three pages draw the conversation's own
  attachment rows, so a voice message plays where it sits, an app runs on
  a tap, and a long press offers a file what the chat's row menu offers
  it. The same press offers, on every kind, Show in chat and Delete.
  Deleting is the conversation's own arrangement, so a run of deletes
  survives the rows it destroys. Show in chat walks the page stack down to
  the conversation this page was opened over (`previousPage` until a page
  has `showMessage`), tells it the message, and pops to it; the
  conversation keeps the ask until it is the page on screen and lands the
  message the way a search result lands.
- **The first screen is the cover with nobody on it yet, and it is a
  picture.** A new reader sees what an old one sees when the app is
  minimised: a field of faces in the ambience's colours, grey in its
  primary and a few lit in its highlight, filling the screen either way
  up, with the app's name, one line saying what it is, and the two ways
  on, in a box the field clears for them. Nobody is known yet, so the
  faces are made up -- busts in discs and initials on discs, the two kinds
  of avatar the app draws -- and they are drawn ahead of time into two
  masks in `qml/art/`, one per orientation, rather than laid out on the
  phone: a screenful of the cover's avatars is a hundred masked,
  desaturated, tinted textures, and a first impression cannot afford a
  frame of that, while a picture is one texture and one pass. The masks
  carry no colour: red is a grey face's ink, green a lit one's, and one
  shader (`components/FaceField.qml`) tints them with the theme's own two
  colours, so one file is right on every ambience and the room for the
  words is cut where the words are. The painter is standard-library Python
  and deterministic, so the masks change only when it does, and a build
  needs neither it nor a display.

  The field is the screen's rather than the page's. A Silica page is
  centred in what holds it and turned inside it, so on a phone that keeps
  a band of its screen for the camera the page is handed that band short
  of the screen -- a strip across the top upright, a strip down one edge
  on its side. A field anchored to the page leaves that strip bare, which
  on a screen made of faces is the first thing the eye finds, so the page
  measures what it is in, through its own coordinates so that one sum
  serves both ways up, and lays the field over all of it. The shader crops
  the mask rather than stretching it, so a wider field is more field, not
  wider faces.
- **A phone that has been used opens on its chat list, and the first
  screen is never made.** Which page goes up first is decided before
  anything is drawn, by the window rather than by a page
  (`qml/postivene.qml`): the chat list writes the profile it is on to
  dconf, and the next launch reads that key back -- a file read, done
  while the core is still spawning -- and opens the chat list on it
  directly. The list is on screen before there is anything to put in it,
  which is the right way round: it fills in behind itself the way it
  already does when the core reconnects. Deciding this on the welcome page
  instead would cost the welcome page itself, put up and animated out on
  screen. The welcome page still carries that path, because a phone whose
  key was never written has only the core's answer to go on, and there it
  offers the hand-over until the stack takes it rather than reading one
  refusal as an answer.
- **A question with two answers is asked with tiles, not with a stack of
  buttons.** The onboarding pages each ask one thing -- what do you want
  to do, do you have a profile already, where is it -- and every answer is
  a place to go rather than something to do to what is on screen. That is
  what the row on a contact's page already is, so it is what these are:
  one tile per answer, side by side, an icon over the words and a quieter
  line under them where there is more to say (`components/ChoiceTiles.qml`,
  built the way `MediaKinds.qml` is, bindings rather than a positioner).
  Two buttons of the same size, one above the other, say only that there
  are two of something; an icon says which is which before the words are
  read, and the reader who needs that most is the one on the first screen
  of an app they have never opened.

  The icons are the theme's own, so they wear the phone's ambience:
  `icon-m-about` and `icon-m-person` on the first screen, `icon-m-transfer`
  and `icon-m-add` on the setup screen, `icon-m-device` and `icon-m-backup`
  where the profile is asked after, and the same three on Add profile,
  which asks all of it at once. Each of those names is one other Sailfish
  apps on the same phone draw, and the first screen is a device check
  (docs/HARBOUR.md) partly for that reason: a name this theme does not have
  draws nothing at all, and a tile with no icon on it is the one thing the
  headless tests cannot see (`components/AppMark.qml` says the same about
  webxdc). The row is as tall as the tile that needs most room, so it reads
  as a row rather than as blocks, and a choice the core is not up for yet
  is greyed rather than missing.

  Two tiles share a screen; three do not -- side by side each would have a
  third of a phone, which is not room for a line of words with a second
  line under it -- so Add profile stacks its three instead. Stacked, a tile
  is a row rather than a tile: the icon at the left where a contact's
  picture would be, the words beside it and ranged left, the way every
  other row in the app reads (`components/ContactRow.qml`). The stack
  starts under the header, not in the middle of the page: a list is read
  from the top.
- **A newcomer is told what Delta Chat is before being asked to pick a
  server.** The first screen offers two ways on rather than one, because a
  reader who has never heard of Delta Chat and a reader who came for it
  want different next screens. "Tell me about Delta Chat" is five facts,
  one per screen, swiped through (`pages/IntroPage.qml`): a profile made
  on the device, no directory to be found in, encryption that is simply
  always on, groups without an owner, a relay that only carries messages.
  They follow delta.chat's own FAQ with the technical half left out, each
  over a drawing made the way the faces are (`components/InkArt.qml`), and
  a drag past the last one goes on to the setup path rather than stopping
  -- a drag and nothing else, since turning the phone moves the view too
  and is not a reader asking for anything. "Set up my profile" goes there
  directly: the choice between creating a profile -- the relay dialog,
  which is where a server is picked -- and having one already. Neither of
  those two screens draws the field: it is the welcome, and behind a
  drawing or a question it would be one pattern too many.
- **A profile that exists already is taken over, not made again.** Both
  ways the other Delta Chat apps offer are here, and both are the core's
  import: from a device that still has the profile, which offers it over
  the local network behind a code this phone reads (`get_backup`), and
  from a backup file that device wrote (`import_backup`). The pages are
  `ExistingProfilePage.qml`, which asks which, and `RestoreProfilePage.qml`,
  which does either -- one page, because everything after the first step
  is the same bar and the same answers. A transfer is an attempt in the
  same sense a signup is (`signup.rs`): one at a time, cancellable, and an
  answer arriving after the reader gave up brings a profile nobody asked
  for, so it is removed rather than kept. Two things are this app's own
  rather than the core's: the code is classified before the transfer
  starts, because "not a second device's code" said in protocol terms is
  no use to somebody holding a camera; and IO is started on what arrives,
  because an imported account has none running and a profile that does not
  fetch is not a profile. The same take-over is reachable from the
  profiles list, behind the one plus under it, for the reader whose old
  phone is in their other hand: that plus asks which of the three ways in
  they want (`AddProfilePage.qml`) rather than putting three answers under
  a list of profiles before anybody has been asked anything.

  Adding a profile is the other half of that screen, and the relay is the
  part nobody here controls: a public relay is somebody's spare-time
  server, and one that is down holds the core's transport call for as long
  as its own connection attempts take, which is minutes. So an attempt is
  bounded (`signup.rs`): at thirty seconds the shim stops the process and
  tells the page the relay did not answer, and from the fourth second the
  page says under Cancel what a relay is and that another is worth trying.
  An attempt given up on is still running in the core, which allows one
  ongoing process per account and refuses a retry that picks the same
  unconfigured account back up; so the accounts an attempt still holds are
  remembered, a retry takes a fresh one, and a profile the first relay
  makes after all is removed rather than found on the next start.
- **A backup is one profile's, because the core's export is.**
  `export_backup` takes an account id and writes that account -- its
  messages, its contacts, its key -- into the one `.tar` that
  `import_backup` reads back. So the way to it is the profile's own row on
  the profiles page and not the settings that belong to no profile: a
  phone with three profiles makes three backups, and the page draws the
  profile at the top the way the invite code's page does. The invite code
  is on that row too, for the same reason: both are about one profile and
  neither is a setting. Once a backup has been written the page is done --
  the button goes, and the chats are attached to the right, so what is
  left is a swipe rather than an offer to write the same profile out
  again. The file goes to Documents under the name the core chooses; a
  reader who has to type a path on a phone is a reader who does not make a
  backup, and where it landed is said in full afterwards, because the next
  thing to do with it is to copy it off the phone.

  Two things the core leaves to the caller are in `backup.rs`. The core
  answers the export with nothing, so the file is found by what appeared
  in the folder: the event carrying its name (`ImexFileWritten`) is polled
  on a call of its own, so one emitted before the export answers can
  arrive after it, and a page waiting for it would sometimes wait for
  ever. And the export reports itself in the same `ImexProgress` events an
  import does, with nothing in them to say which -- so the page feeds the
  core's events in and the object takes them only while it is the one that
  asked.

## Platform baseline

- Toolchain floor **Rust 1.75.0, Qt 5.6.3** — what Sailfish ships.
- Built against the **5.2** SDK, the Jolla Phone's baseline. Anything older
  is out of scope: a binary from a newer SDK can call symbols an older
  phone lacks, and that is accepted rather than worked around. Harbour
  requires it too -- it rejects a binary that does not link
  `__libc_start_main@GLIBC_2.34`, which only a 5.x glibc provides.
- `aarch64` and `armv7hl` for devices; `i486`/`x86_64` for the emulator.
- Account storage is the core's own, pinned inside the sailjail grant at
  `$XDG_DATA_HOME/postivene/postivene/accounts` (`POSTIVENE_ACCOUNTS_DIR`
  overrides).

## What is missing

In order of what matters:

1. **Harbour-readiness.** Every rule a source tree can answer is now a
   mandatory CI gate (`ci/harbour-check.sh`, `HARBOUR.md`), and the real
   validator runs against each built RPM. One blocker remains, and it is not
   fixable here: the bundled `deltachat-rpc-server` is a second ELF
   executable, which Harbour permits nowhere.
2. **Add-as-second-device**, and restore-from-backup.
3. **Message polish**: avatars on bubbles, and a way to react with an
   emoji the quick row does not offer.
4. **The rest of the webxdc API.** Apps are sent, shown and run
   (`webxdc.rs`, `WebxdcPage.qml`), and status updates go both ways. What
   is not offered is the newer calls -- `importFiles`, realtime channels
   -- which are absent rather than present and failing, so an app that
   feature-tests for one takes its own other path. Nor is
   an app's `source_code_url` shown anywhere: the page has no pulley to
   put it in (a WebView cannot sit in the flickable one needs), and a tap
   on the app's own name that opens a URL its sender chose is a worse
   answer than none.
5. **The store page loads itself.** The app a reader takes from the store
   is fetched by the core, but the store's own page is loaded by the
   engine straight off the web -- so that one page does not follow
   whatever the core has been told to reach the network through, and the
   site sees the device rather than the core. deltachat-android proxies
   every request through `get_http_response`; doing the same here means
   serving the site from the shim's own loopback host and rewriting the
   links in it, which is a page-shaped guess this repository cannot test
   against.
