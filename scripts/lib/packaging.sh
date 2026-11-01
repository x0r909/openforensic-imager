#!/usr/bin/env bash

# shellcheck disable=SC2034
ofi_repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
ofi_version=${OFI_VERSION:-0.1.0}
ofi_source_date_epoch=${SOURCE_DATE_EPOCH:-1793318400}
ofi_output_dir=${OFI_OUTPUT_DIR:-"$ofi_repo_root/packaging/output"}
ofi_build_dir=${OFI_BUILD_DIR:-"$ofi_repo_root/build/package-release"}

# --- Architecture -----------------------------------------------------------
#
# Every artifact this repository produces used to name its architecture
# literally: `..._amd64.deb`, `...-x86_64.AppImage`, `flatpak --arch=x86_64`,
# `/usr/lib/x86_64-linux-gnu/qt6/...`, and `Architecture: amd64` in the DEB
# control file. On an arm64 host that produces an x86_64-labelled artifact that
# cannot install, and the suite never noticed because the only host it ran on
# was x86_64. These four helpers are the single source for the names, and
# `packaging/tests/test_packaging_architecture.sh` pins them.
#
# `DEB_HOST_ARCH` overrides the native architecture. That is not a convenience:
# the only way to test the arm64 packaging path on the x86_64 host this project
# is developed on is to build it in an emulated guest, and inside that guest
# `dpkg-architecture` answers `arm64` on its own — the override is what lets the
# same scripts be reasoned about from a host of either architecture.

# The Debian architecture name (`amd64`, `arm64`). Validated through
# `dpkg-architecture -a`, because a plain query echoes back whatever
# `DEB_HOST_ARCH` was set to and would accept a misspelling, silently
# mislabelling the artifact.
ofi_deb_arch() {
    local arch=${DEB_HOST_ARCH:-$(dpkg-architecture -qDEB_HOST_ARCH)}
    dpkg-architecture -a "$arch" -qDEB_HOST_ARCH || return 1
}

# The GNU multiarch tuple (`x86_64-linux-gnu`, `aarch64-linux-gnu`), which is
# the directory Qt and the loaders use. Derived through `-a` rather than from
# the environment: `DEB_HOST_ARCH=arm64` alone leaves `dpkg-architecture
# -qDEB_HOST_MULTIARCH` answering `x86_64-linux-gnu`, which is the trap this
# helper exists to close.
ofi_multiarch() {
    dpkg-architecture -a "$(ofi_deb_arch)" -qDEB_HOST_MULTIARCH 2>/dev/null
}

# The vendor spelling used by AppImage, Flatpak, and prebuilt AppImage tooling.
ofi_appimage_arch() {
    case "$(ofi_deb_arch)" in
        amd64) printf 'x86_64' ;;
        arm64) printf 'aarch64' ;;
        *)
            printf 'no AppImage architecture for DEB_HOST_ARCH=%s\n' \
                "$(ofi_deb_arch)" >&2
            return 1
            ;;
    esac
}

ofi_flatpak_arch() {
    case "$(ofi_deb_arch)" in
        amd64) printf 'x86_64' ;;
        arm64) printf 'aarch64' ;;
        *)
            printf 'no Flatpak architecture for DEB_HOST_ARCH=%s\n' \
                "$(ofi_deb_arch)" >&2
            return 1
            ;;
    esac
}

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

# A Flatpak bundle is not byte-reproducible by design: `flatpak build-export`
# records per-object timestamps and `build-bundle` carries a summary with an
# mtime, so comparing bundle SHA-256 across two builds fails for reasons that
# have nothing to do with the build. What *is* comparable is the app content,
# and it is the part that matters: the `metadata` file `build-finish` writes
# (permissions, finish args, command) and the staged `files/` tree. Print
# `<count> <metadata-sha256> <files-sha256>` for one line of comparison.
ofi_flatpak_content_manifest() {
    local of_root=$1
    local metadata_hash
    local files_hash
    local file_count

    if [[ ! -f "$of_root/metadata" || ! -d "$of_root/files" ]]; then
        printf 'flatpak build tree is incomplete: %s\n' "$of_root" >&2
        return 1
    fi

    metadata_hash=$(sha256sum "$of_root/metadata" | awk '{ print $1 }')
    file_count=$(find "$of_root/files" \( -type f -o -type l \) | wc -l \
        | tr -d ' ')
    # Symlinks are recorded as their target, not as the content of whatever
    # they point at: `build-finish` and the SDK's staging create symlinks whose
    # targets are what a rebuild has to reproduce.
    files_hash=$(
        cd "$of_root/files" || exit 1
        {
            while IFS= read -r -d '' path; do
                printf 'FILE %s %s\n' "$path" \
                    "$(sha256sum "$path" | awk '{ print $1 }')"
            done < <(find . -type f -print0 | LC_ALL=C sort -z)
            while IFS= read -r -d '' path; do
                printf 'LINK %s %s\n' "$path" "$(readlink "$path")"
            done < <(find . -type l -print0 | LC_ALL=C sort -z)
        } | sha256sum | awk '{ print $1 }'
    )

    printf '%s %s %s\n' "$file_count" "$metadata_hash" "$files_hash"
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
    local arch
    declare -A packages=()

    # The version is asked for the architecture being built, not `:amd64`. On
    # an arm64 build the host may hold no arm64 packages at all — the guest
    # that built it did — so a native or `:amd64` query would silently drop
    # every dependency and ship a DEB with an empty `Depends`.
    arch=$(ofi_deb_arch)

    add_package() {
        local package=$1
        if [[ -z "$package" ]]; then
            return
        fi
        local version
        version=$(dpkg-query -W -f='${Version}' "$package:$arch" 2>/dev/null || true)
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
