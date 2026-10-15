# `calls-webapp` provenance

`index.html` here = page a call runs in. **Unmodified upstream build**, compiled into app (`rust/piirit-shim/src/call_host.rs`), served to browser engine from loopback address a call is given.

- **Project:** deltachat/calls-webapp, WebRTC page Delta Chat clients run calls in.
- **Source:** https://github.com/deltachat/calls-webapp
- **Version / tag:** `v0.12.1`, commit `b6c989216fc6dc2ba67599872dcfe77ba3b6ca9f`
- **File:** release's one asset, `index.html`. 44,751 bytes, sha256 `e4a94065a848be0b52f75c9ad53fd5a674e4e66cedc1d75c8a92baf74681e951`
- **Build:** upstream CI build of tag (`pnpm build` = `tsc && vite build`, one self-contained file). Reproducible: `pnpm install --frozen-lockfile && pnpm build` at tag gives same bytes; checksum = what source rebuilds to.
- **Fetched by:** `scripts/fetch-calls-webapp.sh`, pins version + checksum. `--check` fetches again, requires byte-for-byte match. CI runs it (`ci/vendor-check.sh`).

## Licences

- **calls-webapp:** GNU GPL version 3 (its `LICENSE`, unmodified). Piirit GPL-3.0-or-later; combined in one binary under GPL-3.0. Corresponding source = tag above, reproducible build. File installed with package so recipients told where.
- **Bundled into page by its build:**
  - [Preact](https://github.com/preactjs/preact), MIT, Copyright (c) 2015-present Jason Miller.
  - Icons: Google [Material Symbols](https://github.com/google/material-design-icons) via Iconify, Apache License 2.0, Copyright Google LLC.
  - Built file carries neither notice; given here.

## What Piirit adds around it

Nothing inside file. Page asks host for `calls.js` before own code runs (upstream leaves name to embedder). Piirit host answers with own bridge (`rust/piirit-shim/src/calls.js`): five functions page calls, backed by core. Page also asks for `webxdc.js` (only upstream dev setup uses it); host answers with empty script.

## Updating

Bump `VERSION` and `SHA256` in `scripts/fetch-calls-webapp.sh` together, run it, update this file. Read upstream changes to `src/lib/calls.ts` first: hash commands and `window.calls` functions = whole contract with bridge. New one = change to `calls.js` too.
