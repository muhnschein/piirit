# On-device AI

This is a feasibility study, not a design that has been built. The
question is whether Piirit can transcribe voice messages, tag
pictures and videos, sort messages into categories and translate them on
the phone, at a cost in effort and battery this project would accept.

**Transcription: yes. Tagging: yes, if it means a small fixed set of
categories rather than free-form labels.** Neither needs anything Harbour
forbids, and neither sends anything off the phone.

**Categorization: yes, and most of it needs no model. Translation:
likely, but its runtime is the least proven part of this document.**
Both are sketched in less depth than the first two; nothing about them
has been measured yet.

A number marked *measured* was taken for this document, on a build host.
A number marked *estimated* is arithmetic from model sizes and published
figures, and it has to be checked on a phone before anything ships (see
"What to measure first").

## The answer in one table

| | Transcribing voice messages | Tagging pictures and videos |
|---|---|---|
| Runtime | whisper.cpp (ggml), linked statically | the same ggml, or tract |
| Model | Whisper base (57 MB) or small (about 190 MB), downloaded on request | an image encoder of 5–90 MB, downloaded on request |
| Energy for one use | *est.* 4–8 J for a 30 s message on base | *est.* 0.05–0.5 J for one picture |
| Battery on a typical day | *est.* well under 1 % | *est.* under 1 % for new pictures; the backlog runs only while charging |
| Effort | about two weeks | about one to one and a half weeks more |

## What the phone offers

From the published specifications, the Jolla Phone has a MediaTek Dimensity 7100:

- **CPU:** 4 × Cortex-A78 at 2.4 GHz and 4 × Cortex-A55 at 2.0 GHz. Both are Armv8.2-A with fp16 arithmetic and dot-product instructions. Neither has i8mm or SVE.
- **GPU:** Mali-G610 MC2.
- **Memory:** 8 GB of RAM.
- **Battery:** about 5,500 mAh, which is about 76.6 kJ at 3.87 V.

**All inference runs on the CPU.** The other units are out of reach:

- **The NPU:** the SoC's APU is reachable through NeuroPilot and NNAPI, and both are Android frameworks.
- **The GPU:** compute needs `libvulkan` or `libOpenCL`. Neither is on Harbour's library list (`ci/harbour/allowed_libraries.conf`), which offers EGL and GLES only. Loading one with `dlopen` would be the kind of circumvention `docs/HARBOUR.md` already rules out, and ggml has no GLES backend anyway.

That constraint is less limiting than it sounds. Four A78s peak at about 300 GFLOP/s in fp16, and the models worth running here need between 1 and 400 GFLOP per use.

## The rules it has to fit

**Harbour allows one ELF file.** A C or C++ runtime has two options:

- Link it statically into `/usr/bin/harbour-piirit`, which is how LAME is already carried (`mp3lame-sys`).
- Ship it as a `.so` under `/usr/share/harbour-piirit/lib/`.

Static linking is simpler. *Measured:* whisper.cpp with ggml's CPU backend, built static with `GGML_OPENMP=OFF`, links only `libstdc++`, `libm`, `libgcc_s` and `libc`. All four are on the allowed list.

**Rust 1.75 rules out most of Rust's ML crates:**

- candle
- burn
- rten
- `ort` from 2.0.0-rc.10
- `whisper-rs` from 0.16 (needs Rust 1.88)
- tract from 0.22

Two crates remain usable. *Measured:* tract 0.21.18, which is the line tract still maintains for Rust 1.75, and symphonia 0.5.5 both compile under `cargo +1.75.0`. The lockfile has to be resolved for 1.75 (`CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback`), as `docs/BUILDING.md` already requires. C and C++ code is not affected by the floor.

**GCC must be version 9 or newer.** ggml wants C11 and C++17, and it uses `std::filesystem`, so GCC 8 would also need `-lstdc++fs`. Sailfish reportedly moved to GCC 10.3 in 4.6, and the 5.x SDK is newer still. Check with `sb2 gcc --version` before the first build.

**Calling whisper's C API is FFI, so it needs `unsafe` code.** It would be a third `cpp!` file beside `translations.rs` and `recorder.rs`, under the same rule: the narrowest scope, and every block short enough to check by reading. For this use, `whisper.h` comes down to a dozen functions.

**Nothing leaves the phone.** The only network access is fetching a model once, from a URL and checksum pinned in the source, the way `scripts/fetch-rpc-server.sh` pins the core. No cloud API is used, ever: a messenger built on end-to-end encryption cannot send voice messages to a transcription service.

