#!/usr/bin/env bash
#
# Bounded-memory acquisition soak.
#
# Drives the real acquisition path — the same `ofi-parser-worker` the
# application spawns, one JSON request on stdin — against a source of a
# chosen size, and measures what a long run is actually bounded by:
# throughput, peak RSS, and hash stability across repeated passes.
#
# It is deliberately *not* a multi-terabyte test. A multi-terabyte run needs
# release hardware and days of wall clock; this host has a fraction of that,
# and a raw acquisition writes the whole source to disk. What this script
# establishes is the memory property a soak would be run to check: peak RSS
# does not grow with the source, because the acquisition reads and writes a
# fixed-size block at a time. See TEST_STRATEGY.md for the gap.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
    cat <<'EOF'
Usage: soak-acquisition.sh [options]

  --size <size>       Source size, e.g. 100G, 2048M. Default 8G.
  --runs <n>          Passes over the same source. Default 5. Hash stability
                      is the point of more than one.
  --run-seconds <n>   Wall-clock budget per pass; a pass is stopped at the
                      deadline and reported as incomplete. Default 0 (no cap).
  --rss-ceiling-mb <n>
                      Peak-RSS ceiling. A pass above it fails. Default 512.
  --work-dir <path>   Where the source and output go. Default
                      packaging/build/soak-work. MUST be on a filesystem with
                      room for the source plus one full copy: a raw
                      acquisition writes every byte, so a 100G source needs
                      100G of output. A tmpfs is refused — see below.
  --keep              Leave the work directory in place.
  --help              This text.

Exit status is 0 only if every pass completed, stayed under the ceiling, and
produced the same hash.
EOF
}

size=8G
runs=5
run_seconds=0
rss_ceiling_mb=512
work_dir=
keep=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --size) size=${2:?--size needs a value}; shift 2 ;;
        --runs) runs=${2:?--runs needs a value}; shift 2 ;;
        --run-seconds) run_seconds=${2:?--run-seconds needs a value}; shift 2 ;;
        --rss-ceiling-mb) rss_ceiling_mb=${2:?--rss-ceiling-mb needs a value}; shift 2 ;;
        --work-dir) work_dir=${2:?--work-dir needs a path}; shift 2 ;;
        --keep) keep=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) printf 'unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

for value in "$runs" "$run_seconds" "$rss_ceiling_mb"; do
    [[ "$value" =~ ^[0-9]+$ ]] || { printf 'numeric option expected, got %s\n' "$value" >&2; exit 2; }
done
[[ "$runs" -ge 1 ]] || { printf '--runs must be at least 1\n' >&2; exit 2; }
[[ "$rss_ceiling_mb" -ge 1 ]] || { printf '--rss-ceiling-mb must be at least 1\n' >&2; exit 2; }

# `truncate` takes a size suffix directly, including G and T, and both
# `truncate` and `du` are coreutils.
command -v truncate >/dev/null || { printf 'truncate not found\n' >&2; exit 2; }
command -v /usr/bin/time >/dev/null || { printf '/usr/bin/time not found\n' >&2; exit 2; }

# Resolve the requested size to bytes with truncate itself, so the arithmetic
# in the reporting below has one source of truth for what was asked for.
probe=$(mktemp)
trap 'rm -f "$probe"' EXIT
truncate -s "$size" "$probe"
size_bytes=$(stat -c %s "$probe")
rm -f "$probe"
trap - EXIT

log_dir="$repo_root/packaging/build/soak"
mkdir -p "$log_dir"

created_work=0
if [[ -z "$work_dir" ]]; then
    # Default under the repository, not /tmp: /tmp is a tmpfs on many hosts
    # (including this one), so a soak there would fill RAM instead of disk and
    # the peak-RSS measurement would count the page cache as the worker's
    # memory. The worst place to put a multi-gigabyte sink is a RAM disk.
    work_dir="$repo_root/packaging/build/soak-work"
    mkdir -p "$work_dir"
    created_work=1
