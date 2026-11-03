#!/usr/bin/env bash
set -euo pipefail

# `scripts/test-clean-vm.sh --mode build` cannot be run here: it boots a guest
# through emulated firmware and takes tens of minutes. What this test pins is
# the set of defects that made the first build runs fail or deliver nothing,
# each of which is decidable from the repository without a guest:
#
#   1. The workspace copy sent the whole tree minus four --exclude patterns,
#      which was 2.9 GiB because tests/fuzz/target (2.3 GiB) and the supplied
#      FTK installer were not among them. The copy crosses SSH into a guest
#      running under TCG emulation, so it was the slowest step of the run.
#   2. packaging/tests/test_deb_artifact.sh:157 runs desktop-file-validate and
#      aborts on failure, but desktop-file-utils was in neither the base image
#      nor the guest's apt install list — so package-deb.sh could not succeed
#      in the guest no matter how long the build was given.
#   3. The build branch printed digests and stopped. Nothing was copied out, so
#      a run that built correctly still left the artifacts inside a guest that
#      is deleted at the end of the run.
#   4. Nothing installed and self-tested the artifact in the guest, which is
#      the gate the plan sets before the ARM64 item may be ticked.
#   5. cleanup() deleted the seed and keys but not the overlay, contradicting
#      the comment above the overlay creation that promises deletion unless
#      --keep. At the --disk-gb a Rust build needs, that is tens of GiB per run.
#
# Assertions on the script's text are used where the behaviour can only be
# observed inside a guest; where it can be run here, the real mechanism is run.

trap 'printf "assertion failed: %s:%s\n" "${BASH_SOURCE[0]}" "$LINENO" >&2' ERR

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

harness=scripts/test-clean-vm.sh
[[ -f "$harness" ]] || { printf 'missing %s\n' "$harness" >&2; exit 1; }

# --- 1. The workspace copy is the tracked tree, not a deny-list ------------
#
# Behavioural: the copy command is extracted from the harness and run here,
# with its destination switched from the guest to a scratch archive. Measuring
# a pipeline written in this test would only prove this test agrees with
# itself; running the harness's own command is what binds the assertion to the
# script. A deny-list cannot express what the copy needs to exclude, because
# the next large untracked directory is excluded by nothing — the tracked tree
# is exactly what the guest build needs and nothing else.

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

# From the harness's `git -C "$repo_root" ls-files` through the pipe into the
# guest. The trailing line continuation and the `| guest ...` stage are
# stripped, so what runs is the archive-producing half unchanged.
copy_cmd=$(
    awk '/git -C "\$repo_root" ls-files/{found=1} found{print; if ($0 ~ /\| guest/) exit}' \
        "$harness" \
        | sed -e 's/| guest.*//' -e 's/[[:space:]]*\\$//' \
        | sed -e '/^[[:space:]]*$/d'
)
if [[ -z "$copy_cmd" ]]; then
    printf 'could not find the workspace copy command in %s\n' "$harness" >&2
    exit 1
fi

# The command names the tracked tree. If the harness is changed to a different
# mechanism, the extraction above stops finding it and this fails loudly rather
# than silently measuring the wrong thing.
grep -qF 'ls-files' <<<"$copy_cmd"

( cd "$repo_root" && eval "$copy_cmd" ) >"$scratch/workspace.tar"
payload_bytes=$(stat -c '%s' "$scratch/workspace.tar")
# 1.7 MB today. A ceiling rather than an equality: adding a source file must
# not fail the test, but re-admitting a target directory or a binary blob must.
ceiling_bytes=$((64 * 1024 * 1024))
if [[ "$payload_bytes" -gt "$ceiling_bytes" ]]; then
    printf 'workspace payload is %s bytes, over the %s-byte ceiling\n' \
        "$payload_bytes" "$ceiling_bytes" >&2
    printf 'the copy is a git ls-files allow-list; check what was added to it\n' >&2
    exit 1
fi

members=$(tar -tf "$scratch/workspace.tar")
for forbidden in 'tests/fuzz/target/' 'FTK-Imager-' 'target/' 'build/desktop/'; do
    if printf '%s\n' "$members" | grep -qF "$forbidden"; then
        printf 'workspace copy contains %s\n' "$forbidden" >&2
        exit 1
    fi
done
# The allow-list must still carry the inputs a build needs, or the guest build
# fails for the opposite reason.
for required in Cargo.toml Cargo.lock scripts/lib/packaging.sh \
                packaging/debian/control.in apps/desktop/CMakeLists.txt \
                packaging/common/openforensic-imager.desktop; do
    printf '%s\n' "$members" | grep -qxF "$required"
