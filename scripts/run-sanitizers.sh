#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
    printf 'Usage: %s --sanitizer <address|memory> [--crates "<cargo target flags>"]' \
        "$(basename "$0")"
    printf ' [--skip <test name>]... [--no-leaks]\n'
}

sanitizer=
crates=
target=x86_64-unknown-linux-gnu
skips=()
detect_leaks=1

while [[ $# -gt 0 ]]; do
    case "$1" in
        --sanitizer)
            sanitizer=${2:-}
            shift 2
            ;;
        --crates)
            crates=${2:-}
            shift 2
            ;;
        --target)
            target=${2:-}
            shift 2
            ;;
        --skip)
            skips+=(--skip "${2:?--skip needs a test name}")
            shift 2
            ;;
        --no-leaks)
            detect_leaks=0
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

case "$sanitizer" in
    address|memory) ;;
    *)
        printf 'sanitizer must be "address" or "memory"\n' >&2
        usage >&2
        exit 2
        ;;
esac

# rustc has no undefined-behaviour sanitizer; the supported set is checked by
# passing the value through to rustc itself.
# `--cfg ofi_sanitizer` lets crates relax limits that are incompatible with
# instrumentation (see ResourceLimits::parser_default).
export RUSTFLAGS="-Zsanitizer=${sanitizer} --cfg ofi_sanitizer"
export RUSTDOCFLAGS="-Zsanitizer=${sanitizer} --cfg ofi_sanitizer"

# LeakSanitizer needs to know which third-party C leaks are not ours; see the
# file's own comment for the reasoning.
suppressions="$repo_root/scripts/sanitizers/lsan.suppressions"
if [[ -f "$suppressions" ]]; then
    export LSAN_OPTIONS="suppressions=${suppressions}:print_suppressions=1${LSAN_OPTIONS:+:$LSAN_OPTIONS}"
fi

# Leak detection stays on by default; `--no-leaks` exists because the worker is
# spawned with a cleared environment, so LSAN_OPTIONS never reaches it and its
# own leak report is written to stderr after the response.
export ASAN_OPTIONS="detect_leaks=${detect_leaks}${ASAN_OPTIONS:+:$ASAN_OPTIONS}"

# The test binaries run from a cargo-emitted directory that is deeper than the
# beside-the-application lookup walks, so the debug-only override pins the worker
# the tests already built. Release builds ignore this variable.
export OFI_PARSER_WORKER="${CARGO_TARGET_DIR:-$repo_root/target}/${target}/debug/ofi-parser-worker"

log_dir="$repo_root/packaging/build/sanitizers"
cmake -E make_directory "$log_dir"
log="$log_dir/${sanitizer}.log"

selection=(--workspace)
if [[ -n "$crates" ]]; then
    # shellcheck disable=SC2206
    selection=($crates)
fi

printf 'sanitizer: %s\ntarget: %s\ndetect_leaks: %s\nselection: %s\nlog: %s\n' \
    "$sanitizer" "$target" "$detect_leaks" "${selection[*]}" "$log"

test_args=()
if [[ ${#skips[@]} -gt 0 ]]; then
    # A skipped test still runs in the normal suite; see docs/testing/phase9-results.md.
    test_args=(-- "${skips[@]}")
fi

set +e
cargo +nightly test -Zbuild-std --target "$target" "${selection[@]}" \
    "${test_args[@]}" 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
set -e

if [[ $status -ne 0 ]]; then
    printf 'sanitizer run failed with status %s; see %s\n' "$status" "$log" >&2
    exit "$status"
fi

printf '%s\n' "$log"
