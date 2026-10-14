# Building, testing, and packaging

Standard: [clove](https://github.com/muhnschein/clove) §9: pinned toolchain, pedantic lints, CI-parity `make` targets, tests driving real binaries.

## Toolchains

- `rust-toolchain.toml` pins 1.94.1 (reproducible lints). Device floor **1.75.0** (CI `msrv`, warnings denied). rustup only; SDK cargo ignores pin.
- FFmpeg, x264 = submodules `third_party/`, built by `rust/piirit-video` build.rs. Fresh clone needs them before anything builds:

```sh
git submodule update --init --depth 1
```

- `rust/Cargo.lock` stays **v3**: v4 since cargo 1.78; SDK cargo 1.75 can't read it. `cargo update` on modern host rewrites silently; `ci/check-lockfile.sh` catches it.

## Lints

- Workspace-level; bare `cargo clippy` fails like CI. Deny: `clippy::all`, `pedantic`, `unwrap_used`/`expect_used` (non-test), `missing_docs`, `unsafe_code`.
- `unsafe_code` deny, not forbid: Qt harness tests need `env::set_var` before Qt init; no safe binding (qmetaobject wraps none) for installing `QTranslator`, voice recording via `QAudioRecorder`, QML pictures via `QQuickImageProvider`, call mic watch via libpulse.
- Those = the four `cpp!` files: `piirit-app/src/translations.rs`, `piirit-shim/src/recorder.rs`, `piirit-shim/src/pictures.rs`, `piirit-shim/src/mic_watch.rs`. Each crate's `build.rs` C++ step exists for them alone. Each exception: narrowest scope, reason stated, short enough to check by reading.
- Recorder: host `qtmultimedia5-dev` (`Makefile` lists packages), spec `pkgconfig(Qt5Multimedia)`; shim links `libQt5Multimedia` (Harbour allows). Mic watch: host `libpulse-dev`, spec `pkgconfig(libpulse)`; links `libpulse.so.0` (Harbour allows). Its test runs own sound server: needs `pulseaudio`, `pulseaudio-utils`; skipped if absent, except CI fails.
- `rust/clippy.toml` bans (both caused device-only failures):
  - `tokio::runtime::Runtime::new`: use `CoreRuntime`
  - `qmetaobject::single_shot`: truncates sub-second `Duration`s

## Testing

Piirit parses almost nothing; protocol, crypto = core's; subprocess is ours. Failure mode: misreading core JSON or calling it wrong, unnoticed until on phone. Tests aim there.

- `make check`: everything from clean checkout, no phone/account/SDK. Not quite offline: `msrv` fetches 1.75 toolchain first time; `deny` wants advisory DB.
- **Runner: `cargo nextest`**, process per test. 100+ test binaries, most start Qt engine and wait on real timers: `cargo test` ~10 min, mostly idle, serialised; nextest 207 tests ~2.5 min on 4 cores. Parallel-safe, nothing shared: webxdc and call hosts bind port 0; QML probes copy tree into pid-named dir.

```
cargo install --locked cargo-nextest
```

- Without it `make test` still runs all, slow: `make check` must work on laptop with nothing extra installed.
- `rust/.config/nextest.toml`: 30 s slow warn, 2 min kill (Qt test stuck on signal reported by name, not job timeout). No retries: pass on second attempt = defect to see.
- nextest skips doctests: `cargo test --doc` beside it (`Makefile`, `ci.yml`); `ci/packaging-lint.sh` fails tree with one but not other. No doctests today, so first would silently never run.

1. **Transport unit**: fake stdio server.
2. **Protocol-contract**: recording double journals every request; pins call sequence per onboarding action.
3. **Qt event-loop**: `QT_QPA_PLATFORM=offscreen`.
4. **QML load**: stub Silica (`tests/silica-stubs/`), real page files, driven by `objectName`. Stubs imitate no layout: nothing proves page *looks* right.
   - Silica `EnterKey` attached prop unstubbable (QML forbids capitalised property names; `qmetaobject` can't register attached types): pages using it can't load. Put shown content in loadable component.
   - Lay that out with bindings, not `Column`: positioner sizes in polish pass, never runs without window, geometry reads zero.
5. **Static QML** (`tests/qml_syntax.rs`), what no host-Qt run sees: Qt 5.6 rules Qt 5.15 accepts silently, plus tree rules: every string other end chose drawn as plain text (Silica headers can't be, hence `ConversationHeader`); file URLs encoded per segment; only picker pages import `Sailfish.Pickers`; every delegate `model.<role>` exists in its model.
6. **Real-core integration** (`real_server`, `real_core`): gated on `DELTACHAT_RPC_SERVER`, offline. `real_core.rs` tells request real core couldn't decode from one it couldn't deliver.
7. **Packaging** (`ci/packaging-lint.sh`): spec parses, desktop entry validates, shell scripts clean, every catalog current and `lrelease`-clean, every `docs/*.md` a comment points at exists. Missing tool = SKIP locally; CI `PACKAGING_LINT_STRICT=1` = fail, as `HARBOUR_CHECK_STRICT=1` for Harbour check.
8. **Report-script** (`ci/sonar-report-selftest.sh`): only script talking to service outside GitHub; run vs stub answering SonarQube's four endpoints. No other way: analysis absent at test time, and CI can't reach `sonarcloud.io` (why script exists).
9. **Runner-setup** (`ci/apt-install-selftest.sh`): rule for which apt sources every CI job keeps, proved on test's own dir. Backwards = deletes install archive, fails everything.

Aspiration, tracked not gated: test volume exceeds source volume.

## Static analysis

SonarQube Cloud reads tree every PR (`.github/workflows/build.yml`, `sonar-project.properties`). **Report, not gate**: `ci.yml` decides; Sonar can't flip build red/green. Separate workflow so hosted service never part of rule "green `make check` on laptop = green CI".

- Coverage **imported**, not measured: `make sonar-reports` writes `rust/target/sonar/lcov.info` (`cargo llvm-cov`, whole workspace); workflow runs it before scan. Without: confident 0.0%, not "no data". Needs `cargo-llvm-cov`, so opt-in, not in `make check`. Uses nextest if installed (llvm-cov's own runner = `cargo test`, ~10 min instrumented in scan job); without, still written, slow.

```
rustup component add llvm-tools-preview
cargo install --locked cargo-llvm-cov cargo-nextest
make sonar-reports
```

- Clippy findings **not** handed over; Sonar's own pass off (`sonar.rust.clippy.enabled=false`). Two reasons: Sonar invokes cargo where it finds project, workspace is under `rust/` not root, so different clippy from gate's, or none. Own report would be empty: `make lint` denies warnings, so only findings left are in excluded `third_party/qmetaobject`; and costs `cargo clean` + full recompile in scan job.
- `sonar.tests`: fixtures apart from app, so coverage/duplication measured on what ships; test volume > source, fixtures would dominate every ratio.
- `sonar.exclusions`: vendored crates, patched qmetaobject, rendered icons, `translations/` (Qt catalog named `.ts`: scanner reads 39 as TypeScript).
- `sonar.coverage.exclusions`: `qml/` out of coverage only. Can't produce report; one changed QML JS line = 0% new-code coverage, fails gate.
- `sonar.issue.ignore.multicriteria`: findings decided against, one rule on one path each, reason beside: qmetaobject glob import, cognitive complexity in tests and fake servers, char-class name taken for repeated literal.
- Scanner uploads, exits; server processes after, so producing run knows nothing of result. `scripts/sonar-report.sh` asks server from same runner, prints quality gate, measures, open issues to job log + step summary. Reports, never gates: step `continue-on-error`, outage = warning, not build.

## CI

`ci.yml` = gate; runs what `make check` runs. Runner facts:

- **Packages via `ci/apt-install.sh`**, not bare `apt-get`. `apt-get update` exits non-zero if *any* configured repo fails; runner image ships several never used, so one bad index kills every job with nothing here changed. Script drops third-party lists first, keeps Ubuntu's wherever image puts them: list survives only if it names `ubuntu.com` host. Stops it deleting archive it installs from.
- **Rust jobs cache `target/`** (`Swatinem/rust-cache`, `rust` workspace); else every job compiles full dep graph every push. `msrv` own cache key: builds `+1.75.0`, action keys on default toolchain; shared slot = neither hits. `CARGO_INCREMENTAL: 0`: runner compiles once, discarded; incremental state written, cached, never read.
- Cache worth less than looks: warm clippy ~20 s, but `test` barely moves. Its cost = suite waiting on timers (nextest addresses), not compile.
- **Test job installs `cargo-nextest`**, `ci` profile. Vs laptop: `fail-fast` off (CI asked once, report all); failures printed inline and again at end (in 200-line log summary is what's read).

## Translations

- Strings = `qsTr()` in `qml/`. `translations/piirit.ts` = untranslated source catalog; `translations/piirit-<lang>.ts` = one per language Sailfish ships in.
- `scripts/update-translations.sh`: one `lupdate` run regenerates all; new string = `unfinished` in every language at once. `ci/packaging-lint.sh` fails if committed catalog differs from that run. `tests/translation_catalogs.rs` fails on any untranslated string: new string not done until every catalog has it.
- App loads `piirit-<lang>.qm`, compiled by `scripts/release-translations.sh` (`lrelease`): RPM `%build`; locally `make translations`, leaves them beside `.ts` where source-tree run finds them. Tools: Debian `qttools5-dev-tools`, SDK `qt5-qttools-linguist`; also test dep (app's test compiles German catalog).
- `<lang>` = what `QTranslator` matches vs reader locale, most specific first: `de` serves all German; `pt_BR` only Brazil; no catalog = English one (strings already English; catalog holds plural forms).
- New language: write three-line header (`update-translations.sh` documents) to `translations/piirit-<lang>.ts`, run script; `lupdate` fills every string with that language's plural forms.

## Dependencies

Few, each for a reason. `cargo tree` on app = list; table = why each stays, so drop proposal starts from cost.

| Crate | Use | Why it stays |
|---|---|---|
| `tokio` | server subprocess, pipes, event loop | async transport; `process` reaps child |
| `serde`, `serde_json` | wire JSON | core contract is JSON-RPC |
| `qmetaobject` (vendored), `qttypes`, `cpp`, `cpp_build` | Qt from Rust | whole UI hangs off them; `default-features = false` keeps `log` bridge out |
| `chrono` | viewer timezone, for day headings | `std` lacks; alternative `localtime_r` needs `unsafe`, which `unsafe_code` denies |
| `qrcode` | invite drawn as code | one crate, no deps |
| `rqrr` (+ `g2p`, `lru`) | code read off camera | QR decoder not small to vendor |
| `mp3lame-encoder` (+ `mp3lame-sys`, `autotools`) | voice message as MP3 (`voice.rs`) | phone recorder offers no encoder iOS client plays as voice message; LAME = desktop client's encoder; pure-Rust encoders need Rust past 1.75 floor or are shine ports (no psychoacoustic model) |

- `mp3lame-sys` carries LAME 3.100 C source, builds via LAME's `configure` + `make` (host needs both; C++ build machine has them). Only non-Qt C dep; why `rust/deny.toml` allows LGPL-3.0 for its two crates only: LGPL code may be conveyed as part of GPLv3 work.

### Making a video smaller

Picked video recoded on pick, before send, when near core's recommended message size (4/5 of it) or outgoing media quality set worse (`rust/piirit-shim/src/video.rs`, `plan` in `rust/piirit-video`, issue #111). Phone recorder writes several times recommended size; core sends video as-is. Nothing Harbour lets app link can recode (GStreamer, codec libs not on allowed list), so `rust/piirit-video` builds codecs into binary, as `mp3lame-sys` does LAME:

- **FFmpeg 7.1** (LGPL-2.1-or-later), `third_party/ffmpeg`, `release/7.1` branch. `--disable-everything` plus only: `mov` demuxer; H.264, HEVC, AAC decoders + parsers; AAC encoder; `mp4` muxer; swscale; swresample. No network, devices, filters, autodetected system libs. Trimming = most of security story: picked-file parsing = few demuxers/decoders, not FFmpeg's hundreds.
- **x264** (GPL-2.0-or-later), `third_party/x264`, `stable` branch, 8-bit 4:2:0 only. FFmpeg's own H.264 encoders wrap external libs; openh264 (other candidate) has no B-frames, looked visibly worse at same bit rate.

Both GPL-3.0-or-later compatible; both come as source: code in one executable, not another bundled binary like core. `build.rs` runs each project's own `configure` with cargo's target compiler (under scratchbox2 = target's own: device build native, as LAME's). Hand-written asm on for ARM, off for x86 (needs nasm; x86 only ever test host).

Same x264 linked second time, into H.264 encoder video calls send through: Gecko encodes H.264 only via media plugin loaded from disk, never via app binary (docs/CALLS.md).

- `build.rs` builds `csrc/gmp_h264.cpp` into `libpiirit.so` with target C++ compiler, vs Gecko plugin headers `third_party/gmp-api` (BSD, Apache-2.0; `SOURCE.md` there).
- Laid out as Gecko looks: `gmp-piirit/1.0/libpiirit.so` beside `piirit.info`, in `OUT_DIR` and again beside binary (spec installs from there).
- Only three entry points visible; x264 inside, hidden. Tests: `gmp-check` (`tests/gmp_check.cpp`) = Gecko-side stand-in, run by `tests/gmp_h264.rs`.

Cost: binary few MB heavier; plugin weighs what x264 does; clean build +couple min configure/compile (cargo keeps result in `target/`). C glue `rust/piirit-video/csrc`, small enough to read; tests in Rust (`rust/piirit-video/tests`) vs clips made by same libs, no binary fixtures. Dependabot moves submodules along branches.

`tokio` `net` feature: webxdc host's loopback socket; adds only `socket2` (tokio's socket platform layer). Alternative: zip reader + inflate to unpack app core already reads. Call host (`call_host.rs`) = same socket, adds nothing: page = upstream calls-webapp, committed as its one released file, compiled in (`vendor/calls-webapp/`); no node toolchain near build.

## Comments

House style, comments and docs: caveman-full.

- Terse fragments. State what is true now and why; keep constraints.
- No changelog, bug report, history or story (git's job). Delete comment rather than update it into history of its subject.
- Identifiers, paths, flags exact. Doc comments on items stay (`missing_docs` denied).

## Packaging: the supported path

`.github/workflows/rpm.yml` builds device RPM unattended on `ubuntu-latest`, `docker run` of Sailfish SDK, via `scripts/build-in-docker.sh` (anyone with Docker can build same way). Dispatch from Actions tab (SDK version, cargo job count = inputs) or push `v*` tag. Builds **aarch64**: only arch targeted, only one ever built.

```sh
./scripts/fetch-rpc-server.sh                        # bundled server binaries
mb2 -t SailfishOS-<ver>-<arch> -X build-init
mb2 -t SailfishOS-<ver>-<arch> -X build --no-check
```

- `-X` (`--no-fix-version`): spec `Version:`, not git tags. Needed **by `build-init`** too: without it step gives up at version-fixing, never writes `.mb2/spec`; `build` fails identically, flag looks innocent.
- `build-init` before `build` (`build` queries `.mb2/spec` within 1 s of start).
- `--no-check`: tests host-oriented; spec has no `%check`.

Env requirements, each cost an attempt:

- **Tree inside SDK user's home** (`/home/mersdk/<name>`), not `/build`/`/share`. rpm runs under scratchbox2, redirects unrecognised absolute paths into target rootfs; tree elsewhere = `mb2` writes `.mb2/spec` outside, rpm reads inside. Signature: file exists, can't open. Dir keeps package name (`mb2` derives package from it).
- **i686 rustlib at SDK's own `/usr/lib/rustlib`.** `mb2` installs rust into *target*, but build-script links run in sb2 host mode, `/usr` = SDK filesystem. Copy from tooling. `ci/build-sdk-image.sh` does it once, into image; build on upstream image still must.
- **Not root.** `sdk-manage` refuses ("Cannot determine Mer SDK user"); target snapshot never initialises. Chown checkout to container's `mersdk` uid (read from image, don't assume); hand back so artifact upload reads result. Script does both as root in container; host needs no sudo.

`scripts/build-rpm.sh` wraps ordinary dev path, `sfdk build`.

## Cutting a release

Release = `rpm.yml` run for a version, on `main`:

1. Version in `rpm/harbour-piirit.spec` (`Version:`) + three crates' `Cargo.toml`; refresh `Cargo.lock` (`cargo check`); write version's `CHANGELOG.md` section. Merge.
2. Dispatch `rpm.yml` on `main` from Actions tab, version in `release` input (tags commit `v<version>` itself); or tag merge commit `v<version>` by hand, push tag (same workflow).

Workflow builds with spec's own `Release: 1`, not run-number stamp. Refuses version != spec, and dispatch off `main`. Publishes GitHub release `v<version>`: device RPM, `SHA256SUMS`, version's `CHANGELOG.md` section as text (`scripts/release-notes.sh`, fails run if missing). Debug/source RPMs stay on run artifact.

Harbour intake by hand: upload release-page RPM; `docs/HARBOUR.md` = what validator will say.

## What a device build costs

| Step | One job | Four jobs |
|---|---|---|
| Pull the SDK image | 105 s | 80 s |
| Build the RPM | 142 s | 82 s |
| Validate against Harbour | 10 s | 10 s |
| **The whole run** | **284 s** | **190 s** |

`rpm.yml` runs one job. Four faster when it finishes, but at 2 or 4 cargo stalls in scratchbox2 often enough that waiting costs more than parallelism saves (Spec constraints).

Changes, in order they pay:

- **SDK image derived, not upstream's.** `ci/build-sdk-image.sh`: `coderus/sailfishos-platform-sdk` by digest -> one arch not three, package `BuildRequires` preinstalled, i686 rustlib at `/usr/lib/rustlib`. Flattens, not layers (files deleted in new layer still weigh). 5.04 GB pull -> ~2.3 GB; `zypper` off critical path.
- `rust/target` and crates carried between runs.

## What a package weighs

aarch64, SDK 5.2.0.15. "In the package" = cost in zstd payload (download); raw = cost on phone. Taken apart at 1.0.0 (`rpm/harbour-piirit.spec` 1.0.0-1, run 152):

| raw | in the package | what |
|---|---|---|
| 20.42 MB | 9.38 MB | `%{_libexecdir}/%{name}/deltachat-rpc-server` |
| 5.83 MB | 1.46 MB | `%{_bindir}/%{name}` |
| 3.05 MB | 3.01 MB | `qml/art/intro-*.png` |
| 1.42 MB | ~0.1 MB | 41 `.qm` catalogs |
| ~0.7 MB | ~0.6 MB | rest of `qml/`, icons, `LICENSE`, `SOURCE.md` |

- Bundled server = fixed cost: upstream stripped static-musl build; Harbour leaves nowhere else for core (`HARBOUR.md`). Other rows = this tree's; two answered:
- **Pictures painted at draw size.** `IntroPage` shows one at `min(width * 0.42, height * 0.30)`: 453 px tallest phone, 614 px Jolla Tablet. PNG already compressed, so 1254 px master went whole into download, scaled away on device. At 640 px five = 0.75 MB, not 3.05 MB. `the_intro_pictures_are_the_shape_the_shader_reads` pins both ends of range: no regrowth without failing test.
- **Binary stripped by `[profile.release]`, not rpmbuild.** rpmbuild here doesn't strip: validator said `file is not stripped!` of every build to 1.0.0, 2.08 MB symbol tables. `strip = "symbols"` removes them; `main` reachable since `build.rs` exports it into `.dynsym` (`HARBOUR.md`). `lto = "thin"`, `codegen-units = 1` same block.

Either side of those three settings, `main` vs introducing branch:

| | before (run 160) | after (run 161) |
|---|---|---|
| Download | 15,027,710 B (14.33 MiB) | 12,254,749 B (11.69 MiB) |
| Installed | 65126 blocks (31.80 MiB) | 55270 blocks (26.99 MiB) |

LTO costs build ~1 min: 5.5 -> 6.5 min at one cargo job.

## Spec constraints

Encoded in `rpm/harbour-piirit.spec`:

- **Cargo job count under sb2 = define, value 1.** At `-j2`/`-j4` cargo stalls under sb2 (seen futex-waiting forever on unreaped child while qmetaobject C++ glue compiles), so `%{jobs}` defaults **1**; `rpm.yml` passes `mb2 build --define "jobs 1"` every run, no choice. Only inside sb2; native OBS worker lets cargo pick. Spec also keeps build temporaries in build dir: parallel link via shared `/tmp` under sb2 can lose just-written object file. `--define "jobs N"` remains for local trial.
- **No `--target` for cargo.** Jolla's cargo pins build scripts to tooling host triple; `--target` on top makes cargo treat whole build as cross. `SB2_RUST_TARGET_TRIPLE` already tells accelerated rustc what to emit. Whisperfish's spec passes none either.
- **`CARGO_TARGET_<HOST>_LINKER=host-gcc` inside sb2.** rustc links build scripts via plain `cc`, which sb2 rewrites to *cross* compiler (`aarch64-meego-linux-gnu-cc: unrecognized option '-m32'`). scratchbox2 exposes native compiler as `host-gcc`. Tooling gcc by absolute path not enough: sb2 still rewrites `ld` gcc invokes (`cannot find /lib/libgcc_s.so.1`).
- **`QT_INCLUDE_PATH`/`QT_LIBRARY_PATH` exported in `%build`**: qttypes can't exec target `qmake` under sb2. `QT_LIBRARY_PATH` uses `%{_libdir}`: Qt in `/usr/lib64` on aarch64, not `/usr/lib`.
- **`%{_target_cpu}`, not `%{_arch}`**, for bundled server path.
- **`Exec=harbour-piirit`** in desktop file: invoker ignores `Exec=env FOO=bar` wrapper, so bundled server path = fallback inside binary.
- **Harbour constrains name, paths, every `Requires:`.** `ci/harbour-check.sh` fails build breaking one; `HARBOUR.md` = map, incl. two rules package still breaks.
- **No bare `%` in spec comment.** rpm expands macros in comments; on SDK's older rpm, comment mentioning `%build` expands to preamble starting `LANG=C`, read as tag. Host rpm 4.18 leaves comments alone: parses locally, fails only on SDK. `ci/packaging-lint.sh` checks directly.
