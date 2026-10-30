#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source scripts/lib/packaging.sh

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

cmake -E make_directory "$ofi_output_dir"
if [[ "$skip_build" -eq 0 ]]; then
    ofi_prepare_release
fi

stage="$repo_root/packaging/build/deb-root"
ofi_stage_release "$stage"

app="$stage/usr/bin/openforensic-imager"
native="$stage/usr/lib/openforensic-imager/libnative_bindings.so"
if [[ ! -x "$app" || ! -f "$native" ]]; then
    printf 'release install is incomplete\n' >&2
    exit 1
fi

doc_dir="$stage/usr/share/doc/openforensic-imager"
sbom="$ofi_output_dir/openforensic-imager-${ofi_version}.cdx.json"
"$repo_root/scripts/generate-sbom.sh" \
    "$sbom" "$app" "$native" >/dev/null
cp "$sbom" "$doc_dir/sbom.cdx.json"
(
    cd "$doc_dir"
    sha256sum sbom.cdx.json >sbom.cdx.json.sha256
)

mkdir -p "$stage/DEBIAN"
depends=$(ofi_deb_dependencies "$app" "$native")
installed_size=$(du -sk "$stage" | awk '{ print $1 }')
sed \
    -e "s|@VERSION@|$ofi_version|g" \
    -e "s|@DEPENDS@|$depends|g" \
    -e "s|@INSTALLED_SIZE@|$installed_size|g" \
    packaging/debian/control.in >"$stage/DEBIAN/control"

(
    cd "$stage"
    find usr -type f -print0 | sort -z | xargs -0 md5sum >DEBIAN/md5sums
)

artifact="$ofi_output_dir/openforensic-imager_${ofi_version}_amd64.deb"
cmake -E remove -f "$artifact"
SOURCE_DATE_EPOCH="$ofi_source_date_epoch" \
    dpkg-deb --root-owner-group --uniform-compression --compression=xz \
    --build "$stage" "$artifact" >/dev/null

artifact_hash=$(sha256sum "$artifact" | awk '{ print $1 }')
artifact_size=$(stat -c '%s' "$artifact")
(
    cd "$ofi_output_dir"
    printf '%s  %s\n' "$artifact_hash" "$(basename "$artifact")" \
        >"$(basename "$artifact").sha256"
)

packaging/tests/test_deb_artifact.sh "$artifact"
printf '%s\n' "$artifact"
printf 'OFI_ARTIFACT DEB %s %s %s\n' \
    "$(basename "$artifact")" "$artifact_hash" "$artifact_size"
