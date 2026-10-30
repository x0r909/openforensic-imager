#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
deb=${1:-}
if [[ -z "$deb" ]]; then
    deb=$(find "$repo_root/packaging/output" -maxdepth 1 \
        -name 'openforensic-imager_*_amd64.deb' -print -quit)
fi
if [[ ! -f "$deb" ]]; then
    printf 'DEB artifact not found\n' >&2
    exit 1
fi
deb=$(realpath "$deb")

dpkg-deb --info "$deb" >"$deb.info"
dpkg-deb --contents "$deb" >"$deb.contents"

grep -q './usr/bin/openforensic-imager' "$deb.contents"
grep -q './usr/lib/openforensic-imager/libnative_bindings.so' "$deb.contents"
grep -q './usr/share/applications/io.github.x0r909.OpenForensicImager.desktop' "$deb.contents"
grep -q './usr/share/icons/hicolor/scalable/apps/io.github.x0r909.OpenForensicImager.svg' \
    "$deb.contents"
grep -q './usr/share/metainfo/io.github.x0r909.OpenForensicImager.metainfo.xml' \
    "$deb.contents"
grep -q 'Depends:.*libtsk' "$deb.info"
grep -q 'Depends:.*libewf' "$deb.info"
grep -q 'Depends:.*libqt6' "$deb.info"

extract_root=$(mktemp -d)
trap 'rm -rf "$extract_root"' EXIT
dpkg-deb --extract "$deb" "$extract_root"

readelf -d "$extract_root/usr/bin/openforensic-imager" \
    | grep -Fq "\$ORIGIN/../lib/openforensic-imager"

env -u LD_LIBRARY_PATH QT_QPA_PLATFORM=offscreen \
    timeout 20 "$extract_root/usr/bin/openforensic-imager" --self-test

desktop-file-validate \
    "$extract_root/usr/share/applications/io.github.x0r909.OpenForensicImager.desktop"

printf 'DEB artifact passed: %s\n' "$deb"
