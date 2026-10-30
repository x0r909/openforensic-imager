#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

export OFI_VERSION=${OFI_VERSION:-0.1.0}
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-1793318400}
export CARGO_INCREMENTAL=0

build_set() {
    local label=$1
    local output="$repo_root/packaging/build/repro-${label}/output"
    local build="$repo_root/packaging/build/repro-${label}/cmake"
    local target="$repo_root/packaging/build/repro-${label}/target"
    local build_log
    local deb_manifest
    local appimage_manifest
    local deb_hash
    local deb_size
    local appimage_hash
    local appimage_size

    cmake -E remove_directory "$output"
    cmake -E remove_directory "$build"
    cmake -E remove_directory "$target"

    build_log=$(
        set -euo pipefail
        OFI_OUTPUT_DIR="$output" \
        OFI_BUILD_DIR="$build" \
        CARGO_TARGET_DIR="$target" \
            "$repo_root/scripts/package-deb.sh"

        OFI_OUTPUT_DIR="$output" \
        OFI_BUILD_DIR="$build" \
        CARGO_TARGET_DIR="$target" \
            "$repo_root/scripts/package-appimage.sh" --skip-build
    )

    deb_manifest=$(awk '$1 == "OFI_ARTIFACT" && $2 == "DEB" { print $4, $5 }' \
        <<<"$build_log")
    appimage_manifest=$(awk \
        '$1 == "OFI_ARTIFACT" && $2 == "APPIMAGE" { print $4, $5 }' \
        <<<"$build_log")
    if [[ -z "$deb_manifest" || -z "$appimage_manifest" ]]; then
        printf 'build set %s did not report both artifact digests\n' "$label" >&2
        return 1
    fi

    read -r deb_hash deb_size <<<"$deb_manifest"
    read -r appimage_hash appimage_size <<<"$appimage_manifest"
    if [[ "$label" == "a" ]]; then
        deb_a_hash=$deb_hash
        deb_a_size=$deb_size
        appimage_a_hash=$appimage_hash
        appimage_a_size=$appimage_size
    else
        deb_b_hash=$deb_hash
        deb_b_size=$deb_size
        appimage_b_hash=$appimage_hash
        appimage_b_size=$appimage_size
    fi
    printf 'Build set %s complete\n' "$label"
}

compare_artifact() {
    local label=$1
    local left_hash=$2
    local left_size=$3
    local right_hash=$4
    local right_size=$5

    if [[ "$left_hash" != "$right_hash" || "$left_size" != "$right_size" ]]; then
        printf '%s mismatch\na: %s bytes, SHA-256 %s\nb: %s bytes, SHA-256 %s\n' \
            "$label" "$left_size" "$left_hash" "$right_size" "$right_hash" >&2
        return 1
    fi

    printf '%s size %s bytes, SHA-256 %s\n' "$label" "$left_size" "$left_hash"
}

build_set a
build_set b

compare_artifact DEB \
    "$deb_a_hash" "$deb_a_size" "$deb_b_hash" "$deb_b_size"
compare_artifact AppImage \
    "$appimage_a_hash" "$appimage_a_size" \
    "$appimage_b_hash" "$appimage_b_size"
printf 'Reproducible package comparison passed\n'
