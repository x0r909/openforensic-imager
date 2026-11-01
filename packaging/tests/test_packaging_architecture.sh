#!/usr/bin/env bash
set -euo pipefail

# The packaging scripts must derive their architecture from the build host (or
# from DEB_HOST_ARCH) rather than name one. Before this test the DEB was always
# `..._amd64.deb`, the AppImage `...-x86_64.AppImage`, the Flatpak
# `--arch=x86_64`, the Qt plugin directories `/usr/lib/x86_64-linux-gnu/...`,
# and `control.in` said `Architecture: amd64` — so an arm64 build produced an
# x86_64-labelled artifact that could not install, and no test noticed because
# this host is x86_64 and the suite only ever built natively.
#
# The test is behavioural where it can be: it asks the shared helper functions
# what architecture they report, under both a native and an overridden
# architecture, and only falls back to asserting on the scripts' text for the
# literal that must not reappear.

trap 'printf "assertion failed: %s:%s\n" "${BASH_SOURCE[0]}" "$LINENO" >&2' ERR

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

# --- The helper reports the host architecture, and honours an override -------

# `source` in a subshell so each architecture is asked for cleanly; the helper
# reads DEB_HOST_ARCH at call time, not at source time. A missing helper is
# reported as an empty string, which the equality assertions below turn into a
# named failure.
arch_value() {
    local helper=$1
    local deb_arch=$2
    (
        # shellcheck disable=SC1091
        source scripts/lib/packaging.sh
        if [[ -n "$deb_arch" ]]; then
            DEB_HOST_ARCH=$deb_arch
            export DEB_HOST_ARCH
        fi
        if declare -F "$helper" >/dev/null; then
            "$helper"
        fi
    )
}

assert_eq() {
    local expected=$1
    local actual=$2
    local what=$3
    if [[ "$expected" != "$actual" ]]; then
        printf '%s: expected %q, got %q\n' "$what" "$expected" "$actual" >&2
        exit 1
    fi
}

host_deb_arch=$(dpkg-architecture -qDEB_HOST_ARCH)
case "$host_deb_arch" in
    amd64) host_appimage_arch=x86_64; host_flatpak_arch=x86_64 ;;
    arm64) host_appimage_arch=aarch64; host_flatpak_arch=aarch64 ;;
    *) printf 'unsupported test host architecture: %s\n' "$host_deb_arch" >&2; exit 1 ;;
esac

assert_eq "$host_deb_arch" "$(arch_value ofi_deb_arch '')" \
    'ofi_deb_arch on the native host'
assert_eq "$host_appimage_arch" "$(arch_value ofi_appimage_arch '')" \
    'ofi_appimage_arch on the native host'
assert_eq "$host_flatpak_arch" "$(arch_value ofi_flatpak_arch '')" \
    'ofi_flatpak_arch on the native host'
assert_eq "$(dpkg-architecture -qDEB_HOST_MULTIARCH)" \
    "$(arch_value ofi_multiarch '')" 'ofi_multiarch on the native host'

# The override is the only mechanism that works on a host with a single
# architecture installed, which is every host this project is built on. Without
# it, C5's arm64 build could only be tested on arm64 hardware, which is the
# hardware the guest is emulating precisely because none is available.
assert_eq arm64 "$(arch_value ofi_deb_arch arm64)" 'ofi_deb_arch under DEB_HOST_ARCH=arm64'
assert_eq aarch64 "$(arch_value ofi_appimage_arch arm64)" \
    'ofi_appimage_arch under DEB_HOST_ARCH=arm64'
assert_eq aarch64 "$(arch_value ofi_flatpak_arch arm64)" \
    'ofi_flatpak_arch under DEB_HOST_ARCH=arm64'
assert_eq aarch64-linux-gnu "$(arch_value ofi_multiarch arm64)" \
    'ofi_multiarch under DEB_HOST_ARCH=arm64'
assert_eq x86_64 "$(arch_value ofi_appimage_arch amd64)" \
    'ofi_appimage_arch under DEB_HOST_ARCH=amd64'

# An architecture dpkg does not know must fail loudly rather than fall back to
# the host's, which would silently mislabel the artifact.
if ( source scripts/lib/packaging.sh
     DEB_HOST_ARCH=not-a-real-arch ofi_deb_arch >/dev/null 2>&1 ); then
    printf 'an unknown DEB_HOST_ARCH was accepted\n' >&2
    exit 1
fi

# --- No architecture literal survives where it was hardcoded ---------------

# `control.in` takes the architecture as a substitution; the three packaging
# scripts take it from the helpers. A literal in any of them is the defect this
# test exists for.
for file in scripts/package-deb.sh scripts/package-appimage.sh \
    scripts/package-flatpak.sh; do
    if grep -nE '(^|[^_a-z-])(amd64|x86_64|aarch64|arm64)([^_a-z-]|$)' "$file"; then
        printf '%s still names an architecture literally\n' "$file" >&2
        exit 1
    fi
done

grep -q 'Architecture: @ARCH@' packaging/debian/control.in
if grep -qE 'Architecture:\s*(amd64|arm64)' packaging/debian/control.in; then
    printf 'control.in hardcodes an architecture\n' >&2
    exit 1
fi

# dpkg-query for the DEB's dependency versions must ask about the architecture
# being built, not `:amd64`: on an arm64 build the arm64 package's version is
# what the dependency line means, and the host may not even have the arm64
# packages installed to answer a native query.
grep -q 'ofi_deb_arch' scripts/lib/packaging.sh
grep -qE 'dpkg-query .*":\$arch"|\$package:\$arch' scripts/lib/packaging.sh
# Comments that explain the old `:amd64` query are fine; a code line that still
# makes it is not.
if grep -vE '^[[:space:]]*#' scripts/lib/packaging.sh | grep -q ':amd64'; then
    printf 'packaging.sh queries dpkg for a hardcoded :amd64 package\n' >&2
    exit 1
fi

# --- The AppImage tool is per-architecture, and both are pinned ------------

grep -q 'APPIMAGETOOL_URL_x86_64=' packaging/tools.lock
grep -q 'APPIMAGETOOL_SHA256_x86_64=' packaging/tools.lock
grep -q 'APPIMAGETOOL_URL_aarch64=' packaging/tools.lock
grep -q 'APPIMAGETOOL_SHA256_aarch64=' packaging/tools.lock
# The digest is the point of the lock file; an unpinned new entry is the defect
# it was created to prevent.
if grep -qE 'APPIMAGETOOL_(URL|SHA256)_(aarch64|x86_64)=$' packaging/tools.lock; then
    printf 'an AppImage tool entry has no value\n' >&2
    exit 1
fi

printf 'packaging architecture derivation passed\n'
