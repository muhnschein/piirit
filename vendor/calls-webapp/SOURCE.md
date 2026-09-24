# `calls-webapp` provenance

`index.html` in this directory is the page a call runs in. It is an
**unmodified upstream build**, compiled into the app
(`rust/piirit-shim/src/call_host.rs`) and served to the browser engine
from the loopback address a call is given:

- **Project:** deltachat/calls-webapp, the WebRTC page Delta Chat's
  clients have run their calls in
- **Source code:** https://github.com/deltachat/calls-webapp
- **Version / tag:** `v0.12.1`, commit
  `b6c989216fc6dc2ba67599872dcfe77ba3b6ca9f`
- **File:** the release's one asset, `index.html` -- 44,751 bytes,
  sha256 `e4a94065a848be0b52f75c9ad53fd5a674e4e66cedc1d75c8a92baf74681e951`
- **Build:** upstream's own CI build of the tag (`pnpm build`, which is
  `tsc && vite build` into one self-contained file). Reproducible:
  `pnpm install --frozen-lockfile && pnpm build` at the tag gives the
  same bytes, so the checksum above is also what the source rebuilds to.
- **Fetched by:** `scripts/fetch-calls-webapp.sh`, which pins the version
  and the checksum. `--check` fetches the release again and requires this
  copy to match it byte for byte; CI runs that (`ci/vendor-check.sh`).

## Licences

- **calls-webapp itself:** GNU GPL version 3 (its `LICENSE`, unmodified).
  Piirit is GPL-3.0-or-later, and the two are combined in one binary
  under GPL-3.0. The corresponding source is the tag above, whose
  build is reproducible as described; this file is installed with the
  package so that every recipient is told where it is.
- **Bundled into the page by its build:**
  - [Preact](https://github.com/preactjs/preact), MIT licence,
    Copyright (c) 2015-present Jason Miller.
  - Icons from Google's
    [Material Symbols](https://github.com/google/material-design-icons),
    through Iconify, Apache License 2.0, Copyright Google LLC.

  The built file carries neither notice, so they are given here.

## What Piirit adds around it

Nothing inside the file. The page asks its host for a `calls.js` before
its own code runs -- upstream leaves that name to whoever embeds it --
and Piirit's host answers with its own bridge
(`rust/piirit-shim/src/calls.js`): the five functions the page calls,
backed by the core. The page also asks for a `webxdc.js`, which only its
own development setup uses; the host answers that with an empty script.

## Updating

Bump `VERSION` and `SHA256` in `scripts/fetch-calls-webapp.sh` together,
run it, and update this file to match. Read upstream's changes to
`src/lib/calls.ts` first: the hash commands and the `window.calls`
functions are the whole of the contract with the bridge, and a new one
is a change to `calls.js` as well.