**Every new string has to be translated into 39 languages.** Tag names are strings too, which is one reason the tag set below is small and fixed.

## Energy

**The budget:** the battery holds about 76.6 kJ, so 1 % is about 770 J.

**The cost:** four A78s at full load draw roughly 3–4 W, plus 0.5–1 W for memory. Taking 4 W:

- one second of inference costs about 4 J
- 1 % of the battery is about three minutes of all four big cores at full load

Two facts decide how much of that budget a feature uses.

1. **Whisper's encoder always processes 30 seconds of audio.** A 5-second voice message costs as much as a 30-second one unless `audio_ctx` is set to the audio's real length. whisper.cpp exposes that setting, at a small cost in accuracy. Most voice messages are short, so this is the biggest single saving.
2. **When a job runs matters more than how fast it runs.** Work the user asked for runs now. Work nobody is waiting for waits until the phone is charging.

*Estimated* cost per 30-second window, with greedy decoding of about 100 tokens. The GFLOP figures are worked out from each model's dimensions. The time assumes an effective 60–120 GFLOP/s on four A78s, and the energy assumes 4 W.

| Model | Size (q5_1) | GFLOP per window | Time | Energy | Windows per 1 % of battery |
|---|---|---|---|---|---|
| tiny | 32 MB | ~45 | 0.4–0.8 s | 2–3 J | ~300 |
| base | 57 MB | ~110 | 1–2 s | 4–8 J | 100–200 |
| small | ~190 MB | ~420 | 3.5–7 s | 14–28 J | 30–55 |

Take a heavy user who receives 30 voice messages a day and transcribes them with `small`, with `audio_ctx` trimmed. Untrimmed that is at most 840 J, about 1.1 % of the battery; trimmed to messages of 15 seconds on average, it is about half that. The screen, which is on while they read, costs more.

**Tagging costs less per item. The backlog is what costs.**

| Model | Compute per picture | Time per picture | Energy per picture | 10,000-picture backlog |
|---|---|---|---|---|
| A 0.8-GFLOP CNN (EfficientNet-Lite0) | 0.8 GFLOP | 10–20 ms | ~0.05 J | ~0.5 kJ |
| A CLIP ViT-B/32 image encoder | ~9 GFLOP | 0.1–0.2 s | ~0.5 J | ~5 kJ (6–7 % of the battery) |

New pictures as they arrive cost almost nothing with either model. The backlog of pictures already in a profile is work for while the phone is charging.

The policy that follows:

- **Transcription runs on tap by default.** A setting can make it run automatically, once for each incoming voice message.
- **Tagging, when switched on, handles new pictures as they arrive.** It works through the backlog only while the phone is charging, one item at a time, and stops when the charger is removed.
    - The charger state is on MCE's D-Bus interface, which `Nemo.DBus` (an allowed import) can read.
    - Whether sailjail lets the app see it has to be checked on a phone.
- **One job runs at a time**, on its own thread. It never runs on the Qt thread or on the tokio runtime that the JSON-RPC client waits on.
- **Jobs start with four threads**, the size of the big cluster. Whether running fast on the big cores and then going idle beats running slowly on the little ones is one of the things to measure.
- **A model is unloaded after a minute of inactivity.** Keeping it in memory costs RAM rather than energy.

## Transcribing voice messages

### The runtime: whisper.cpp

ggml's CPU backend is the best-tuned ARM inference code that builds here. It has NEON, fp16 and dot-product kernels for quantized weights, and it needs no OpenMP.

*Measured* on a 4-core x86 host:

- `whisper-cli` and its libraries build from scratch in 1 min 45 s with one job.
- The result strips to 2.8 MB.

Under scratchbox2 with `jobs 1`, a device build would take about two minutes longer. Build for the one phone:

    -DGGML_NATIVE=OFF -DGGML_CPU_ARM_ARCH=armv8.2-a+dotprod+fp16

It would be carried the way qmetaobject is:

- A pinned upstream release.
- Only the files the CPU backend needs. ggml ships backends for CUDA, Metal, Vulkan and a dozen more that this build never reads.
- Fetched by a script that verifies a checksum.
- Compiled from `build.rs` through the `cmake` crate (MSRV 1.65), with `cmake` added to the spec's `BuildRequires`.

**Not `whisper-rs`.** Its current release needs Rust 1.88. The last release that might build bundles an older whisper.cpp and needs bindgen compiled. It would save about a dozen calls, which is not worth a crate held to an old version by the Rust floor.

### The model: multilingual Whisper

Nothing lighter covers the languages Piirit is translated into:

- Vosk and Moonshine have no Finnish model.
- Parakeet v3 has Finnish, but it is 640 MB.