fi
work_type=$(stat -f -c %T "$work_dir" 2>/dev/null || echo unknown)
if [[ "$work_type" == tmpfs ]]; then
    printf 'refusing to run in %s: it is a tmpfs.\n' "$work_dir" >&2
    printf 'A tmpfs \"filesystem\" is RAM, so a soak there fills memory instead of\n' >&2
    printf 'disk and the peak-RSS figure measures the wrong thing. Pass --work-dir\n' >&2
    printf 'pointing at a real filesystem with room for the source plus one copy.\n' >&2
    exit 2
fi

# The worker refuses an output directory that is group- or other-writable
# (worker.sandbox), and that is a real constraint of the product rather than
# something to work around.
mkdir -p "$work_dir/source" "$work_dir/out" "$work_dir/reports"
chmod 700 "$work_dir/source" "$work_dir/out" "$work_dir/reports"
source_path="$work_dir/source/evidence.raw"

# A sparse file: the acquisition reads every byte it is told the source is, and
# a hole reads back as zeroes, so the content is defined even though the disk
# is not asked to hold it. That is what makes a size larger than free space a
# valid *read* test. It is not valid as a *write* test — see the note about
# output below.
truncate -s "$size_bytes" "$source_path"
# The marker goes in with `conv=notrunc`, because a plain `cat >` would
# truncate the file just created and leave a 25-byte source. It exists so a
# pass over an all-zero source is not the same bytes as a file that was never
# written, and so two different runs can be told apart.
printf 'openforensic soak marker\n' \
    | dd of="$source_path" conv=notrunc status=none

free_kb=$(df -Pk "$work_dir/out" | awk 'NR==2 {print $4}')
free_bytes=$((free_kb * 1024))
if [[ "$free_bytes" -lt "$size_bytes" ]]; then
    printf 'WARNING: %s free under %s, less than the %s source.\n' \
        "$(numfmt --to=iec "$free_bytes")" "$work_dir/out" "$size" >&2
    printf 'A raw acquisition writes every byte of the source, so this run will\n' >&2
    printf 'fill the filesystem and fail. Use --size within free space, or point\n' >&2
    printf '--work-dir at a filesystem that has room for source plus one copy.\n' >&2
fi

# The release worker is the one a soak uses: the debug build is roughly
# twenty-five times slower, and a soak is a wall-clock exercise.
printf 'building the release worker...\n'
cargo build --release -p native-bindings --bin ofi-parser-worker >/dev/null
worker="$repo_root/target/release/ofi-parser-worker"

log="$log_dir/acquisition-$(date -u +%Y%m%dT%H%M%SZ).log"
{
    printf 'soak-acquisition\n'
    printf '  source      %s (sparse, %s)\n' "$source_path" "$size"
    printf '  output      %s\n' "$work_dir/out"
    printf '  worker      %s\n' "$worker"
    printf '  runs        %s\n' "$runs"
    printf '  ceiling     %s MB peak RSS\n' "$rss_ceiling_mb"
    printf '  free space  %s in %s\n' "$(numfmt --to=iec "$free_bytes")" "$work_dir/out"
    printf '  log         %s\n\n' "$log"
} | tee "$log"

