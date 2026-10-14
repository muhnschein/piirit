# Gecko Media Plugin API headers

The headers in this directory are the interface between Gecko and a
media plugin: the classes Gecko hands a plugin (a video host, the frames
it allocates, the callback it is told about encoded pictures through),
the class a plugin hands back (a video encoder), and the three C entry
points Gecko looks a plugin library up by. Piirit's H.264 encoder for
video calls, `rust/piirit-video/csrc/gmp_h264.cpp`, is built against
them; nothing here is compiled on its own.

- **Project:** Mozilla Firefox (Gecko), the engine the Sailfish WebView
  is built on: xulrunner-qt5, ESR 91.
- **Source code:** https://github.com/mozilla/gecko-dev, branch `esr91`
  (the last ESR 91 release, 91.13), directory `dom/media/gmp/gmp-api/`.
- **Changes:** none. Only the files the encoder includes are copied:

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

  `gmp-storage.h` is not used by the encoder; `gmp-platform.h` includes
  it.

## Licences

Each file carries its own notice, unmodified:

- `gmp-storage.h`: Apache License 2.0, Copyright 2013, Mozilla
  Foundation and contributors.
- Every other file: the BSD licence in its header, Copyright (c) 2014,
  Mozilla, and for the video classes also Copyright (c) 2011, The WebRTC
  project authors.

Both are compatible with Piirit's GPL-3.0-or-later, under which the
plugin built from them is distributed.
