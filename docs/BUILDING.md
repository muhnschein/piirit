# Building, testing, and packaging

Standard: [clove](https://github.com/muhnschein/clove) §9: pinned toolchain, pedantic lints, CI-parity `make` targets, tests driving real binaries.

## Toolchains

- `rust-toolchain.toml` pins 1.94.1 (reproducible lints). Device floor **1.75.0** (CI `msrv`, warnings denied). rustup only; SDK cargo ignores pin.
- FFmpeg, x264 = submodules in `third_party/`, built by `rust/piirit-video` build.rs; fresh clone needs them first:

```sh
git submodule update --init --depth 1
```

- `rust/Cargo.lock` stays **v3** (v4 since cargo 1.78; SDK cargo 1.75 can't read). Modern `cargo update` rewrites silently; `ci/check-lockfile.sh` catches.

## Lints

- Workspace-level: bare `cargo clippy` fails like CI. Deny: `clippy::all`, `pedantic`, `unwrap_used`/`expect_used` (non-test), `missing_docs`, `unsafe_code`.
- `unsafe_code` deny, not forbid: Qt harness tests need `env::set_var` before Qt init; no safe binding (qmetaobject wraps none) for `QTranslator` install, voice recording (`QAudioRecorder`), QML pictures (`QQuickImageProvider`), call mic watch (libpulse).
- Hence four `cpp!` files: `piirit-app/src/translations.rs`, `piirit-shim/src/recorder.rs`, `piirit-shim/src/pictures.rs`, `piirit-shim/src/mic_watch.rs`; each crate's `build.rs` C++ step exists only for them. Each exception narrowest scope, reason stated, short enough to verify by reading.
- Recorder: host `qtmultimedia5-dev` (`Makefile` lists packages), spec `pkgconfig(Qt5Multimedia)`, links `libQt5Multimedia`. Mic watch: host `libpulse-dev`, spec `pkgconfig(libpulse)`, links `libpulse.so.0`. Both Harbour-allowed. Mic-watch test runs own sound server: needs `pulseaudio`, `pulseaudio-utils`; skipped if absent, except CI fails.
- `rust/clippy.toml` bans (both caused device-only failures):
  - `tokio::runtime::Runtime::new`: use `CoreRuntime`
  - `qmetaobject::single_shot`: truncates sub-second `Duration`s

## Testing

Piirit parses almost nothing (protocol, crypto = core's; subprocess ours). Failure mode: misread core JSON or wrong call, unnoticed till phone. Tests aim there.

- `make check`: everything from clean checkout, no phone/account/SDK. Not quite offline: `msrv` fetches 1.75 first time; `integration` fetches pinned `deltachat-rpc-server` first time; `deny` wants advisory DB.
- **Runner `cargo nextest`**, process per test. 100+ test binaries, most start Qt, wait on real timers: `cargo test` ~10 min serialised, mostly idle; nextest 207 tests ~2.5 min on 4 cores. Parallel-safe: webxdc, call hosts bind port 0; QML probes copy tree into pid-named dir.

```
cargo install --locked cargo-nextest
```

- Without it `make test` still runs all, slowly (`make check` must work on bare laptop).
- `rust/.config/nextest.toml`: 30 s slow warn, 2 min kill (stuck Qt test named, not job timeout). No retries: second-attempt pass = defect.
- nextest skips doctests: `cargo test --doc` beside it (`Makefile`, `ci.yml`); `ci/packaging-lint.sh` fails tree with one but not other. None today; first would else never run.

1. **Transport unit**: fake stdio server.
2. **Protocol-contract**: recording double journals requests; pins call sequence per onboarding action.
3. **Qt event-loop**: `QT_QPA_PLATFORM=offscreen`.
   - Steps scheduled by `common::Plan` (`rust/piirit-shim/tests/common/mod.rs`): each a set wait after one before *ended*; `until` polls a condition. Fixed times from start let an overrunning step pull next one in right behind it, before async answer lands (flaked on loaded runners).
   - Bare `single_shot` denied in tests (clippy ban below, no file-wide allow); only `Plan` and two self-rescheduling waits allow it.
4. **QML load**: stub Silica (`tests/silica-stubs/`), real pages, driven by `objectName`. Stubs have no layout: nothing proves page *looks* right.
   - Silica `EnterKey` attached prop unstubbable (QML forbids capitalised property names; `qmetaobject` can't register attached types): pages using it can't load. Put shown content in loadable component.
   - Lay that out with bindings, not `Column`: positioner sizes in polish pass, never runs windowless; geometry reads zero.
5. **Static QML** (`tests/qml_syntax.rs`): Qt 5.6 rules Qt 5.15 accepts silently, plus tree rules: every peer-chosen string drawn as plain text (Silica headers can't, hence `ConversationHeader`); file URLs encoded per segment; only picker pages import `Sailfish.Pickers`; every delegate `model.<role>` exists in its model.
6. **Real-core integration** (`real_server`, `real_core`): gated on `DELTACHAT_RPC_SERVER`, offline. `make test` skips them; `make integration` (in `check`, CI `integration`) sets it. `real_core.rs` tells undecodable request from undeliverable.
7. **Packaging** (`ci/packaging-lint.sh`): spec parses, desktop entry validates, shell scripts clean, catalogs current, `lrelease`-clean, every `docs/*.md` a comment names exists. Missing tool = SKIP locally; CI `PACKAGING_LINT_STRICT=1` fails (as `HARBOUR_CHECK_STRICT=1`).
8. **Report-script** (`ci/sonar-report-selftest.sh`): only script talking outside GitHub; run vs stub of SonarQube's four endpoints (no analysis at test time; CI can't reach `sonarcloud.io`, why script exists).
9. **Runner-setup** (`ci/apt-install-selftest.sh`): apt-source keep rule proved on test's own dir. Inverted = deletes install archive, fails everything. Mirror fallback proved against stand-in apt.
10. **Change-filter** (`ci/changes-selftest.sh`): which PR skips Rust jobs. Most cases guard costly side: code read as docs = green without tests.

Aspiration, tracked not gated: test volume > source volume.

## Static analysis

SonarQube Cloud reads every PR (`.github/workflows/build.yml`, `sonar-project.properties`). **Report, not gate**: `ci.yml` decides. Separate workflow: green `make check` on laptop = green CI, no hosted service.

- Coverage **imported**: `make sonar-reports` writes `rust/target/sonar/lcov.info` (`cargo llvm-cov`, whole workspace), run before scan; without, confident 0.0%. Needs `cargo-llvm-cov`: opt-in, not in `make check`. Uses nextest if installed (else llvm-cov's `cargo test`, ~10 min instrumented).

```
rustup component add llvm-tools-preview
cargo install --locked cargo-llvm-cov cargo-nextest
make sonar-reports
```

- Clippy **not** handed over; Sonar's pass off (`sonar.rust.clippy.enabled=false`): Sonar runs cargo at root, workspace under `rust/` (other clippy or none); report would be empty (`make lint` denies warnings; rest in excluded `third_party/qmetaobject`) yet cost `cargo clean` + full rebuild.
- `sonar.tests`: fixtures apart, so coverage, duplication measure shipped code (fixtures would dominate).
- `sonar.exclusions`: vendored crates, patched qmetaobject, rendered icons, `translations/` (39 `.ts` catalogs read as TypeScript).
- `sonar.coverage.exclusions`: `qml/` (no report possible; one QML JS line = 0% new-code coverage, fails gate).
- `sonar.issue.ignore.multicriteria`: decided-against findings, one rule per path, reason beside: qmetaobject glob import, cognitive complexity in tests/fake servers, char-class name as repeated literal.
- Scanner uploads, exits; server processes later. `scripts/sonar-report.sh` asks server from same runner, prints quality gate, measures, open issues to log + step summary. Step `continue-on-error`: outage = warning.

## CI

`ci.yml` = gate, runs what `make check` runs.

- **Packages via `ci/apt-install.sh`**, not bare `apt-get`: `apt-get update` fails if *any* repo fails; image ships unused ones. Script drops third-party lists, keeps Ubuntu's wherever they are (list kept only if it names `ubuntu.com` host), so archive it installs from survives.
- Runner's mirror (`azure.archive.ubuntu.com`) slow or down: apt retries each file 3 times, drops one stalled 30 s; update or install still failing = lists switched to `archive.ubuntu.com`, tried once more. Selftest drives a stand-in apt that fails.
- **Docs-only PR skips Rust jobs** (`test`, `msrv`, `integration`, `harbour`): `changes` job runs `ci/changes.sh` on merge commit's diff vs base. Only `*.md` outside `vendor/`, `third_party/` (tests read those). Skipped job passes required check. Push to `main`, dispatch: everything. `packaging`, `deny`, `zizmor` always; Sonar (`build.yml`) too.
- **Rust jobs cache `target/`** (`Swatinem/rust-cache`, `rust` workspace). `msrv` own key (builds `+1.75.0`; action keys on default toolchain; shared slot = both miss). `CARGO_INCREMENTAL: 0`: incremental state never reused.
- Cache helps less than it looks: warm clippy ~20 s; `test` cost = suite waiting on timers, not compile.
- **Test job installs `cargo-nextest`**, `ci` profile: `fail-fast` off (report all), failures inline and repeated at end (summary is what's read in 200-line log).

## Translations

- Strings = `qsTr()` in `qml/`. `translations/piirit.ts` = untranslated source; `translations/piirit-<lang>.ts` per language Sailfish ships.
- `scripts/update-translations.sh`: one `lupdate` regenerates all; new string `unfinished` everywhere. `ci/packaging-lint.sh` fails on drift from that run; `tests/translation_catalogs.rs` on any untranslated string.
- App loads `piirit-<lang>.qm` from `scripts/release-translations.sh` (`lrelease`): RPM `%build`; locally `make translations` (beside `.ts`, where source-tree run finds them). Tools: Debian `qttools5-dev-tools`, SDK `qt5-qttools-linguist`; also test dep (test compiles German catalog).
- `<lang>` matched by `QTranslator` vs locale, most specific first: `de` all German; `pt_BR` Brazil only; none = English catalog (plural forms).
- New language: three-line header (in `update-translations.sh`) to `translations/piirit-<lang>.ts`, run script; `lupdate` fills plural forms.

## Dependencies

Few, each justified; `cargo tree` = list, table = why, so drop proposal starts from cost.

| Crate | Use | Why it stays |
|---|---|---|
| `tokio` | server subprocess, pipes, event loop | async transport; `process` reaps child |
| `serde`, `serde_json` | wire JSON | core contract = JSON-RPC |
| `qmetaobject` (vendored), `qttypes`, `cpp`, `cpp_build` | Qt from Rust | whole UI; `default-features = false` drops `log` bridge |
| `chrono` | viewer timezone, day headings | not in `std`; `localtime_r` needs denied `unsafe` |
| `qrcode` | invite as code | one crate, no deps |
| `rqrr` (+ `g2p`, `lru`) | code from camera | QR decoder too big to vendor |
| `mp3lame-encoder` (+ `mp3lame-sys`, `autotools`) | voice message MP3 (`voice.rs`) | phone recorder has no encoder iOS plays as voice; LAME = desktop client's; pure-Rust ones need Rust > 1.75 or are shine ports (no psychoacoustic model) |

- `mp3lame-sys` carries LAME 3.100 C source, builds via its `configure` + `make` (C++ build host has both). Only non-Qt C dep; `rust/deny.toml` allows LGPL-3.0 for its two crates only (LGPL code conveyable in GPLv3 work).

### Making a video smaller

- Picked video recoded before send when ≥ 4/5 of core's recommended message size or outgoing media quality set worse (`rust/piirit-shim/src/video.rs`, `plan` in `rust/piirit-video`, #111). Phone recorder writes several times that; core sends as-is. Nothing Harbour lets app link recodes (GStreamer, codec libs off list), so `rust/piirit-video` builds codecs in, as `mp3lame-sys` LAME.
- **FFmpeg 7.1** (LGPL-2.1-or-later), `third_party/ffmpeg`, `release/7.1`. `--disable-everything` plus: `mov` demuxer; H.264, HEVC, AAC decoders + parsers; AAC encoder; `mp4` muxer; swscale; swresample. No network, devices, filters, autodetected libs: picked-file attack surface = few demuxers/decoders.
- **x264** (GPL-2.0-or-later), `third_party/x264`, `stable`, 8-bit 4:2:0 only. FFmpeg's own H.264 encoders wrap external libs; openh264 lacks B-frames, looked worse at same rate.
- Both GPL-3.0-or-later compatible, built from source into one executable (not bundled binary like core). `build.rs` runs each `configure` with cargo's target compiler (under scratchbox2 = target's own: native, like LAME). ARM asm on; x86 off (needs nasm; x86 only test host).
- Same x264 linked again into call H.264 encoder: Gecko encodes H.264 only via on-disk media plugin (`HARBOUR.md`).
  - `build.rs` builds `csrc/gmp_h264.cpp` → `libpiirit.so`, target C++ compiler, vs Gecko plugin headers `third_party/gmp-api` (BSD, Apache-2.0; `SOURCE.md` there).
  - Laid out as Gecko looks: `gmp-piirit/1.0/libpiirit.so` beside `piirit.info`, in `OUT_DIR` and beside binary (spec installs from there).
  - Only three entry points exported; x264 hidden. `gmp-check` (`tests/gmp_check.cpp`) = Gecko stand-in, run by `tests/gmp_h264.rs`.
- Cost: binary few MB heavier; plugin ≈ x264; clean build +couple min (cached in `target/`). C glue `rust/piirit-video/csrc`, small; Rust tests (`rust/piirit-video/tests`) vs clips made by same libs, no binary fixtures. Dependabot moves submodules along branches.
- `tokio` `net`: webxdc host loopback socket; adds only `socket2` (tokio's socket platform layer). Alternative was zip reader + inflate for archive core already reads. Call host (`call_host.rs`) same socket, adds nothing: calls-webapp committed as one released file, compiled in (`vendor/calls-webapp/`); no node toolchain.

## Comments

Comments, docs: caveman-full.

- Terse fragments. What is true now, why; keep constraints.
- No changelog, bug report, history, story (git's job). Delete comment rather than turn it into history.
- Identifiers, paths, flags exact. Item doc comments stay (`missing_docs` denied).

## Packaging: the supported path

`.github/workflows/rpm.yml` builds device RPM unattended on `ubuntu-latest`, `docker run` of Sailfish SDK, via `scripts/build-in-container.sh` (anyone with Podman or Docker same way). Reproducible: a release rebuilds byte for byte, checked before publishing (`docs/VERIFYING.md`). Dispatch from Actions tab (inputs: SDK version, cargo jobs) or push `v*` tag. **aarch64** only: only arch targeted or built.

```sh
./scripts/fetch-rpc-server.sh                        # bundled server binaries
mb2 -t SailfishOS-<ver>-<arch> -X build-init
mb2 -t SailfishOS-<ver>-<arch> -X build --no-check
```

- `-X` (`--no-fix-version`): spec `Version:`, not git tags. Needed **on `build-init`** too: else it stops at version-fixing, never writes `.mb2/spec`; `build` fails identically.
- `build-init` first (`build` reads `.mb2/spec` within 1 s).
- `--no-check`: tests host-oriented; spec has no `%check`.

Env requirements:

- **Tree in SDK user's home** (`/home/mersdk/<name>`), not `/build`/`/share`: sb2 redirects unknown absolute paths into target rootfs, so `mb2` writes `.mb2/spec` outside, rpm reads inside (file exists, can't open). Dir = package name (`mb2` derives it).
- **i686 rustlib at SDK's `/usr/lib/rustlib`.** `mb2` installs rust into *target*; build-script links run sb2 host mode (`/usr` = SDK). Copy from tooling. `ci/build-sdk-image.sh` bakes it in; upstream image needs it per build.
- **Not root.** `sdk-manage` refuses ("Cannot determine Mer SDK user"), target snapshot never initialises. Chown checkout to image's `mersdk` uid (read, don't assume), hand back for artifact upload. Script does both as root in container; host needs no sudo.

`scripts/build-rpm.sh` wraps dev path, `sfdk build`.

## Cutting a release

Release = `rpm.yml` run for a version, on `main`:

1. Bump `rpm/harbour-piirit.spec` `Version:` + three crates' `Cargo.toml`; refresh `Cargo.lock` (`cargo check`); write version's `CHANGELOG.md` section. Merge.
2. Dispatch `rpm.yml` on `main`, version in `release` input (tags `v<version>` itself); or push `v<version>` tag on merge commit.

Builds with spec's `Release: 1`, not run-number stamp. Refuses version ≠ spec, dispatch off `main`. Publishes GitHub release `v<version>`: device RPM, `SHA256SUMS`, `CHANGELOG.md` section as text (`scripts/release-notes.sh`; missing fails run). Debug/source RPMs stay run artifacts.

Harbour intake by hand: upload release RPM; `docs/HARBOUR.md` = what validator will say.

## What a device build costs

| Step | One job | Four jobs |
|---|---|---|
| Pull the SDK image | 105 s | 80 s |
| Build the RPM | 142 s | 82 s |
| Validate against Harbour | 10 s | 10 s |
| **The whole run** | **284 s** | **190 s** |

`rpm.yml` runs one job: at 2 or 4 cargo stalls in sb2 often enough to cost more than it saves (Spec constraints).

Payoffs, in order:

- **Derived SDK image.** `ci/build-sdk-image.sh`: `coderus/sailfishos-platform-sdk` by digest → one arch not three, `BuildRequires` preinstalled, i686 rustlib at `/usr/lib/rustlib`; flattened (deleted files in layers still weigh). 5.04 GB pull → ~2.3 GB; `zypper` off critical path.
- `rust/target`, crates carried between runs.

## What a package weighs

aarch64, SDK 5.2.0.15, 1.0.0 (`rpm/harbour-piirit.spec` 1.0.0-1, run 152). "In the package" = zstd payload (download); raw = on phone:

| raw | in the package | what |
|---|---|---|
| 20.42 MB | 9.38 MB | `%{_libexecdir}/%{name}/deltachat-rpc-server` |
| 5.83 MB | 1.46 MB | `%{_bindir}/%{name}` |
| 3.05 MB | 3.01 MB | `qml/art/intro-*.png` |
| 1.42 MB | ~0.1 MB | 41 `.qm` catalogs |
| ~0.7 MB | ~0.6 MB | rest of `qml/`, icons, `LICENSE`, `SOURCE.md` |

- Server = fixed: upstream stripped static-musl build, nowhere else for it (`HARBOUR.md`). Rest ours; two fixed:
- **Pictures at draw size.** `IntroPage` draws at `min(width * 0.42, height * 0.30)`: 453 px tallest phone, 614 px Jolla Tablet. 1254 px PNG masters (already compressed) shipped whole, scaled down on device. Five at 640 px = 0.75 MB, not 3.05 MB. `the_intro_pictures_are_the_shape_the_shader_reads` pins both ends of range.
- **Stripped by `[profile.release]`, not rpmbuild** (doesn't strip here): validator said `file is not stripped!` up to 1.0.0, 2.08 MB symbol tables. `strip = "symbols"` removes them; `main` stays reachable via `.dynsym` (`HARBOUR.md`). Same block: `lto = "thin"`, `codegen-units = 1`.

Those three settings, `main` vs branch:

| | before (run 160) | after (run 161) |
|---|---|---|
| Download | 15,027,710 B (14.33 MiB) | 12,254,749 B (11.69 MiB) |
| Installed | 65126 blocks (31.80 MiB) | 55270 blocks (26.99 MiB) |

LTO: build 5.5 → 6.5 min at one cargo job.

## Spec constraints

In `rpm/harbour-piirit.spec`:

- **Cargo jobs under sb2 = define, 1.** At `-j2`/`-j4` cargo stalls in sb2 (futex-waiting forever on unreaped child during qmetaobject C++ glue), so `%{jobs}` defaults **1**; `rpm.yml` always passes `mb2 build --define "jobs 1"`. sb2 only; native OBS worker lets cargo pick. Build temporaries kept in build dir: parallel link via shared `/tmp` under sb2 can lose fresh object file. `--define "jobs N"` for local trial.
- **No `--target` for cargo.** Jolla's cargo pins build scripts to tooling host triple; `--target` makes whole build cross. `SB2_RUST_TARGET_TRIPLE` already sets rustc output. Whisperfish's spec passes none either.
- **`CARGO_TARGET_<HOST>_LINKER=host-gcc` inside sb2.** rustc links build scripts via `cc`, sb2 rewrites to *cross* compiler (`aarch64-meego-linux-gnu-cc: unrecognized option '-m32'`); sb2's native = `host-gcc`. Tooling gcc by absolute path fails: sb2 still rewrites its `ld` (`cannot find /lib/libgcc_s.so.1`).
- **`QT_INCLUDE_PATH`/`QT_LIBRARY_PATH` exported in `%build`**: qttypes can't exec target `qmake` in sb2. `QT_LIBRARY_PATH` = `%{_libdir}` (Qt in `/usr/lib64` on aarch64, not `/usr/lib`).
- **`%{_target_cpu}`, not `%{_arch}`**, for bundled server path.
- **`Exec=harbour-piirit`** in desktop file: invoker ignores `Exec=env FOO=bar`, so bundled server path = fallback in binary.
- **Harbour constrains name, paths, every `Requires:`**: `ci/harbour-check.sh` fails violations; `HARBOUR.md` maps them, incl. what package still breaks.
- **No bare `%` in spec comments.** SDK's older rpm expands macros in comments: `%build` → preamble starting `LANG=C`, read as tag. Host rpm 4.18 doesn't: local parse passes, SDK fails. `ci/packaging-lint.sh` checks.
