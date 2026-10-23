# `deltachat-rpc-server` provenance

Binaries bundled by Piirit's RPM (installed to `/usr/libexec/harbour-piirit/deltachat-rpc-server`) = **unmodified upstream builds**:

- **Project:** Delta Chat core (chatmail core library)
- **Source:** https://github.com/chatmail/core
- **Version / tag:** `v2.65.0`
- **License:** MPL-2.0. Piirit GPL-3.0-or-later; the two sit side by side in RPM as separate works. This file installed with package for MPL-2.0 §3.2(a): recipients told how to get Source Code Form.
- **Build:** upstream release builds from `.github/workflows/deltachat-rpc-server.yml` nix builds, **statically linked against musl libc** ("to avoid problems with glibc version incompatibility", per that workflow). Triples: `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf` (hard-float, matches Sailfish `armv7hl`), `x86_64-unknown-linux-musl`.
- **Channel:** PyPI `deltachat-rpc-server==2.65.0` wheels = same binaries as upstream GitHub release (both nix output). Fetched, checksum-verified, placed by `scripts/fetch-rpc-server.sh`, which pins sha256 of every wheel *and* every extracted binary.

## Layout

```
vendor/deltachat-rpc-server/
  aarch64/deltachat-rpc-server   sha256 01c0f9ff0ce1fe8c3f99ba8f66d056f75653fdf2e39c00decac374285097c33c
  armv7hl/deltachat-rpc-server   sha256 0da3625b78ad06ae2cafcbeff115aa218c76aa12a475e47d8aae017222c06e8a
  x86_64/deltachat-rpc-server    sha256 fc4945c3179499f5b69579d224e3da2f1f4bfa5ce927db4253fba844bec4ad1a
```

Dir names = Sailfish arch names (`%{_target_cpu}`, not `%{_arch}`, which maps every armv7h* to `arm`). `rpm/harbour-piirit.spec` `%install` keys on them.

Binaries not committed (see `.gitignore`). Run fetch script to populate.

## Verified so far / still open

- [x] All three binaries statically linked and stripped (`file`, `ldd`).
- [x] x86_64 binary runs, answers over stdio. Gated test `rust/deltachat-jsonrpc/tests/real_server.rs` (`DELTACHAT_RPC_SERVER=<path> cargo test -p deltachat-jsonrpc`) passes: `get_system_info` health check, account creation, config round trip, transport added against stub mail server on 127.0.0.1, real event delivery, chat/message list wire shapes UI depends on.
- [ ] `aarch64`/`armv7hl` binaries on real Sailfish device or emulator. Static musl = only kernel ABI matters, so expected to run as-is. Only provable on hardware.

## Updating

Bump `VERSION` and all sha256 pins in `scripts/fetch-rpc-server.sh` together, re-run, update this file. Bundled version and version stated here must never drift apart.
