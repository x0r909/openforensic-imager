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
    # Debian policy 12.7 wants the Debian changelog at
    # changelog.Debian.gz; the name changelog.gz is reserved for an upstream
    # changelog, and a file there without one is a lintian error.
    install -Dm644 "$ofi_repo_root/packaging/debian/changelog" \
        "$doc_dir/changelog.Debian"
    gzip -9n -f "$doc_dir/changelog.Debian"

    # A GPL build must ship the license and third-party notices with every
    # artifact that this helper stages. The full GPL text goes to
    # LICENSE.GPL-3.0; /usr/share/doc/<pkg>/copyright must keep the
    # machine-readable DEP-5 text that `cmake --install` put there, which
    # references the system copy at /usr/share/common-licenses/GPL-3 and
    # therefore must not also contain the license verbatim.
    install -Dm644 "$ofi_repo_root/LICENSE" "$doc_dir/LICENSE.GPL-3.0"
    install -Dm644 "$ofi_repo_root/packaging/common/THIRD_PARTY.md" \
        "$doc_dir/THIRD_PARTY.md"

    # The man pages point at SECURITY_MODEL.md, so it ships beside them. The
    # Phase 6, 8, and 9 review documents do not: they cite evidence paths and
    # host-specific measurements that do not exist on an installed system.
    install -Dm644 "$ofi_repo_root/SECURITY_MODEL.md" \
        "$doc_dir/SECURITY_MODEL.md"

    # Debian policy 12.1: manual pages are installed compressed, with gzip -9n.
    local page
    for page in openforensic-imager.8 ofi-parser-worker.8; do
        install -Dm644 "$ofi_repo_root/docs/man/$page" \
            "$stage/usr/share/man/man8/$page"
        gzip -9n -f "$stage/usr/share/man/man8/$page"
    done

    ofi_normalize_stage_modes "$stage"
}

# The staging tree is assembled with `cmake --install` and `install`, both of
# which apply the builder's umask. A release built under a permissive umask
# (022 vs 002) then differs byte-for-byte from one built under a stricter one,
# because the mode is stored in the archive. Normalize every mode to what the
# Debian archive expects: directories 0755, shared libraries 0644 (policy 8.1),
# other data files 0644, executables 0755.
ofi_normalize_stage_modes() {
    local stage=$1
    local path

    while IFS= read -r -d '' path; do
        chmod 0755 "$path"
    done < <(find "$stage" -type d -print0)

    while IFS= read -r -d '' path; do
        case "$path" in
            *.so|*.so.*)
                chmod 0644 "$path"
                ;;
            *)
                if [[ -x "$path" ]]; then
                    chmod 0755 "$path"
                else
                    chmod 0644 "$path"
                fi
                ;;
        esac
    done < <(find "$stage" -type f -print0)
}

# Strip the release binaries. The Rust release profile does not strip by
# default, so every shipped executable and shared object carries symbol tables
# that lintian reports as unstripped-binary-or-object. `--strip-unneeded`
# removes what the dynamic loader does not need and keeps the dynamic symbol
# table, so the shared object still links and still exports the C ABI. It runs
# after `ofi_normalize_stage_modes` so the stripped files keep their mode.
ofi_strip_stage() {
    local stage=$1
    local path

    # `file -b` answers "ELF" for the Rust binaries and the Qt shell alike, and
    # skipping anything that is not ELF keeps the AppStream XML and the desktop
    # entry below /usr/share out of this pass. ELF-ness is the test, not the
    # execute bit: it runs after mode normalization, which makes shared
    # libraries 0644 by policy.
    while IFS= read -r -d '' path; do
        if file -b "$path" | grep -q 'ELF'; then
            # .comment carries the toolchain version string and is what lintian
            # reports as binary-has-unneeded-section. .note.* is kept: the GNU
            # build-id in it is what a debug-symbol server is looked up by.
            strip --strip-unneeded --remove-section=.comment "$path"
        fi
    done < <(find "$stage/usr/bin" "$stage/usr/lib" -type f -print0 2>/dev/null)
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
