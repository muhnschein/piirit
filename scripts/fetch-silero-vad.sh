#!/bin/sh
# Fetches speech detector the engine checks clips with before transcribing:
# whisper.cpp's ggml build of Silero VAD from Hugging Face. See
# vendor/silero-vad/SOURCE.md.
#
# Model is committed, unlike core binaries: 0.9 MB, shipped in RPM beside
# engine libs (rust/piirit-speech/build.rs); builds must not need network.
#
#     scripts/fetch-silero-vad.sh           fetch, verify, install
#     scripts/fetch-silero-vad.sh --check   fetch, verify, and require
#                                           the committed copy to match
#
# Pinned: bump COMMIT and SHA256 together, then SOURCE.md.

set -eu

FILE="ggml-silero-v5.1.2.bin"
COMMIT="9ffd54a1e1ee413ddf265af9913beaf518d1639b"
SHA256="29940d98d42b91fbd05ce489f3ecf7c72f0a42f027e4875919a28fb4c04ea2cf"

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
target="$repo_root/vendor/silero-vad/$FILE"
url="https://huggingface.co/ggml-org/whisper-vad/resolve/$COMMIT/$FILE"

check=0
if [ "${1:-}" = "--check" ]; then
    check=1
fi

if ! command -v curl >/dev/null 2>&1; then
    # A check passing without its tool is no check: skip locally, fail on CI
    # (GitHub sets CI).
    if [ "$check" = 1 ] && [ -z "${CI:-}" ]; then
        echo "fetch-silero-vad: SKIP curl not found"
        exit 0
    fi
    echo "fetch-silero-vad: FAIL curl not found" >&2
    exit 1
fi

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

# -L follows redirect to Hugging Face's store; both --proto flags keep hops on HTTPS.
if ! curl -sSfL --proto '=https' --proto-redir '=https' "$url" -o "$workdir/$FILE"; then
    echo "fetch-silero-vad: FAIL could not fetch $url" >&2
    exit 1
fi

echo "$SHA256  $workdir/$FILE" | sha256sum -c - >/dev/null || {
    echo "fetch-silero-vad: FAIL checksum mismatch for $FILE at $COMMIT; refusing it" >&2
    exit 1
}

if [ "$check" = 1 ]; then
    if ! cmp -s "$workdir/$FILE" "$target"; then
        echo "fetch-silero-vad: FAIL vendor/silero-vad/$FILE is not $COMMIT's" >&2
        exit 1
    fi
    echo "fetch-silero-vad: OK vendor/silero-vad/$FILE is $COMMIT's"
    exit 0
fi

install -Dm 644 "$workdir/$FILE" "$target"
echo "Done. vendor/silero-vad/$FILE is $COMMIT's."
echo "Remember: vendor/silero-vad/SOURCE.md must describe this exact file."