done

# --- 2. Every validator the guest runs is installed in the guest ------------
#
# package-deb.sh and package-appimage.sh run packaging/tests/test_deb_artifact.sh
# and test_appimage_artifact.sh inside the guest, and those tests run under
# `set -euo pipefail` with an ERR trap, so a missing tool aborts the packaging
# script rather than skipping a check. This asks which tools those two tests
# call and whether the harness installs their providers.
#
# It is deliberately a bounded list, not a dependency resolver: the base image
# already ships most of what the tests use (jq, man-db, binutils, file), and
# checking each of those against the image's dpkg database would duplicate what
# the image pins. What it covers is the tools the image does *not* ship, which
# is the set that has to be in the apt line. test_phase7_layout.sh's
# appstreamcli is absent on purpose: only scripts/test-phase7.sh runs that test,
# and the build branch does not invoke it.

apt_line=$(grep -n 'apt-get install -y --no-install-recommends' "$harness" | head -1)
[[ -n "$apt_line" ]] || { printf 'no guest apt install line found\n' >&2; exit 1; }

# Command the packaging tests invoke -> the Debian package that provides it.
declare -A providers=(
    [desktop-file-validate]=desktop-file-utils
)
for tool in "${!providers[@]}"; do
    package=${providers[$tool]}
    # The tool has to be required by a test the build branch runs, or the entry
    # is stale and this test is guarding nothing.
    grep -rqF "$tool " packaging/tests/test_deb_artifact.sh packaging/tests/test_appimage_artifact.sh \
        || { printf '%s is in the provider map but no packaging test calls it\n' "$tool" >&2; exit 1; }
    if ! grep -qF -- "$package" <<<"$apt_line"; then
        printf 'the guest installs no provider for %s (needs %s)\n' "$tool" "$package" >&2
        exit 1
    fi
    # A package name that the distribution does not have would make the whole
    # apt-get install line fail, taking every later step with it.
    apt-get -s install --no-install-recommends "$package" >/dev/null 2>&1 \
        || { printf '%s is not installable by this host\n' "$package" >&2; exit 1; }
done

# --- 3 and 4. The build branch copies out, verifies, and installs ----------
#
# Only observable inside a guest, so this asserts on the script. The build
# branch is the text between its case arm and the next one; searching the whole
# file would let --mode install's copy-out satisfy the build branch's check.
build_branch=$(sed -n '/^    build)$/,/^    install)$/p' "$harness")
[[ -n "$build_branch" ]] || { printf 'no build branch in %s\n' "$harness" >&2; exit 1; }

for needle in 'scp -q' 'sha256sum -c' 'apt-get install -y ./packaging/output/*.deb'; do
    if ! grep -qF -- "$needle" <<<"$build_branch"; then
        printf 'the build branch is missing: %s\n' "$needle" >&2
        exit 1
    fi
done
# The guest copies out to a directory named for the architecture, so an arm64
# run cannot be mistaken for the host's own amd64 builds.
if ! grep -qF 'copy_dir="$work_dir/$arch-artifacts"' <<<"$build_branch"; then
    printf 'the copy-out directory is not named for the architecture\n' >&2
    exit 1
fi

# --- 4a. A failed step produces a verdict, not a silent abort --------------
#
# The script runs under `set -euo pipefail`, so a bare command that fails aborts
# it before the `clean-vm … FAILED` line at the bottom of the run: the run exits
# non-zero with no statement of which step failed, which reads as a crash rather
# than as a failed check. Every step in the build arm must therefore capture its
# status (`&& status=0 || status=$?_`) and gate the steps after it. Asserted as
# text for the same reason as the checks above: reachable only inside a guest.
if ! grep -qF 'git -C "$repo_root" ls-files -z | tar' <<<"$build_branch"; then
    printf 'the build arm no longer copies the workspace in\n' >&2
    exit 1
fi
# The copy is the one step written as a pipeline rather than a run_in_guest
# call, and it is also the step a failed transfer surfaces on first, so it is
# checked on its own rather than by the loop below.
if ! grep -A2 -F 'git -C "$repo_root" ls-files -z | tar' <<<"$build_branch" | grep -qF 'status=$?'; then
    printf 'the workspace copy does not capture its exit status\n' >&2
    exit 1
