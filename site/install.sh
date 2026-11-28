#!/bin/sh
# Installs the latest prebuilt track into ${TRACK_INSTALL_DIR:-$HOME/.local/bin}.
#
#   curl -fsSL https://udoprog.github.io/track/install.sh | sh
#
# TRACK_VERSION pins a release (the dated tag, e.g. 2026-05-04) instead of the latest.
# Windows is not supported by this script; download the zip from the releases.

set -eu

repo="udoprog/track"
api="https://api.github.com/repos/$repo/releases"

say() {
    printf '%s\n' "track-install: $*"
}

die() {
    printf '%s\n' "track-install: error: $*" >&2
    exit 1
}

# Prints the <arch>-<os> pair for this machine, or fails.
detect_platform() {
    os=$(uname -s)
    machine=$(uname -m)

    case "$os" in
    Linux) os=linux ;;
    Darwin) os=macos ;;
    MINGW* | MSYS* | CYGWIN*) die "this script does not support Windows; use the zip from https://github.com/$repo/releases" ;;
    *) die "no prebuilt track for $machine-$os" ;;
    esac

    case "$machine" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "no prebuilt track for $machine-$os" ;;
    esac

    # The platforms releases are built for.
    case "$arch-$os" in
    x86_64-linux | aarch64-macos) ;;
    *) die "no prebuilt track for $arch-$os" ;;
    esac

    printf '%s\n' "$arch-$os"
}

# Reads a URL to stdout, or to the file given as the second argument.
fetch() {
    if command -v curl >/dev/null 2>&1; then
        if [ $# -gt 1 ]; then
            curl -fsSL -o "$2" "$1"
        else
            curl -fsSL "$1"
        fi
    elif command -v wget >/dev/null 2>&1; then
        if [ $# -gt 1 ]; then
            wget -q -O "$2" "$1"
        else
            wget -q -O - "$1"
        fi
    else
        die "need curl or wget"
    fi
}

# Picks the download URL ending in -<platform>.tar.gz from release JSON on stdin.
pick_asset() {
    grep -o '"browser_download_url": *"[^"]*"' |
        sed 's/^[^:]*: *"//; s/"$//' |
        grep -e "-$1\\.tar\\.gz\$" |
        head -n 1
}

# main
command -v tar >/dev/null 2>&1 || die "need tar"

platform=$(detect_platform)
dir=${TRACK_INSTALL_DIR:-$HOME/.local/bin}

if [ -n "${TRACK_VERSION:-}" ]; then
    release_url="$api/tags/$TRACK_VERSION"
else
    release_url="$api/latest"
fi

say "looking up release ${TRACK_VERSION:-latest}"
release=$(fetch "$release_url") || die "could not read $release_url"
url=$(printf '%s\n' "$release" | pick_asset "$platform") || true
[ -n "$url" ] || die "no prebuilt track for $platform in release ${TRACK_VERSION:-latest}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $url"
fetch "$url" "$tmp/track.tar.gz" || die "download failed"
tar -xzf "$tmp/track.tar.gz" -C "$tmp" track || die "archive has no track binary"

mkdir -p "$dir"
mv "$tmp/track" "$dir/track"
chmod +x "$dir/track"
say "installed $dir/track"

case ":$PATH:" in
*":$dir:"*) ;;
*) say "warning: $dir is not on your PATH; add it to your shell profile" ;;
esac
