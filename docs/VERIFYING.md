# Verifying a release

Each release RPM rebuilds from its tag to the same file, byte for byte. Check
one yourself:

```
$ git clone https://github.com/muhnschein/piirit.git
$ cd piirit
$ scripts/verify-release.sh          # latest release
$ scripts/verify-release.sh 2.2.0    # or a given one
```

Needs Linux on x86_64, git, curl, and Podman (rootless is fine) or Docker.
Both installed: Docker if running; `--podman` or `--docker` picks.
First run: ~2.3 GB image pull (~7 GB unpacked, where Podman or Docker keep
images), ~30 min build. Builds in a fresh clone of the tag under
`~/.cache/piirit-verify`, never in your tree: ~1 GB, checks for 2 GB free.
Elsewhere: `PIIRIT_WORKDIR=/some/big/disk scripts/verify-release.sh`. Not
`/tmp`: often a RAM disk, small and shared.

| Result | Exit | Meaning |
|---|---|---|
| `VERIFIED` | 0 | Same file as published. |
| `SAME FILES` | 1 | Every file inside matches; packaging (build time, file dates) differs. Expected for 2.1.0 and earlier, which predate reproducible builds. |
| `DIFFERENT` | 2 | Files differ. Please open an issue with the output. |

## Not trusting our SDK image

The build runs in our SDK image: upstream's Sailfish SDK image
(`coderus/sailfishos-platform-sdk`, pinned by digest in
`ci/build-sdk-image.sh`) with build dependencies preinstalled and other
architectures removed by that script. To derive it yourself instead of
pulling ours:

```
$ scripts/verify-release.sh --upstream-sdk
```

~5 GB more download, ~30 min more. Rootless Podman: upstream's build user is
uid 100000, beyond the 65536 subordinate ids a user gets by default. Once:

```
$ sudo usermod --add-subuids 100000-599999 --add-subgids 100000-599999 $USER
$ podman system migrate
```

## An RPM from elsewhere

E.g. one copied off a phone. Checked with GitHub's in one run: one build,
each copy compared with it, a line per copy; exit is the worst.

```
$ scripts/verify-release.sh --rpm harbour-piirit-2.2.0-1.aarch64.rpm
github: VERIFIED, same file as the rebuild, byte for byte.
local: VERIFIED, same file as the rebuild, byte for byte.
```

`--no-download`: that copy only. Store installs keep no RPM; on the phone,
`pkcon download ~/Downloads harbour-piirit` fetches it again (untested: Piirit
isn't in the Store yet).

## What makes it reproducible

- Package build time and file dates: the commit's time (`SOURCE_DATE_EPOCH`,
  set by `scripts/build-in-container.sh`); build host fixed (spec).
- Build environment: SDK image pinned by digest (`ci/sdk-images`); same build
  path inside it.
- Inputs pinned: `rust/Cargo.lock`, FFmpeg and x264 submodule commits,
  deltachat-rpc-server by sha256 (`scripts/fetch-rpc-server.sh`).

Releases pass this same check in CI before publishing (`reproduce` in
`.github/workflows/rpm.yml`: fresh clone, Podman, other runner); `main` weekly
(`.github/workflows/scheduled.yml`), with `--upstream-sdk` too.

## What it doesn't cover

- deltachat-rpc-server is upstream's binary, checked by hash, not rebuilt
  here. Its own build: `vendor/deltachat-rpc-server/SOURCE.md`.
- The Sailfish SDK's compilers and libraries themselves: you trust Jolla's
  SDK, as every Sailfish app does.
- macOS, Windows, ARM hosts: untested. The image is x86_64.