failed=0
reference_hash=
for ((pass = 1; pass <= runs; pass++)); do
    request="$work_dir/request-$pass.json"
    response="$work_dir/response-$pass.json"
    timing="$work_dir/time-$pass.txt"
    # A fresh output directory per pass: the acquisition refuses to overwrite
    # an existing image (create_new), which is itself a property worth keeping
    # rather than defeating.
    rm -rf "$work_dir/out"
    mkdir -p "$work_dir/out"
    chmod 700 "$work_dir/out"

    cat >"$request" <<EOF
{"operation":"acquisition","source_path":"$source_path","destination_dir":"$work_dir/out","base_name":"soak","format":"raw","options":{"block_size":1048576,"maximum_segment_size":134217728,"compression":"none","verify_after":false}}
EOF

    # The worker's stdout goes through `cat` so that the worker itself writes to
    # a pipe. That is production's shape — the parent spawns it with
    # `Stdio::piped()` — and it matters: `RLIMIT_FSIZE` does not govern pipes,
    # so a job that does exceed its budget returns a typed error over the pipe,
    # whereas the same worker writing straight into this regular file would
    # breach the limit on the *response* and die without one. Capturing through
    # a pipe is what keeps a limit breach legible instead of turning it into a
    # bare exit status.
    start=$(date +%s.%N)
    set +e
    if [[ "$run_seconds" -gt 0 ]]; then
        /usr/bin/time -v -o "$timing" timeout "$run_seconds" "$worker" \
            <"$request" 2>/dev/null | cat >"$response"
    else
        /usr/bin/time -v -o "$timing" "$worker" <"$request" 2>/dev/null | cat >"$response"
    fi
    status=${PIPESTATUS[0]}
    set -e
    end=$(date +%s.%N)
    wall=$(awk "BEGIN {printf \"%.2f\", $end - $start}")
    peak_rss_kb=$(awk '/Maximum resident set size/ {print $NF}' "$timing" 2>/dev/null || echo 0)
    peak_rss_mb=$((peak_rss_kb / 1024))

    note=
    if [[ "$status" -eq 124 ]]; then
        note=" (stopped at the ${run_seconds}s deadline)"
    elif [[ "$status" -ne 0 ]]; then
        note=" (worker exit $status)"
    fi

    if [[ "$status" -ne 0 ]]; then
        printf 'pass %s/%s FAILED: exit %s%s, peak RSS %s MB, %ss\n' \
            "$pass" "$runs" "$status" "$note" "$peak_rss_mb" "$wall" | tee -a "$log"
        tail -n 5 "$response" | tee -a "$log"
        failed=1
        break
    fi

    hash=$(python3 - "$response" <<'PY'
import json, sys
with open(sys.argv[1]) as handle:
    document = json.load(handle)
outcome = document.get("outcome") or {}
manifest = outcome.get("result", {}).get("manifest")
if manifest is None:
    print("")
    raise SystemExit(0)
scoped = [h for h in manifest["hashes"]
          if h["algorithm"] == "sha256" and h["scope"] == "output_logical"]
print(scoped[0]["digest_hex"] if scoped else "")
PY
)
    bytes_written=$(python3 - "$response" <<'PY'
import json, sys
with open(sys.argv[1]) as handle:
    document = json.load(handle)
manifest = (document.get("outcome") or {}).get("result", {}).get("manifest")
print(manifest["bytes_written"] if manifest else 0)
PY
)

    if [[ -z "$hash" ]]; then
        printf 'pass %s/%s FAILED: no output hash in the response\n' "$pass" "$runs" | tee -a "$log"
        head -c 400 "$response" | tee -a "$log"
        printf '\n' | tee -a "$log"
        failed=1
        break
    fi

    throughput=$(awk "BEGIN {printf \"%.1f\", $size_bytes / $wall / 1048576}")
    printf 'pass %s/%s  ok  %ss  %s MiB/s  peak RSS %s MB  bytes %s  sha256 %s\n' \
        "$pass" "$runs" "$wall" "$throughput" "$peak_rss_mb" "$bytes_written" "${hash:0:16}" \
        | tee -a "$log"

    if [[ "$peak_rss_mb" -gt "$rss_ceiling_mb" ]]; then
        printf 'pass %s/%s FAILED: peak RSS %s MB is above the %s MB ceiling\n' \
            "$pass" "$runs" "$peak_rss_mb" "$rss_ceiling_mb" | tee -a "$log"
        failed=1
        break
    fi

    if [[ -z "$reference_hash" ]]; then
        reference_hash=$hash
    elif [[ "$hash" != "$reference_hash" ]]; then
        printf 'pass %s/%s FAILED: hash %s differs from pass 1 %s\n' \
            "$pass" "$runs" "${hash:0:16}" "${reference_hash:0:16}" | tee -a "$log"
        failed=1
        break
    fi
done

{
    printf '\n'
    if [[ "$failed" -eq 0 ]]; then
        printf 'soak-acquisition PASSED: %s passes over %s, hash stable at %s, peak RSS under %s MB\n' \
            "$runs" "$size" "${reference_hash:0:16}" "$rss_ceiling_mb"
    else
        printf 'soak-acquisition FAILED\n'
    fi
} | tee -a "$log"

if [[ "$keep" -eq 0 ]] && [[ "$created_work" -eq 1 ]]; then
    rm -rf "$work_dir"
else
    printf 'work directory kept: %s\n' "$work_dir" | tee -a "$log"
fi

exit "$failed"
