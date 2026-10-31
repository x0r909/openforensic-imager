#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
build_root=${1:-"$repo_root/packaging/build/flatpak-app"}
bundle=${2:-}

if [[ ! -d "$build_root/files" ]]; then
    printf 'Flatpak app build not found: %s\n' "$build_root" >&2
    exit 1
fi

test -x "$build_root/files/bin/openforensic-imager"
test -x "$build_root/files/bin/ofi-parser-worker"
test -f "$build_root/files/lib/openforensic-imager/libnative_bindings.so"
test -f "$build_root/files/share/applications/io.github.x0r909.OpenForensicImager.desktop"
test -f "$build_root/files/share/metainfo/io.github.x0r909.OpenForensicImager.metainfo.xml"

flatpak build "$build_root" \
    env QT_QPA_PLATFORM=offscreen \
    /app/bin/openforensic-imager --self-test

worker_root="$build_root/files/.worker-self-test"
mkdir -p "$worker_root"
trap 'cmake -E remove_directory "$worker_root"' EXIT
truncate -s 4096 "$worker_root/evidence.raw"
worker_response=$(
    printf '{"operation":"image_identify","path":"%s"}\n' \
        "/app/.worker-self-test/evidence.raw" \
        | flatpak build "$build_root" \
            env LD_LIBRARY_PATH=/app/lib/openforensic-imager \
            /app/bin/ofi-parser-worker
)
printf '%s' "$worker_response" \
    | jq -e '.ok == true and .outcome.result.format == "raw"' >/dev/null

flatpak build --readonly "$build_root" sh -c \
    'test ! -e /dev/sda && test ! -e /dev/nvme0n1'

if [[ -n "$bundle" && -f "$bundle" ]]; then
    temporary_data=$(mktemp -d)
    XDG_DATA_HOME="$temporary_data" \
        flatpak --user install --noninteractive --no-deps "$bundle"
    XDG_DATA_HOME="$temporary_data" \
        flatpak run --env=QT_QPA_PLATFORM=offscreen \
        io.github.x0r909.OpenForensicImager --self-test
    cmake -E remove_directory "$temporary_data"
fi

printf 'Flatpak analysis build passed: %s\n' "$build_root"
