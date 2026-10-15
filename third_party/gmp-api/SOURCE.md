# Gecko Media Plugin API headers

Headers = interface between Gecko and media plugin: classes Gecko hands plugin (video host, frames it allocates, callback for encoded pictures), class plugin returns (video encoder), three C entry points Gecko looks plugin lib up by. Piirit's video-call H.264 encoder, `rust/piirit-video/csrc/gmp_h264.cpp`, built against them. Nothing here compiled alone.

- **Project:** Mozilla Firefox (Gecko), engine under Sailfish WebView: xulrunner-qt5, ESR 91.
- **Source:** https://github.com/mozilla/gecko-dev, branch `esr91` (last ESR 91 release, 91.13), dir `dom/media/gmp/gmp-api/`.
- **Changes:** none. Only encoder-included files copied:

| file | sha256 |
| --- | --- |
| gmp-entrypoints.h | `1140f4a0f820da84a0f04fc7510093bdfc88e02035eaf98d32dedb9ca535a8fa` |
| gmp-errors.h | `d4c26779987e9a68b48fcf2e0f255fd7b397fdfdbabb34c5a6efd0768f06e8c2` |
| gmp-platform.h | `9ca466542fb56c5f7da7e0074294bcf8d6bc9b390a59765f660fd039831af741` |
| gmp-storage.h | `2baa321c4faac945834a22f1a68eecb34276a1391f979c0884bbdcd7ed8c43ac` |
| gmp-video-codec.h | `e36e3dc13a1b1cede422774dd7262cca193da445bcfa5323f2acadfbf76b9d48` |
| gmp-video-encode.h | `a09a0ac9a11497784c076cd895a634ee51186b202d18ef001f54cb2e2d0be7fc` |
| gmp-video-frame-encoded.h | `1afbddb698411616ac8951eb0f30dd59e5e07ab3e147fec98bc9a67a377f1b09` |
| gmp-video-frame-i420.h | `33fe35cedb747dc5e17c5058eb0f7f1448ffd47ce38bb308dd18ecb0976d82e7` |
| gmp-video-frame.h | `15be23eb34201f9d539c91447071e55fab635be64d8541a668bd9f73d3109996` |
| gmp-video-host.h | `9bb02492469fa5a57dd3680feb1805960d117ccae9bbb1e20d322b842a45316d` |
| gmp-video-plane.h | `877b44077cdf91b1c7ca606f647e77f8ed71f3bb678f0f24512fe5327f7554f8` |

`gmp-storage.h` not used by encoder; `gmp-platform.h` includes it.

## Licences

Each file carries own notice, unmodified:

- `gmp-storage.h`: Apache License 2.0, Copyright 2013, Mozilla Foundation and contributors.
- Every other file: BSD licence in header, Copyright (c) 2014, Mozilla. Video classes also Copyright (c) 2011, The WebRTC project authors.

Both compatible with Piirit's GPL-3.0-or-later, under which plugin built from them is distributed.
