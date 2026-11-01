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

if [[ "$skip_build" -eq 0 ]]; then
    ofi_prepare_release
fi

stage="$repo_root/packaging/build/flatpak-stage"
of_root="$repo_root/packaging/build/flatpak-app"
repo="$repo_root/packaging/build/flatpak-repo"
ofi_stage_release "$stage"

cmake -E make_directory "$ofi_output_dir"
cmake -E remove_directory "$of_root"
cmake -E remove_directory "$repo"
flatpak_arch=$(ofi_flatpak_arch)
flatpak build-init "--arch=$flatpak_arch" \
    "$of_root" io.github.x0r909.OpenForensicImager \
    org.kde.Sdk org.kde.Platform 6.10

cp -a "$stage/usr/." "$of_root/files/"
app="$of_root/files/bin/openforensic-imager"
mv "$app" "$of_root/files/bin/openforensic-imager.real"
install -m755 packaging/flatpak/openforensic-imager-wrapper.sh \
    "$of_root/files/bin/openforensic-imager"

# The worker is the binary that links libtsk and libewf. The application's own
# cdylib stopped carrying those DT_NEEDED entries when parsing moved into the
# worker, so following the cdylib's `ldd` finds nothing — the sandbox has no
# system copies either, and the worker cannot be handed an LD_LIBRARY_PATH
# because the desktop spawns it with a cleared environment. Both binaries are
# walked so a future change that puts a forensic dependency back in the cdylib
# is covered without editing this list.
lib_dir="$of_root/files/lib/openforensic-imager"
mkdir -p "$lib_dir"
for binary in "$of_root/files/bin/ofi-parser-worker" \
    "$lib_dir/libnative_bindings.so"; do
    ofi_copy_forensic_libraries "$binary" "$lib_dir"
done

mapfile -t finish_args <packaging/flatpak/finish-args.txt
flatpak build-finish "$of_root" --command=openforensic-imager \
    "${finish_args[@]}"

packaging/tests/test_flatpak_build.sh "$of_root"

cmake -E make_directory "$repo"
flatpak build-export "--arch=$flatpak_arch" "$repo" "$of_root" "$ofi_version"
artifact="$ofi_output_dir/openforensic-imager-${ofi_version}-${flatpak_arch}.flatpak"
cmake -E remove -f "$artifact"
flatpak build-bundle "--arch=$flatpak_arch" "$repo" "$artifact" \
    io.github.x0r909.OpenForensicImager "$ofi_version"

(
    cd "$ofi_output_dir"
    sha256sum "$(basename "$artifact")" >"$(basename "$artifact").sha256"
)

printf '%s\n' "$artifact"
# The bundle itself is not byte-reproducible (build-export records object
# timestamps), so report the comparable content instead: the metadata file and
# the staged files/ tree.
flatpak_manifest=$(ofi_flatpak_content_manifest "$of_root")
printf 'OFI_FLATPAK_CONTENT %s\n' "$flatpak_manifest"