Whisper's quality drops steeply with model size in the less widely spoken languages; even `medium` has about 16 % word error rate on Finnish FLEURS. So:

- **`base` (57 MB) is the default offer.** It is good in English, German, Spanish and French, and rough in Finnish.
- **`small` (about 190 MB) is the choice for languages where base does badly.** It could be the default where the phone's language is Finnish, Estonian or Hungarian.
- **Nothing larger.** `medium` is 1.5 GB and costs about four times the energy of `small`.

**Language detection costs nothing extra.** Whisper detects the language from the same encoder pass. The phone's language could be passed as a hint.

### Getting samples out of a message

Whisper wants 16 kHz mono f32 samples. What arrives is:

- AAC in MP4 from Android and iOS
- MP3 from the desktop client and from Piirit itself
- occasionally Opus

*Measured:* symphonia 0.5.5 decodes AAC-LC in MP4, MP3, Vorbis and WAV in pure Rust under Rust 1.75. It adds about 1 MB to the binary, and its licence (MPL-2.0) is already allowed. It has no Opus decoder, which leaves two options:

- Use Qt's `QAudioDecoder`, which goes through the same GStreamer the player uses. That is another `cpp!` block, and whether `QAudioDecoder` has a backend on the phone still has to be checked.
- Leave Opus messages without a Transcribe offer at first.

Resampling to 16 kHz needs only a small windowed-sinc filter, not a crate.

### Where a transcript is stored

**Not in the core.** A received message cannot be edited, and the core has no field for the reader's own notes on a message.

**In a file per profile** in the app's data directory, keyed by message id. A transcript is deleted:

- when the core reports `MsgDeleted` for its message
- with its profile
- by "Delete messages from device"

**Transcripts are not in a backup.** After a restore they are made again, which costs energy the user chooses to spend.

### What the user sees

- A Transcribe action in the voice-message bubble.
- A spinner while it runs.
- The text under the player once it is done, selectable and copyable.
- Optionally, search finds transcripts too, merged into the message section of `search.rs`.

## Tagging pictures and videos

### What a tag means here

**Tags are not free-form labels.** Every label shown on screen is a translated string. A model's thousand ImageNet classes include over a hundred dog breeds and no "person", so they are neither useful nor translatable.

**Tags are a fixed set of about 30 categories** that the gallery can filter by and search can find: people, groups, selfies, pets, food, documents, screenshots, receipts, text, nature, buildings, vehicles, drawings, memes and so on. That is 30 strings to translate into 39 languages, once.

### The model

| | ImageNet CNN mapped to categories | CLIP-style zero-shot |
|---|---|---|
| Example | EfficientNet-Lite0, MobileNetV3-Large (Apache-2.0) | OpenAI CLIP ViT-B/32 (MIT), or a smaller open CLIP |
| Weights | 5 MB | about 90 MB for the image encoder in int8 |
| Energy per picture | about 0.05 J | about 0.5 J |
| Categories | Mapped from the 1000 ImageNet classes, which cover no people, screenshots or documents. | Any text. Each category's embedding is computed once, offline, and shipped as a table, so no text encoder runs on the phone. |
| Quality for chat pictures | Poor exactly where chat pictures are. | Good. |

**The zero-shot route answers the question people actually ask of a chat's pictures.**

**Apple's MobileCLIP would be the ideal size, but its weights are licensed for research only.** A GPL app cannot ship them.

**Smaller CLIPs with permissive licences exist,** such as TinyCLIP. They are worth measuring against ViT-B/32 before choosing, and each candidate's licence needs checking.

### The runtime

| | tract 0.21.18 | The ggml brought in for Whisper |
|---|---|---|
| What it is | Pure Rust, no `unsafe`, loads ONNX or NNEF | The image encoder written as a ggml graph |
| Binary growth (*measured* on x86_64, stripped, thin LTO) | +9.7 MB raw and +2.1 MB xz-compressed with only the NNEF loader; +15.6 MB raw with ONNX. The whole app is 5.8 MB today. | None |
| Build time | 4–5 minutes on a 4-core host (*measured*), and more at one job under scratchbox2 | little |
| Work | Lowest effort | A ViT is a dozen ops that ggml already has, and llama.cpp's `clip.cpp` is an existing reference. About 300 lines of C++ in the `cpp!` file. |

**Recommended:** ggml, if transcription lands first. It keeps one inference stack and adds nothing to the binary.

### Videos

**Tag the frame the gallery already has.** The gallery asks the platform thumbnailer (`Nemo.Thumbnailer`) for a frame of each video. Decoding more frames would cost far more than the tagging.

