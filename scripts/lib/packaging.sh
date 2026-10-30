#!/usr/bin/env bash

# shellcheck disable=SC2034
ofi_repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
ofi_version=${OFI_VERSION:-0.1.0}
ofi_source_date_epoch=${SOURCE_DATE_EPOCH:-1793318400}
ofi_output_dir=${OFI_OUTPUT_DIR:-"$ofi_repo_root/packaging/output"}
ofi_build_dir=${OFI_BUILD_DIR:-"$ofi_repo_root/build/package-release"}

ofi_prepare_release() {
    local cmake_args=(
        -DCMAKE_BUILD_TYPE=Release
        -DOFI_RUST_PROFILE=release
    )
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        cmake_args+=("-DOFI_RUST_TARGET_DIR=${CARGO_TARGET_DIR}/release")
    fi
    cmake -S "$ofi_repo_root" -B "$ofi_build_dir" -G Ninja \
        "${cmake_args[@]}"
    cmake --build "$ofi_build_dir"
}

ofi_stage_release() {
    local stage=$1
    cmake -E remove_directory "$stage"
    cmake -E make_directory "$stage"
    DESTDIR="$stage" cmake --install "$ofi_build_dir" --prefix /usr

    local doc_dir="$stage/usr/share/doc/openforensic-imager"
    install -Dm644 "$ofi_repo_root/packaging/debian/changelog" \
        "$doc_dir/changelog"
    gzip -9n -f "$doc_dir/changelog"
}

ofi_skip_bundled_library() {
    case "$(basename "$1")" in
        ld-linux-*|linux-vdso.so.*|libc.so.*|libdl.so.*|libm.so.*|\
        libpthread.so.*|libresolv.so.*|librt.so.*|libnss_*.so.*)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

ofi_copy_library_dependencies() {
    local binary=$1
    local destination=$2
    local libraries
    cmake -E make_directory "$destination"
    libraries=$(ldd "$binary" | awk '/=> \// { print $3 }' | sort -u)
    while IFS= read -r library; do
        if [[ -z "$library" || ! -f "$library" ]]; then
            continue
        fi
        if ofi_skip_bundled_library "$library"; then
            continue
        fi
        if [[ "$(readlink -f "$library")" == "$(readlink -f "$destination/$(basename "$library")")" ]]; then
            continue
        fi
        cp -Lf "$library" "$destination/"
    done <<<"$libraries"
}

ofi_copy_forensic_libraries() {
    local binary=$1
    local destination=$2
    local libraries
    cmake -E make_directory "$destination"
    libraries=$(ldd "$binary" | awk '/=> \// { print $3 }' | sort -u)
    while IFS= read -r library; do
        case "$(basename "$library")" in
            libtsk.so.*|libewf.so.*|libafflib.so.*|libvmdk.so.*|libvhdi.so.*)
                cp -Lf "$library" "$destination/"
                ;;
        esac
    done <<<"$libraries"
}

ofi_deb_dependencies() {
    local binary=$1
    local native_library=$2
    local libraries
    declare -A packages=()

    add_package() {
        local package=$1
        if [[ -z "$package" ]]; then
            return
        fi
        local version
        version=$(dpkg-query -W -f='${Version}' "$package:amd64" 2>/dev/null || true)
        if [[ -n "$version" ]]; then
            packages["$package"]="$version"
        fi
    }

    libraries=$(
        {
            ldd "$binary"
            ldd "$native_library"
        } | awk '/=> \// { print $3 }' | sort -u
    )
    while IFS= read -r library; do
        local package
        package=$(dpkg -S "$library" 2>/dev/null \
            | awk -F: 'NR == 1 { print $1 }' || true)
        add_package "$package"
    done <<<"$libraries"

    for package in libtsk19t64 libewf2 qt6-qpa-plugins qt6-svg-plugins \
        qml6-module-qtqml qml6-module-qtqml-models qml6-module-qtquick \
        qml6-module-qtquick-controls qml6-module-qtquick-layouts \
        qml6-module-qtquick-templates qml6-module-qtquick-window; do
        add_package "$package"
    done

    for package in "${!packages[@]}"; do
        printf '%s (>= %s)\n' "$package" "${packages[$package]}"
    done | sort | paste -sd, -
}
