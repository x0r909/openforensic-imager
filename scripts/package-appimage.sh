#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source scripts/lib/packaging.sh
# shellcheck source=packaging/tools.lock
source packaging/tools.lock

cmake -E make_directory "$ofi_output_dir"

usage() {
    printf 'Usage: %s [--skip-build]\n' "$(basename "$0")"
}

skip_build=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build)
            skip_build=1
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            printf 'unknown option: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [[ "$skip_build" -eq 0 ]]; then
    ofi_prepare_release
fi

stage="$repo_root/packaging/build/appimage-stage"
appdir="$repo_root/packaging/build/OpenForensic-Imager.AppDir"
ofi_stage_release "$stage"
cmake -E remove_directory "$appdir"
cmake -E make_directory "$appdir"
cp -a "$stage/usr" "$appdir/usr"

app="$appdir/usr/bin/openforensic-imager"
native="$appdir/usr/lib/openforensic-imager/libnative_bindings.so"
if [[ ! -x "$app" || ! -f "$native" ]]; then
    printf 'recorded release tree is incomplete\n' >&2
    exit 1
fi

install -m755 packaging/appimage/AppRun "$appdir/AppRun"
install -m644 packaging/common/openforensic-imager.desktop \
    "$appdir/io.github.x0r909.OpenForensicImager.desktop"
convert -background none \
    -define png:exclude-chunks=date,time \
    packaging/common/openforensic-imager.svg \
    -resize 512x512 "$appdir/io.github.x0r909.OpenForensicImager.png"
cp "$appdir/io.github.x0r909.OpenForensicImager.png" "$appdir/.DirIcon"

qt_plugins=/usr/lib/x86_64-linux-gnu/qt6/plugins
qt_qml=/usr/lib/x86_64-linux-gnu/qt6/qml
cmake -E make_directory "$appdir/usr/plugins"
for plugin_dir in platforms imageformats iconengines tls platformthemes \
    xcbglintegrations; do
    if [[ -d "$qt_plugins/$plugin_dir" ]]; then
        cp -a "$qt_plugins/$plugin_dir" "$appdir/usr/plugins/"
    fi
done
cp -a "$qt_qml" "$appdir/usr/qml"

binaries=$(
    {
        printf '%s\n' "$app" "$native"
        find "$appdir/usr/plugins" "$appdir/usr/qml" -type f -name '*.so'
    } | sort -u
)
while IFS= read -r elf; do
    ofi_copy_library_dependencies "$elf" "$appdir/usr/lib"
done <<<"$binaries"

sbom="$ofi_output_dir/openforensic-imager-${ofi_version}.cdx.json"
"$repo_root/scripts/generate-sbom.sh" \
    "$sbom" "$app" "$native" >/dev/null
cp "$sbom" \
    "$appdir/usr/share/doc/openforensic-imager/sbom.cdx.json"

cache_dir="$repo_root/packaging/cache"
tool="$cache_dir/$APPIMAGETOOL_NAME"
cmake -E make_directory "$cache_dir"
if [[ ! -f "$tool" ]] \
    || [[ "$(sha256sum "$tool" | awk '{ print $1 }')" != "$APPIMAGETOOL_SHA256" ]]; then
    cmake -E remove -f "$tool" "$tool.part"
    curl --fail --location --silent --show-error \
        "$APPIMAGETOOL_URL" --output "$tool.part"
    actual_hash=$(sha256sum "$tool.part" | awk '{ print $1 }')
    if [[ "$actual_hash" != "$APPIMAGETOOL_SHA256" ]]; then
        printf 'appimagetool checksum mismatch: %s\n' "$actual_hash" >&2
        exit 1
    fi
    mv "$tool.part" "$tool"
fi
chmod 0755 "$tool"

artifact="$ofi_output_dir/OpenForensic-Imager-${ofi_version}-x86_64.AppImage"
cmake -E remove -f "$artifact"
ARCH=x86_64 \
SOURCE_DATE_EPOCH="$ofi_source_date_epoch" \
APPIMAGE_EXTRACT_AND_RUN=1 \
    "$tool" "$appdir" "$artifact" >"$repo_root/packaging/build/appimagetool.log" 2>&1
chmod 0755 "$artifact"

artifact_hash=$(sha256sum "$artifact" | awk '{ print $1 }')
artifact_size=$(stat -c '%s' "$artifact")
(
    cd "$ofi_output_dir"
    printf '%s  %s\n' "$artifact_hash" "$(basename "$artifact")" \
        >"$(basename "$artifact").sha256"
)

packaging/tests/test_appimage_artifact.sh "$artifact"
printf '%s\n' "$artifact"
printf 'OFI_ARTIFACT APPIMAGE %s %s %s\n' \
    "$(basename "$artifact")" "$artifact_hash" "$artifact_size"
