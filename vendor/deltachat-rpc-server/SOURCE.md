# `deltachat-rpc-server` provenance

The `deltachat-rpc-server` binaries bundled by Piirit's RPM (installed to
`/usr/libexec/harbour-piirit/deltachat-rpc-server`) are **unmodified
upstream builds**:

- **Project:** Delta Chat core (chatmail core library)
- **Source code:** https://github.com/chatmail/core
- **Version / tag:** `v2.62.0`
- **License:** MPL-2.0. Piirit itself is GPL-3.0-or-later; the two sit
  side by side in the RPM as separate works, and this file is installed
  with the package to satisfy MPL-2.0 §3.2(a)'s requirement that recipients
  of the Executable Form be told how to obtain the corresponding Source
  Code Form.
- **Build:** upstream's own release builds, produced by their
  `.github/workflows/deltachat-rpc-server.yml` nix builds, **statically
  linked against musl libc** ("to avoid problems with glibc version
  incompatibility", per that workflow). Target triples:
  `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf`
  (hard-float, matching Sailfish `armv7hl`), `x86_64-unknown-linux-musl`.
- **Distribution channel used:** the PyPI `deltachat-rpc-server==2.62.0`
  wheels, which contain the same binaries upstream attaches to its GitHub
  release (both are the nix build output). Fetched, checksum-verified, and
  placed here by `scripts/fetch-rpc-server.sh` — that script pins the
  exact sha256 of every wheel *and* every extracted binary.

## Layout

```
vendor/deltachat-rpc-server/
  aarch64/deltachat-rpc-server   sha256 a81a75e2de356c5b0074100aee655a7a1884dcc747c79e2b57e7b6f4316a70ee
  armv7hl/deltachat-rpc-server   sha256 ec968b307d0e82ba9012e2d194dcdab123ce4c1289b8c756b715e8b27e176973
  x86_64/deltachat-rpc-server    sha256 62aa375904aeea88f356a20cb5d7bb6b90728f5e14d81c2b0f689c6c7341af2d
```

Directory names are Sailfish's architecture names (`%{_target_cpu}` in
rpm's terms -- not `%{_arch}`, which canonicalises every armv7h* to
`arm`), which is what `rpm/harbour-piirit.spec` keys its `%install`
step on. Binaries are not
committed to git (see `.gitignore`); run the fetch script to populate.

## Verified so far / still open

- [x] All three binaries confirmed statically linked and stripped
      (`file`, `ldd`).
- [x] The x86_64 binary runs and answers over stdio: the gated integration
      test `rust/deltachat-jsonrpc/tests/real_server.rs` (run with
      `DELTACHAT_RPC_SERVER=<path> cargo test -p deltachat-jsonrpc`)
      passes against it — `get_system_info` health check, account
      creation, config round trip, a transport added against a stub mail
      server on 127.0.0.1, real event delivery, and the chat/message list
      wire shapes the UI depends on.
- [ ] `aarch64`/`armv7hl` binaries exercised on an actual Sailfish device
      or emulator (static musl linking means only the kernel ABI matters,
      so these are expected to run as-is — but this is the one claim that
      can only be proven on hardware).

## Updating

Bump `VERSION` and all sha256 pins in `scripts/fetch-rpc-server.sh`
together, re-run it, and update this file to match. The version bundled
and the version stated here must never drift apart.
