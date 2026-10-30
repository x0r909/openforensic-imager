#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

required_files=(
    packaging/common/openforensic-imager.desktop
    packaging/common/openforensic-imager.metainfo.xml
    packaging/common/openforensic-imager.svg
    packaging/common/THIRD_PARTY.md
    packaging/debian/control.in
    packaging/debian/copyright
    packaging/flatpak/finish-args.txt
    packaging/tools.lock
    docs/packaging/INSTALL.md
    docs/packaging/REPRODUCIBLE_BUILD.md
    docs/testing/phase7-results.md
    scripts/generate-sbom.sh
    scripts/package-appimage.sh
    scripts/package-deb.sh
    scripts/package-flatpak.sh
    scripts/test-phase7.sh
    scripts/verify-reproducible-packages.sh
)

for path in "${required_files[@]}"; do
    if [[ ! -f "$path" ]]; then
        printf 'missing packaging file: %s\n' "$path" >&2
        exit 1
    fi
done

for script in scripts/generate-sbom.sh scripts/package-appimage.sh \
    scripts/package-deb.sh scripts/package-flatpak.sh scripts/test-phase7.sh \
    scripts/verify-reproducible-packages.sh; do
    if [[ ! -x "$script" ]]; then
        printf 'packaging script is not executable: %s\n' "$script" >&2
        exit 1
    fi
    bash -n "$script"
done

desktop-file-validate packaging/common/openforensic-imager.desktop
appstreamcli validate --no-net --strict \
    packaging/common/openforensic-imager.metainfo.xml

grep -q '^Exec=openforensic-imager %F$' \
    packaging/common/openforensic-imager.desktop
grep -q '^Icon=io.github.x0r909.OpenForensicImager$' \
    packaging/common/openforensic-imager.desktop
grep -q 'Analysis-only Flatpak build' packaging/common/THIRD_PARTY.md
grep -q 'appimagetool-x86_64.AppImage' packaging/tools.lock
grep -q 'SHA256=' packaging/tools.lock
grep -q 'LINKER:--build-id=none' apps/desktop/CMakeLists.txt
grep -q 'Physical acquisition with this build is' docs/packaging/INSTALL.md
grep -qi 'digest and size comparison' docs/packaging/REPRODUCIBLE_BUILD.md
grep -q 'Reproducible package comparison passed' docs/testing/phase7-results.md

printf 'Phase 7 packaging layout passed\n'
