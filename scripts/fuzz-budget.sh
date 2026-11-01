#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
    printf 'Usage: %s [--minutes <n>] [--target <name>]\n' "$(basename "$0")"
}

minutes=5
targets=(c_abi_json viewer_pages)
selected=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --minutes)
            minutes=${2:-}
            shift 2
            ;;
        --target)
            selected+=("${2:?--target needs a name}")
            shift 2
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

if ! [[ "$minutes" =~ ^[0-9]+$ ]] || [[ "$minutes" -eq 0 ]]; then
    printf 'minutes must be a positive integer\n' >&2
    exit 2
fi

if [[ ${#selected[@]} -gt 0 ]]; then
    targets=("${selected[@]}")
fi

seconds=$((minutes * 60))
log_dir="$repo_root/packaging/build/fuzz"
cmake -E make_directory "$log_dir"

failed=0
for target in "${targets[@]}"; do
    log="$log_dir/${target}.log"
    artifact_dir="$repo_root/tests/fuzz/artifacts/$target"
    printf 'fuzzing %s for %s minute(s); log: %s\n' "$target" "$minutes" "$log"

    # AddressSanitizer is on by default in cargo-fuzz, so this is also the ASan
    # fuzz budget. -rss_limit_mb keeps one runaway input from taking the host.
    set +e
    cargo +nightly fuzz run --fuzz-dir tests/fuzz "$target" -- \
        -max_total_time="$seconds" -rss_limit_mb=4096 >"$log" 2>&1
    status=$?
    set -e

    tail -n 3 "$log"
    if [[ $status -ne 0 ]]; then
        printf 'fuzz target %s exited with %s\n' "$target" "$status" >&2
        failed=1
    fi
    if [[ -d "$artifact_dir" ]] && [[ -n "$(ls -A "$artifact_dir")" ]]; then
        printf 'fuzz target %s produced artifacts in %s; triage them before continuing\n' \
            "$target" "$artifact_dir" >&2
        failed=1
    fi
done

exit "$failed"
