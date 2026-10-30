#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
build_root=${1:-"$repo_root/packaging/build/flatpak-app"}
bundle=${2:-}

if [[ ! -d "$build_root/files" ]]; then
    printf 'Flatpak app build not found: %s\n' "$build_root" >&2
    exit 1
fi

test -x "$build_root/files/bin/openforensic-imager"
test -f "$build_root/files/lib/openforensic-imager/libnative_bindings.so"
test -f "$build_root/files/share/applications/io.github.x0r909.OpenForensicImager.desktop"
test -f "$build_root/files/share/metainfo/io.github.x0r909.OpenForensicImager.metainfo.xml"

flatpak build "$build_root" \
    env QT_QPA_PLATFORM=offscreen \
    /app/bin/openforensic-imager --self-test

flatpak build --readonly "$build_root" sh -c \
    'test ! -e /dev/sda && test ! -e /dev/nvme0n1'

if [[ -n "$bundle" && -f "$bundle" ]]; then
    temporary_data=$(mktemp -d)
    XDG_DATA_HOME="$temporary_data" \
        flatpak --user install --noninteractive --no-deps "$bundle"
    XDG_DATA_HOME="$temporary_data" \
        flatpak run --env=QT_QPA_PLATFORM=offscreen \
        io.github.x0r909.OpenForensicImager --self-test
    cmake -E remove_directory "$temporary_data"
fi

printf 'Flatpak analysis build passed: %s\n' "$build_root"