fi
# Any run_in_guest call in this arm without a status capture would abort the
# script. Each call is a `run_in_guest "name" \` line followed by its arguments
# and the `&& status=0 || status=$?` on its last line, so each call is one block:
# from its own line up to the next call. Checked per block rather than by
# counting occurrences, because a capture belonging to a neighbouring step is
# only a few lines away and counting would let a missing one hide behind the next
# step's. `grep -A4` is not used for the same reason: overlapping context merges
# adjacent blocks. awk over the whole arm delimits them exactly.
call_count=$(grep -c 'run_in_guest "' <<<"$build_branch" || true)
if [[ "${call_count:-0}" -eq 0 ]]; then
    printf 'the build arm makes no guest calls; this check is guarding nothing\n' >&2
    exit 1
fi
unguarded=$(awk '
    /run_in_guest "/ { if (n && !hit) print prev; prev = $0; hit = 0; n++; next }
    /status=\$\?/    { hit = 1 }
    END              { if (n && !hit) print prev }' <<<"$build_branch")
if [[ -n "$unguarded" ]]; then
    printf 'guest steps run without capturing their status:\n%s\n' "$unguarded" >&2
    exit 1
fi
# The digest step specifically: it reports the artifacts, and a failure there
# must stop the copy-out that follows rather than abort between them. Checked as
# a window from the step's own name, because `&& status=0 || status=$?` also ends
# the three guest steps listed above it, so a plain search finds a neighbour's.
# The window ends at the first `fi` that closes the call.
digest_block=$(grep -A6 -F '"artifact digests"' <<<"$build_branch" | sed -n '1,/^[[:space:]]*fi$/p')
if ! grep -qF 'status=$?' <<<"$digest_block"; then
    printf 'the artifact-digest step does not capture its status\n' >&2
    exit 1
fi
# The dependency report describes the guest as it stands, so in the build arm it
# has to run after the install that puts the libraries there — before that
# install it reports the stock image, which ships none of the libraries the
# artifacts link against and so prints nothing.
if ! grep -qF 'report_native_dependencies()' "$harness"; then
    printf 'the native-dependency report is not a function that can be re-run\n' >&2
    exit 1
fi
# `|| true` on both: with no match, `grep` exits 1 and `pipefail` would abort the
# script here, so the check below could never report what is actually wrong.
install_line=$(grep -n 'install build dependencies' <<<"$build_branch" | head -1 | cut -d: -f1 || true)
report_line=$(grep -n 'report_native_dependencies' <<<"$build_branch" | head -1 | cut -d: -f1 || true)
if [[ -z "$install_line" || -z "$report_line" ]]; then
    printf 'the build arm does not install dependencies and report them\n' >&2
    exit 1
fi
if (( report_line < install_line )); then
    printf 'the build arm reports dependencies before installing them\n' >&2
    exit 1
fi
# `boot` mode never installs anything, so its report is the stock image's and
# must still run — the two arms are the two places the function is called.
if ! grep -qF 'if [[ "$mode" == boot ]]; then' "$harness"; then
    printf 'boot mode no longer reports the stock image dependencies\n' >&2
    exit 1
fi

# --- 4b. The deadline scale reaches the guest's test run -------------------
#
# The workspace tests time jobs on the wall clock, and the guest hashes about 9x
# slower than the build host, so without a factor the in-guest
# `cargo test --workspace` fails on the slowest jobs for a reason that has
# nothing to do with the code. Asserted on the script because the effect is only
# observable inside a guest; the default is checked against the value the option
# documents, so a default that drifts from its own help text fails here.

if ! grep -qF -- '--deadline-scale <n>' "$harness"; then
    printf 'the harness does not document --deadline-scale\n' >&2
    exit 1
fi
if ! grep -qF 'OFI_TEST_DEADLINE_SCALE=$deadline_scale' <<<"$build_branch"; then
    printf 'the build branch does not pass the deadline scale into the guest\n' >&2
    exit 1
fi
# A non-numeric value would abort the run at the numeric-option validator, so
# the default has to be a number.
default_scale=$(grep -E '^deadline_scale=[0-9]+$' "$harness" | head -1 | cut -d= -f2)
[[ -n "$default_scale" ]] || { printf 'deadline_scale has no numeric default\n' >&2; exit 1; }
# The reader must accept the default, or every build run fails on a value the
# harness chose. The upper bound lives in the test source, so it is read from
# there rather than restated.
reader=crates/native-bindings/tests/worker_isolation.rs
max_scale=$(grep -E '^const MAX_DEADLINE_SCALE: u32 = [0-9]+;' "$reader" | sed -E 's/.*= ([0-9]+);/\1/')
[[ -n "$max_scale" ]] || { printf 'MAX_DEADLINE_SCALE not found in %s\n' "$reader" >&2; exit 1; }
if (( default_scale < 1 || default_scale > max_scale )); then
    printf 'the harness default %s is outside the accepted range 1..=%s\n' \
        "$default_scale" "$max_scale" >&2
    exit 1
fi
# The help text states the default; keep it honest.
if ! grep -qF "Default $default_scale" <<<"$(sed -n '/--deadline-scale/,/--work-dir/p' "$harness")"; then
    printf -- '--deadline-scale help does not state its default %s\n' "$default_scale" >&2
    exit 1
fi

# --- 5. cleanup() deletes the overlay unless --keep ------------------------
#
# The overlay is the one artefact of a run that costs tens of GiB, and the
# comment above its creation promises it is deleted. Asserted as text because
# deleting it is reachable only at the end of a completed run; the guard for an
# exit before the overlay exists is exercised for real below.

cleanup_body=$(sed -n '/^cleanup() {/,/^}/p' "$harness")
[[ -n "$cleanup_body" ]] || { printf 'no cleanup() in %s\n' "$harness" >&2; exit 1; }
if ! grep -qF 'rm -f "$disk"' <<<"$cleanup_body"; then
    printf 'cleanup() does not delete the overlay; a run leaves --disk-gb of qcow2 behind\n' >&2
    exit 1
fi
# The deletion must be guarded against `disk` being unset, or a run that exits
# before the overlay exists — a base-image digest mismatch is the reachable
# case, exercised below — aborts its own cleanup under `set -u` instead of
# reporting the mismatch. Assumed here as well as exercised, because the
# exercise only reaches the digest path while this text check reaches any early
# exit that is added later.
if ! grep -qF '[[ -n "${disk:-}" ]] && rm -f "$disk"' <<<"$cleanup_body"; then
    printf 'cleanup() deletes the overlay without guarding an unset $disk under set -u\n' >&2
    exit 1
fi

# Behavioural, and a guard on this change rather than on the defect: the
# deletion of the overlay is guarded with `[[ -n "${disk:-}" ]]`, because a run
# that exits before the overlay is created (a digest mismatch) reaches cleanup()
# with `disk` unset, where an unguarded `rm -f "$disk"` aborts the trap under
# `set -u` and buries the real error. This exercises that early path for real.
#
# The trap is disabled around the expected failure: bash runs an ERR trap on
# any failing command whether or not `set -e` is in effect, so `set +e` alone
# would still leave the trap firing on the harness's own non-zero exit — and an
# ERR trap inside an EXIT trap cannot be caught by a surrounding test. The
# status is compared by hand instead.
#
# The work directory must not be a tmpfs: the harness refuses those by design,
# and $TMPDIR usually is one.
work="$repo_root/packaging/build/clean-vm-harness-test"
rm -rf "$work"
mkdir -p "$work"
# One EXIT trap for both scratch trees: a second `trap ... EXIT` replaces the
# first, which is how the harness's own temporaries would have leaked.
trap 'rm -rf "$scratch" "$work"' EXIT
# The image must exist for the digest check to be reached; a 1-byte stand-in
# fails the digest and exits, which is the path under test.
printf 'x' >"$work/stand-in.img"
# Both are needed: `set -e` would abort this test at the harness's non-zero
# exit before the status is read, and the ERR trap fires on a failing command
# whether or not errexit is set.
set +e
trap - ERR
bash "$harness" --arch arm64 --mode boot --image "$work/stand-in.img" \
    --digest "$(printf '0%.0s' {1..64})" --work-dir "$work" \
    >"$work/out.log" 2>&1
early_status=$?
set -e
trap 'printf "assertion failed: %s:%s\n" "${BASH_SOURCE[0]}" "$LINENO" >&2' ERR
if [[ "$early_status" -eq 0 ]]; then
    printf 'a digest mismatch was reported as success\n' >&2
    exit 1
fi
if grep -q 'unbound variable' "$work/out.log"; then
    printf 'cleanup() aborted on an unset overlay path:\n' >&2
    sed -n '1,20p' "$work/out.log" >&2
    exit 1
fi
if ! grep -q 'base image digest mismatch' "$work/out.log"; then
    printf 'the digest mismatch did not reach the check under test:\n' >&2
    sed -n '1,20p' "$work/out.log" >&2
    exit 1
fi
rm -rf "$work"

printf 'clean-VM harness passed: %s bytes of workspace, no forbidden paths\n' \
    "$payload_bytes"