#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
appimage=${1:-}
if [[ -z "$appimage" ]]; then
    appimage=$(find "$repo_root/packaging/output" -maxdepth 1 \
        -name 'OpenForensic-Imager-*-x86_64.AppImage' -print -quit)
fi
if [[ ! -f "$appimage" ]]; then
    printf 'AppImage artifact not found\n' >&2
    exit 1
fi
appimage=$(realpath "$appimage")

APPIMAGE_EXTRACT_AND_RUN=1 QT_QPA_PLATFORM=offscreen \
    timeout 30 "$appimage" --self-test

extract_root=$(mktemp -d)
trap 'rm -rf "$extract_root"' EXIT
(
    cd "$extract_root"
    "$appimage" --appimage-extract >/dev/null
)

grep -q '^X-AppImage-Version=0.1.0' \
    "$extract_root/squashfs-root/io.github.x0r909.OpenForensicImager.desktop"
test -x "$extract_root/squashfs-root/AppRun"
test -f "$extract_root/squashfs-root/.DirIcon"
test -f "$extract_root/squashfs-root/usr/lib/openforensic-imager/libnative_bindings.so"
test -x "$extract_root/squashfs-root/usr/bin/ofi-parser-worker"
test -f "$extract_root/squashfs-root/usr/plugins/platforms/libqoffscreen.so"
test -d "$extract_root/squashfs-root/usr/qml/QtQuick"
test -f "$extract_root/squashfs-root/usr/share/doc/openforensic-imager/THIRD_PARTY.md"
truncate -s 4096 "$extract_root/evidence.raw"
worker_response=$(
    printf '{"operation":"image_identify","path":"%s"}\n' \
        "$extract_root/evidence.raw" \
        | LD_LIBRARY_PATH="$extract_root/squashfs-root/usr/lib:$extract_root/squashfs-root/usr/lib/openforensic-imager" \
            "$extract_root/squashfs-root/usr/bin/ofi-parser-worker"
)
printf '%s' "$worker_response" \
    | jq -e '.ok == true and .outcome.result.format == "raw"' >/dev/null

printf 'AppImage artifact passed: %s\n' "$appimage"
