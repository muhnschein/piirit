#!/bin/sh
# Fetches upstream deltachat-rpc-server binaries for Piirit's archs, to where
# rpm/harbour-piirit.spec expects:
# vendor/deltachat-rpc-server/<sailfish-arch>/deltachat-rpc-server.
#
# Upstream (chatmail/core) links static against musl ("to avoid glibc version
# incompatibility", per its CI): needs only the kernel. Runs on Sailfish as is.
#
# Source: PyPI wheels, holding the same binaries upstream attaches to its GitHub
# release (both nix `result/bin/deltachat-rpc-server`, per upstream
# .github/workflows/deltachat-rpc-server.yml). PyPI: plain HTTPS, wheel is a zip.
# Needs curl, unzip, sha256sum.
#
#     scripts/fetch-rpc-server.sh [--check]
#
# --check: fetch nothing; fail unless each binary present matches its pin
# (build-in-container.sh runs it every build).
#
# All pinned: bump VERSION and every checksum together (fetch new wheels, run
# `sha256sum`, update below), then vendor/deltachat-rpc-server/SOURCE.md: its
# MPL-2.0 source notice must describe the binaries actually bundled.

set -eu

check_only=0
[ "${1:-}" = --check ] && check_only=1

VERSION="2.66.0"

# sailfish-arch  upstream-arch  wheel-tag                                                                     wheel-sha256                                                       binary-sha256
TABLE="
aarch64 aarch64 py3-none-manylinux_2_17_aarch64.manylinux2014_aarch64.musllinux_1_1_aarch64 d372c6a828aba34f71f1ed8f0c439c0dfc49f8c5d756fd273c69c4e65a4d96fd 323981e007ec4545c8e7ce277b7f96b9db54991dda4fc636c41c5c74f353f6a0
armv7hl armv7l py3-none-linux_armv7l.manylinux_2_17_armv7l.manylinux2014_armv7l.musllinux_1_1_armv7l a480ef0a6a0f8d17a92652e98533206bc2a176843919fd679145bb824ee6d5e6 a000bb09673ac8642178f7ac162c29dd6042cef97b8790ea98bbd5dd7df52a6d
x86_64 x86_64 py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.musllinux_1_1_x86_64 a6e34a825eea4f66a3b5252cb0232f4adc62337c209dccba9e0c863466657db8 186c4e72386f05eed8ba36655008037ae1d8b3ceb156969cb5dc921cc1649eec
"

repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
vendor_dir="$repo_root/vendor/deltachat-rpc-server"
workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

fetch_one() {
    sfos_arch="$1"; upstream_arch="$2"; wheel_tag="$3"; wheel_sha="$4"; bin_sha="$5"

    wheel="deltachat_rpc_server-$VERSION-$wheel_tag.whl"

    echo ">> $sfos_arch (upstream $upstream_arch)"
    # PyPI's by-name file URL (redirects to the hashed one). Exact file name and
    # the sha256 check below keep it honest, whatever answers.
    curl -sSfL --proto '=https' --proto-redir '=https' --retry 3 -o "$workdir/$wheel" \
        "https://files.pythonhosted.org/packages/py3/d/deltachat_rpc_server/$wheel"

    echo "$wheel_sha  $workdir/$wheel" | sha256sum -c - >/dev/null || {
        echo "fetch-rpc-server: checksum mismatch for $wheel; refusing to install it" >&2
        exit 1
    }

    unzip -oq "$workdir/$wheel" -d "$workdir/$sfos_arch"
    bin="$workdir/$sfos_arch/deltachat_rpc_server/deltachat-rpc-server"
    echo "$bin_sha  $bin" | sha256sum -c - >/dev/null || {
        echo "fetch-rpc-server: checksum mismatch for the $sfos_arch binary inside $wheel; refusing to install it" >&2
        exit 1
    }

    install -Dm 755 "$bin" "$vendor_dir/$sfos_arch/deltachat-rpc-server"
    echo "   -> vendor/deltachat-rpc-server/$sfos_arch/deltachat-rpc-server"
}

# Present binaries only: a build needs its own arch, and checks that itself.
check_one() {
    sfos_arch="$1"; bin_sha="$2"
    bin="$vendor_dir/$sfos_arch/deltachat-rpc-server"
    [ -f "$bin" ] || return 0
    echo "$bin_sha  $bin" | sha256sum -c - >/dev/null 2>&1 || {
        echo "fetch-rpc-server: vendor/deltachat-rpc-server/$sfos_arch/deltachat-rpc-server" \
             "is not the pinned v$VERSION binary; run scripts/fetch-rpc-server.sh" >&2
        exit 1
    }
}

if [ "$check_only" = 1 ]; then
    echo "$TABLE" | while read -r sfos_arch _ _ _ bin_sha; do
        [ -n "$sfos_arch" ] || continue
        check_one "$sfos_arch" "$bin_sha"
    done
    echo "fetch-rpc-server: bundled binaries match the v$VERSION pins"
    exit 0
fi

echo "$TABLE" | while read -r sfos_arch upstream_arch wheel_tag wheel_sha bin_sha; do
    [ -n "$sfos_arch" ] || continue
    fetch_one "$sfos_arch" "$upstream_arch" "$wheel_tag" "$wheel_sha" "$bin_sha"
done

echo "Done. Binaries are deltachat-rpc-server v$VERSION, statically linked (musl)."
echo "Remember: vendor/deltachat-rpc-server/SOURCE.md must describe this exact version."
