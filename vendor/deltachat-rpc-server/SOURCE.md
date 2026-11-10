# `deltachat-rpc-server` provenance

Binaries bundled by Piirit's RPM (installed to `/usr/share/harbour-piirit/libexec/deltachat-rpc-server`) = **unmodified upstream builds**:

- **Project:** Delta Chat core (chatmail core library)
- **Source:** https://github.com/chatmail/core
- **Version / tag:** `v2.69.0`
- **License:** MPL-2.0. Piirit GPL-3.0-or-later; the two sit side by side in RPM as separate works. This file installed with package for MPL-2.0 §3.2(a): recipients told how to get Source Code Form.
- **Build:** upstream release builds from `.github/workflows/deltachat-rpc-server.yml` nix builds, **statically linked against musl libc** ("to avoid problems with glibc version incompatibility", per that workflow). Triples: `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf` (hard-float, matches Sailfish `armv7hl`), `x86_64-unknown-linux-musl`.
- **Channel:** PyPI `deltachat-rpc-server==2.69.0` wheels = same binaries as upstream GitHub release (both nix output). Fetched, checksum-verified, placed by `scripts/fetch-rpc-server.sh`, which pins sha256 of every wheel *and* every extracted binary.

## Layout

```
vendor/deltachat-rpc-server/
  aarch64/deltachat-rpc-server   sha256 206beb5ff18ed1cb2929f822dee95fed4d1793a1bbdffecb1925793d8fd4b978
  armv7hl/deltachat-rpc-server   sha256 fd388e10dce4d6d2bc68c3b1bc5d9a42cd72e403d36c787a81e60f96d84cdfb4
  x86_64/deltachat-rpc-server    sha256 db1a2c08375c9c2e73c8d6009cc99094e642722a41d06370cdb4dc680247eaa0
```

Dir names = Sailfish arch names (`%{_target_cpu}`, not `%{_arch}`, which maps every armv7h* to `arm`). `rpm/harbour-piirit.spec` `%install` keys on them.

Binaries not committed (see `.gitignore`). Run fetch script to populate.

## Verified so far / still open

- [x] All three binaries statically linked and stripped (`file`, `ldd`).
- [x] x86_64 binary runs, answers over stdio. Gated test `rust/deltachat-jsonrpc/tests/real_server.rs` (`DELTACHAT_RPC_SERVER=<path> cargo test -p deltachat-jsonrpc`) passes: `get_system_info` health check, account creation, config round trip, transport added against stub mail server on 127.0.0.1, real event delivery, chat/message list wire shapes UI depends on.
- [ ] `aarch64`/`armv7hl` binaries on real Sailfish device or emulator. Static musl = only kernel ABI matters, so expected to run as-is. Only provable on hardware.

## Updating

Bump `VERSION` and all sha256 pins in `scripts/fetch-rpc-server.sh` together, re-run, update this file. Bundled version and version stated here must never drift apart.
