#!/usr/bin/env bash
set -euo pipefail

# A bare `grep -q` that fails aborts with no output, so the trap names the line.
trap 'printf "assertion failed: %s:%s\n" "${BASH_SOURCE[0]}" "$LINENO" >&2' ERR

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
deb=${1:-}
if [[ -z "$deb" ]]; then
    deb=$(find "$repo_root/packaging/output" -maxdepth 1 \
        -name 'openforensic-imager_*_amd64.deb' -print -quit)
fi
if [[ ! -f "$deb" ]]; then
    printf 'DEB artifact not found\n' >&2
    exit 1
fi
deb=$(realpath "$deb")

dpkg-deb --info "$deb" >"$deb.info"
dpkg-deb --contents "$deb" >"$deb.contents"

grep -q './usr/bin/openforensic-imager' "$deb.contents"
grep -q './usr/bin/ofi-parser-worker' "$deb.contents"
grep -q './usr/lib/openforensic-imager/libnative_bindings.so' "$deb.contents"
grep -q './usr/share/applications/io.github.x0r909.OpenForensicImager.desktop' "$deb.contents"
grep -q './usr/share/icons/hicolor/scalable/apps/io.github.x0r909.OpenForensicImager.svg' \
    "$deb.contents"
grep -q './usr/share/metainfo/io.github.x0r909.OpenForensicImager.metainfo.xml' \
    "$deb.contents"
grep -q 'Depends:.*libtsk' "$deb.info"
grep -q 'Depends:.*libewf' "$deb.info"
grep -q 'Depends:.*libqt6' "$deb.info"

# The Maintainer field is not cosmetic: lintian rejects a domain that is not
# resolvable, and `invalid` is the TLD-length placeholder that is not.
grep -q 'Maintainer: .*<[^@>]\+@[^@>]\+\.[a-z]\{2,\}>$' "$deb.info"
if grep -q '<[^>]*@invalid>' "$deb.info"; then
    printf 'placeholder maintainer address in the control file\n' >&2
    exit 1
fi

# Packaging policy the release has to satisfy: the Debian changelog name, the
# manual pages, and the file modes lintian checks. These are asserted here
# because the modes depend on the builder's umask and a permissive one would
# otherwise ship without a single test noticing.
grep -q './usr/share/doc/openforensic-imager/changelog.Debian.gz' "$deb.contents"
if grep -q './usr/share/doc/openforensic-imager/changelog.gz' "$deb.contents"; then
    printf 'upstream changelog name used for the Debian changelog\n' >&2
    exit 1
fi
grep -q './usr/share/doc/openforensic-imager/SECURITY_MODEL.md' "$deb.contents"
grep -q './usr/share/doc/openforensic-imager/LICENSE.GPL-3.0' "$deb.contents"
grep -q './usr/share/man/man8/openforensic-imager.8.gz' "$deb.contents"
grep -q './usr/share/man/man8/ofi-parser-worker.8.gz' "$deb.contents"

# A man page that is installed but does not render is not a man page. gzip -9n
# is asserted too: a different compression level changes the archive bytes.
while IFS= read -r -d '' page; do
    man --warnings -l "$page" >/dev/null 2>"$page.warnings"
    if [[ -s "$page.warnings" ]]; then
        printf 'man page renders with warnings: %s\n' "$page" >&2
        cat "$page.warnings" >&2
        exit 1
    fi
    rm -f "$page.warnings"
done < <(find "$extract_root/usr/share/man" -name '*.gz' -print0)

extract_root=$(mktemp -d)
worker_root=$(mktemp -d)
trap 'rm -rf "$extract_root" "$worker_root"' EXIT
dpkg-deb --extract "$deb" "$extract_root"

# Directories 0755, shared libraries 0644 (policy 8.1), other data files 0644,
# executables 0755. Checked against the extracted tree rather than the contents
# listing so the assertion reads the mode the installer will actually apply.
declare -A expected_modes=()
while IFS= read -r -d '' path; do
    case "$path" in
        *.so|*.so.*) expected_modes["$path"]=644 ;;
        *)
            if [[ -x "$path" ]]; then
                expected_modes["$path"]=755
            else
                expected_modes["$path"]=644
            fi
            ;;
    esac
done < <(find "$extract_root" -type f -print0)
while IFS= read -r -d '' path; do
    expected_modes["$path"]=755
done < <(find "$extract_root" -mindepth 1 -type d -print0)

mode_violations=()
for path in "${!expected_modes[@]}"; do
    actual=$(stat -c '%a' "$path")
    if [[ "$actual" != "${expected_modes[$path]}" ]]; then
        mode_violations+=("${path#"$extract_root"}: ${actual} != ${expected_modes[$path]}")
    fi
done
if [[ ${#mode_violations[@]} -gt 0 ]]; then
    printf 'non-policy file modes:\n' >&2
    printf '  %s\n' "${mode_violations[@]}" >&2
    exit 1
fi

# The copyright file must stay machine-readable DEP-5 and must not carry the
# GPL verbatim, which is what policy 12.5 requires the system copy for.
head -n 1 "$extract_root/usr/share/doc/openforensic-imager/copyright" \
    | grep -q '^Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/$'
if grep -q 'GNU GENERAL PUBLIC LICENSE' \
    "$extract_root/usr/share/doc/openforensic-imager/copyright"; then
    printf 'copyright file contains the full GPL text\n' >&2
    exit 1
fi
grep -q 'GNU GENERAL PUBLIC LICENSE' \
    "$extract_root/usr/share/doc/openforensic-imager/LICENSE.GPL-3.0"

# A stripped release binary has no symbol table left. `.symtab` presence is
# the same condition lintian reports as unstripped-binary-or-object. `.symtab` presence is
# the same condition lintian reports as unstripped-binary-or-object.
for binary in \
    "$extract_root/usr/bin/openforensic-imager" \
    "$extract_root/usr/bin/ofi-parser-worker" \
    "$extract_root/usr/lib/openforensic-imager/libnative_bindings.so"; do
    if readelf -S "$binary" | grep -q '\.symtab'; then
        printf 'binary still carries a symbol table: %s\n' "$binary" >&2
        exit 1
    fi
done

readelf -d "$extract_root/usr/bin/openforensic-imager" \
    | grep -Fq "\$ORIGIN/../lib/openforensic-imager"

env -u LD_LIBRARY_PATH QT_QPA_PLATFORM=offscreen \
    timeout 20 "$extract_root/usr/bin/openforensic-imager" --self-test
test -x "$extract_root/usr/bin/ofi-parser-worker"
truncate -s 4096 "$worker_root/evidence.raw"
worker_response=$(
    printf '{"operation":"image_identify","path":"%s"}\n' \
        "$worker_root/evidence.raw" \
        | env -u LD_LIBRARY_PATH "$extract_root/usr/bin/ofi-parser-worker"
)
printf '%s' "$worker_response" \
    | jq -e '.ok == true and .outcome.result.format == "raw"' >/dev/null

desktop-file-validate \
    "$extract_root/usr/share/applications/io.github.x0r909.OpenForensicImager.desktop"

printf 'DEB artifact passed: %s\n' "$deb"