**A video's sound can go through the transcription path.** It is the same AAC in MP4 as a voice message.

### Pictures arrive pre-shrunk

The thumbnailer caches squares for `ChatMediaPage`, and they are exactly the 224-pixel input these models want. Reading them avoids decoding a full JPEG.

### Where tags are stored

Each profile gets one small index, mapping message id to category bits, kept in memory. It is used in two places:

- `ChatMediaPage` gets a row of category filters.
- A search query that matches a category's name adds that category's pictures to the results.

## Sorting messages into categories

### Two kinds of category

**"Family" and "friends" belong to a contact, not to a message.** A
model reading each message would guess at something the user knows and
can say once. So:

- A contact carries a label the user picks: family, friends, work, or
  none. A chat with that contact, or a group the user labels, inherits it.
- The core has no field for this, so labels live in a file per profile in
  the app's data directory, like transcripts.
- The chat list gets a filter by label, the same way #124 asks for an
  unread filter.

**"Important" belongs to a message.** It is the only category a model
could help with, and even there rules come first.

### Rules first

These cost nothing and need no download:

- the message mentions me
- the message quotes one of mine
- a question to me in a one-to-one chat
- the sender is verified, or the chat is pinned
- the chat is labelled family

That covers most of what people mean by important. A model is an
optional layer on top, for the rest.

### The optional model

**A small multilingual sentence encoder**, run on the same ggml as
Whisper. Each incoming message becomes a vector, and "important" is
judged two ways:

- **Zero-shot to start:** the distance to a handful of prototype
  sentences per category, embedded offline and shipped as a table, the
  way tagging ships its category embeddings.
- **Learned from the user after that:** marking a message important or
  not trains a logistic regression on its vector, on the phone. That is
  a few kilobytes of weights per profile, and it never leaves the phone.

**Candidates**, both permissively licensed:

| | multilingual-e5-small (MIT) | paraphrase-multilingual-MiniLM-L12-v2 (Apache-2.0) |
|---|---|---|
| Parameters | about 118 M | about 118 M |
| Download, int8 | *est.* about 120 MB | *est.* about 120 MB |
| Compute for a 64-token message | *est.* about 3 GFLOP | *est.* about 3 GFLOP |
| Energy per message | *est.* 0.1–0.2 J | *est.* 0.1–0.2 J |

**Most of that size is the vocabulary table, not the network.** About
96 M of the 118 M parameters are a 250,000-token embedding table, which
costs a lookup, not compute. Two ways to shrink it, both to be measured:

- Trim the vocabulary to the tokens the 39 shipped languages use.
- Use a static-embedding model distilled from one of these (Model2Vec).
  It skips the transformer entirely and runs in microseconds. Its size and
  its quality on chat messages are not known yet.

**The encoder is a BERT graph.** llama.cpp has one that can serve as the
reference, as `clip.cpp` does for tagging.

**Where the result is stored:** one bit per message in the per-profile
index that tagging already keeps.

## Translating messages

### The runtime: Bergamot

**Bergamot is what Firefox uses for its offline translation.** It is
Marian, a C++ translation engine, with small int8 student models
trained by Mozilla for each language pair. It would be linked statically
behind a `cpp!` wrapper, the way whisper.cpp is.

**This is the riskiest runtime in this document:**

- Marian's fast int8 matrix code (intgemm) targets x86. Firefox runs
  Bergamot on ARM through WebAssembly with other kernels. Whether a
  native aarch64 build exists that fits the Rust 1.75 and GCC floors has
  not been checked.
- Marian brings more dependencies than whisper.cpp does. Each has to be
  on Harbour's library list or linked in.
- The model licence has to be checked per model before shipping.

**If Bergamot does not build, OPUS-MT through CTranslate2 is the
fallback.** Its models are larger, around 75 MB a pair (*est.*).

### The models

**Pivot through English.** A message in Finnish shown to a German reader
goes Finnish → English → German. That needs two models for each
language, not one for every pair: twenty for ten languages instead of
ninety.

**Each model is about 15–40 MB** (*est.*, Bergamot "tiny" int8).

**Download only what is used:**

- First the pair for the phone's language, offered when the user first
  taps Translate.
- When a message arrives in a language no model covers yet, Translate
  offers that model's download instead of a translation.

**Which languages Mozilla covers has to be checked,** Finnish first.
Every language Piirit is translated into and Mozilla does not cover gets
no Translate offer.

### Detecting the language

