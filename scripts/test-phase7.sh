#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

"$repo_root/packaging/tests/test_phase7_layout.sh"

deb=$(find packaging/output -maxdepth 1 \
    -name 'openforensic-imager_*_amd64.deb' -print -quit)
appimage=$(find packaging/output -maxdepth 1 \
    -name 'OpenForensic-Imager-*-x86_64.AppImage' -print -quit)

if [[ -z "$deb" || -z "$appimage" ]]; then
    printf 'Build DEB and AppImage before running artifact tests\n' >&2
    exit 1
fi

"$repo_root/packaging/tests/test_deb_artifact.sh" "$deb"
"$repo_root/packaging/tests/test_appimage_artifact.sh" "$appimage"

if [[ -d packaging/build/flatpak-app/files ]]; then
    flatpak_bundle=$(find packaging/output -maxdepth 1 \
        -name 'openforensic-imager-*-x86_64.flatpak' -print -quit)
    "$repo_root/packaging/tests/test_flatpak_build.sh" \
        packaging/build/flatpak-app "$flatpak_bundle"
fi

jq -e '.bomFormat == "CycloneDX" and .specVersion == "1.6"' \
    packaging/output/openforensic-imager-0.1.0.cdx.json >/dev/null
test -s packaging/output/openforensic-imager-0.1.0.cdx.json.sha256
test -s packaging/output/openforensic-imager_0.1.0_amd64.deb.sha256
test -s packaging/output/OpenForensic-Imager-0.1.0-x86_64.AppImage.sha256

printf 'Phase 7 packaging tests passed\n'