**fastText's `lid.176.ftz` is under 1 MB,** small enough to ship in the
package. Its licence (CC BY-SA 3.0) covers data, and has to be checked
against a GPL app before shipping. Whisper's own detection covers voice
messages.

### What the user sees

- Translate in the message's context menu, shown only when the message
  is not in the phone's language.
- The translation under the original, marked as a translation, with a
  way back to the original.
- Translations are stored like transcripts: per profile, keyed by message
  id, deleted with their message.

## What it adds to the package

Figures are *measured* on x86_64, stripped, with thin LTO, less an empty Rust binary.

| | Binary | Compressed (xz) | Model, downloaded on request |
|---|---|---|---|
| whisper.cpp and ggml | at most +2.8 MB (all of `whisper-cli`) | at most +0.9 MB | 57–190 MB |
| symphonia | +1.0 MB | +0.4 MB | – |
| Tagging, ggml graph | about +0.2 MB (*est.*) | – | 5–90 MB |
| Tagging, tract NNEF (instead of the ggml graph) | +9.7 MB | +2.1 MB | 5–90 MB |
| Categorization, BERT graph on ggml | small (*est.*) | – | about 120 MB (*est.*), less if trimmed |
| Translation, Bergamot | not measured | – | 15–40 MB per language (*est.*) |
| Language detection, `lid.176.ftz` | – | – | under 1 MB, packaged |

**Models are downloaded on request, not packaged.** The RPM is 11.7 MB today, and most users will never switch these features on.

**Harbour's rule against downloading code does not apply.** A model is data, not code.

**Models are hosted as assets of a Piirit GitHub release** and pinned by SHA-256 in the source. They are not fetched from Hugging Face at run time, which would tell a third party who uses the feature.

## What to measure first

Every estimate above can be checked in an afternoon with a phone. Do this before building any UI:

1. **Time the models.** Build `whisper-bench` and `whisper-cli` with the SDK and the flags above, and run them on a Jolla Phone. Try tiny, base and small with four threads, on a 10 s clip and a 30 s clip, with and without `audio_ctx`.
2. **Measure the energy.** Read the battery drain while they run (`/sys/class/power_supply/battery/current_now` and `voltage_now`), and compare it with the phone idling with the screen on. The difference is the energy the tables above estimate.
3. **Choose the default model.** Transcribe real voice messages in English, German and Finnish with base and small, and choose by reading the results.
4. **Check the platform:**
    - `sb2 gcc --version`
    - whether `Nemo.DBus` can read MCE's charger state from inside sailjail
    - whether `QAudioDecoder` has a backend on the phone
5. **Build Bergamot for aarch64.** Try a native build of
   bergamot-translator with the SDK, and translate a paragraph with a
   tiny model on the phone. If it does not build, try CTranslate2.
6. **Check language coverage.** List which of the 39 shipped languages
   Mozilla's models cover.

## Plan

1. **The measurements above: about a day.** They could end the project if the estimates are an order of magnitude off.
2. **Whisper in the build: about two days**, mostly waiting on scratchbox2. That covers vendoring, `build.rs`, the spec and `vendor-check`, with the Harbour check and the `msrv` job still green.
3. **Transcription end to end: about a week.** That covers decoding, resampling, the `cpp!` wrapper, the model download page, the bubble, storage, settings and 39 catalogs. The tests run on the host with the tiny model, the way the `real_core` tests run against a real server.
4. **Tagging: one to one and a half weeks.**
    - Choose the image model by measuring two or three candidates on real chat pictures.
    - Write the ggml graph, the category table and the index.
    - Add the gallery filter and the backlog that runs only while charging.
5. **Categorization rules and contact labels: a few days.** No model. The
   labels file, the rules, the chat-list filter and the strings.
6. **The categorization model: about a week**, after transcription has
   brought ggml in. The BERT graph, the prototype table and the
   on-phone training from the user's marks.
7. **Translation: unknown until step 5 of the measurements.** If
   Bergamot builds, about as long as transcription end to end.

## What was ruled out

- **The NPU and the GPU:** a Harbour app cannot reach them (see "What the phone offers").
- **ONNX Runtime:** version 1.30 for aarch64 is a 25 MB `.so`. Harbour allows it under `/usr/share/harbour-piirit/lib/`, but it would double the package. The `ort` crate's last release that builds on Rust 1.75 is a 2.0 release candidate.
- **Vosk and Moonshine:** no Finnish model.
- **Parakeet v3:** 640 MB.
- **RAM++ (Recognize Anything):** 3 GB.
- **MobileCLIP:** licensed for research only.
- **Any cloud API:** voice messages would leave the phone (see "The rules it has to fit").
