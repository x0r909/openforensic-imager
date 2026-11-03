#!/usr/bin/env bash
#
# Boot a clean Ubuntu cloud image and exercise the product inside it.
#
# This is the clean-system test: a disposable guest, built by cloud-init from a
# pinned cloud image, running the real artifact with the distribution's own
# native libraries rather than the build host's. It exists because the
# packaging tests on the build host prove the artifact is well formed, not that
# it installs and runs on a host that has never seen this repository.
#
#   scripts/test-clean-vm.sh --arch arm64    # qemu-system-aarch64 + AAVMF
#   scripts/test-clean-vm.sh --arch amd64    # qemu-system-x86_64
#
# The guest is driven over SSH on a forwarded port — not by expecting a shell
# prompt on the serial console, which is not a stable interface. The serial
# console is still captured to a log, because a guest that fails before SSH
# comes up is diagnosable only from there.
#
# The modes differ in what they prove, and neither is a substitute for the
# other:
#
#   --mode boot      boot the guest, wait for cloud-init, print its identity
#                    and the native library versions it ships, and stop. This
#                    is the cheapest way to verify the harness itself, and it
#                    is C5 step 1: the guest runs at all.
#   --mode install   install the artifact and run its self-test. This is the
#                    clean-system gate: does the package install on a host with
#                    none of the build dependencies, and does the installed
#                    binary work.
#   --mode build     build the workspace and the packages natively inside the
#                    guest. Slower by far — an emulated guest runs at a
#                    fraction of native speed — but it is what produces a
#                    native-architecture artifact.
#
# A guest that did not finish is reported as incomplete, not as a pass. See
# docs/testing/device-acquisition-runbook.md for the same rule applied to
# hardware.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
    cat <<'EOF'
Usage: test-clean-vm.sh [options]

  --arch <arm64|amd64>   Guest architecture. Default arm64.
  --mode <boot|build|install>
                         What to do in the guest. Default build.
                           boot:    just boot and report the guest's identity
                           build:   cargo test --workspace, then package
                           install: install the prepared artifact + self-test
  --image <path>         Base cloud image. Default: the pinned image under
                         packaging/build/clean-vm/images, downloaded if absent.
  --digest <sha256>      Expected SHA-256 of the base image. Default: the
                         pinned digest for the selected architecture.
  --artifact <path>      Artifact to install in --mode install. Default: the
                         newest matching artifact under packaging/output.
  --disk-gb <n>          Guest disk size. Default 24.
  --memory-mb <n>        Guest memory. Default 4096.
  --cpus <n>             Guest vCPUs. Default 4. The guest runs the
                         distribution's own udev and systemd, whose coldplug is
                         CPU-bound under emulation. This run raises the guest's
                         device-unit timeout in the overlay so a slow coldplug
                         does not fail the boot; more vCPUs still make an
                         emulated build finish sooner.
  --boot-timeout <n>     Seconds to wait for SSH. Default 600.
  --work-timeout <n>     Seconds to let the in-guest work run; 0 disables.
                         Default 0.
  --deadline-scale <n>   Factor by which the workspace tests' wall-clock
                         deadlines are stretched, passed to the guest as
                         OFI_TEST_DEADLINE_SCALE. Default 10, measured: the
                         guest hashes a 70 MiB image about 9x slower than the
                         build host. Set 1 to run the tests' native deadlines
                         unchanged.
  --work-dir <path>      Where images, overlays, and logs go. Default
                         packaging/build/clean-vm.
  --keep                 Keep the overlay, seed, and keys for inspection.
  --help                 This text.

Exit status is 0 only if the guest booted, cloud-init finished, and every
in-guest command succeeded within its timeout.
EOF
}

arch=arm64
mode=build
image=
digest=
artifact=
disk_gb=24
memory_mb=4096
cpus=4
boot_timeout=600
work_timeout=0
# An emulated guest hashes a 70 MiB image about 9x slower than the build host
# (measured; see --deadline-scale), and the workspace tests time their jobs on
# the wall clock. Without this factor the guest's `cargo test --workspace`
# fails on slow hosts for jobs that would pass natively — a slow host reading
# as broken code. 10 is the measurement rounded up.
deadline_scale=10
work_dir=
keep=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --arch) arch=${2:?--arch needs a value}; shift 2 ;;
        --mode) mode=${2:?--mode needs a value}; shift 2 ;;
        --image) image=${2:?--image needs a path}; shift 2 ;;
        --digest) digest=${2:?--digest needs a value}; shift 2 ;;
        --artifact) artifact=${2:?--artifact needs a path}; shift 2 ;;
        --disk-gb) disk_gb=${2:?--disk-gb needs a value}; shift 2 ;;
        --memory-mb) memory_mb=${2:?--memory-mb needs a value}; shift 2 ;;
        --cpus) cpus=${2:?--cpus needs a value}; shift 2 ;;
        --boot-timeout) boot_timeout=${2:?--boot-timeout needs a value}; shift 2 ;;
        --work-timeout) work_timeout=${2:?--work-timeout needs a value}; shift 2 ;;
        --deadline-scale) deadline_scale=${2:?--deadline-scale needs a value}; shift 2 ;;
        --work-dir) work_dir=${2:?--work-dir needs a path}; shift 2 ;;
        --keep) keep=1; shift ;;
        --help|-h) usage; exit 0 ;;
        *) printf 'unknown option: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

case "$arch" in
    arm64|amd64) ;;
    *) printf '--arch must be arm64 or amd64, got %s\n' "$arch" >&2; exit 2 ;;
esac
case "$mode" in
    boot|build|install) ;;
    *) printf '--mode must be boot, build, or install, got %s\n' "$mode" >&2; exit 2 ;;
esac
for value in "$disk_gb" "$memory_mb" "$cpus" "$boot_timeout" "$work_timeout" "$deadline_scale"; do
    [[ "$value" =~ ^[0-9]+$ ]] || { printf 'numeric option expected, got %s\n' "$value" >&2; exit 2; }
done

# The pinned base images. Ubuntu 26.04 ("resolute") server cloud images, taken
# from cloud-images.ubuntu.com/releases/resolute/release/SHA256SUMS. The digest
# is pinned here rather than read from the directory listing at run time, so a
# re-published image is a deliberate change to this file and not a silent one.
image_url_base=https://cloud-images.ubuntu.com/releases/resolute/release
case "$arch" in
    arm64)
        qemu_binary=qemu-system-aarch64
        # The Debian package that provides qemu-system-aarch64 is
        # qemu-system-arm; naming the binary would send a reader to an
        # apt package that does not exist.
        qemu_package=qemu-system-arm
        default_image=ubuntu-26.04-server-cloudimg-arm64.img
        default_digest=8eb876c2ccc4b6ff89e4a65d511815923b285e04503ba465809bf587f763da24
        ;;
    amd64)
        qemu_binary=qemu-system-x86_64
        qemu_package=qemu-system-x86
        default_image=ubuntu-26.04-server-cloudimg-amd64.img
        default_digest=5645d0318ad21f643ebb1ac4567514620e9893380b07b0201fda163baa5b7eb5
        ;;
esac
image=${image:-}
digest=${digest:-$default_digest}

command -v "$qemu_binary" >/dev/null \
    || { printf '%s not found; install it with: sudo apt install %s\n' \
         "$qemu_binary" "$qemu_package" >&2; exit 2; }
command -v qemu-img >/dev/null || { printf 'qemu-img not found\n' >&2; exit 2; }
command -v xorriso >/dev/null || { printf 'xorriso not found\n' >&2; exit 2; }
command -v ssh-keygen >/dev/null || { printf 'ssh-keygen not found\n' >&2; exit 2; }
command -v ssh >/dev/null || { printf 'ssh not found\n' >&2; exit 2; }
# The BOOT partition's grub.cfg is edited in the disposable overlay, before the
# guest starts, to raise the systemd device-unit timeout (see the boot section).
# `debugfs` is the only ext4 editor available without root.
command -v debugfs >/dev/null || { printf 'debugfs not found; install e2fsprogs\n' >&2; exit 2; }
# These two carry the timeout patch: python3 parses the GPT, qemu-io writes the
# edited partition back into the overlay. Both are checked here because a
# harness that reaches the patch without them fails in the middle of a run.
command -v python3 >/dev/null || { printf 'python3 not found\n' >&2; exit 2; }
command -v qemu-io >/dev/null || { printf 'qemu-io not found\n' >&2; exit 2; }

work_dir=${work_dir:-"$repo_root/packaging/build/clean-vm"}
mkdir -p "$work_dir"
# Absolute paths, not the caller's: qemu-img resolves an overlay's backing file
# relative to the overlay's own directory, so a relative image path becomes
# "<work-dir>/<work-dir>/images/…" and the run dies with "Could not open backing
# image" before it boots anything.
work_dir=$(realpath "$work_dir")
image=${image:-"$work_dir/images/$default_image"}
image=$(realpath -m "$image")

# A tmpfs is RAM: a 900 MiB base image plus a 24 GiB sparse overlay plus a
# build tree is the wrong thing to hold in memory, and it makes the guest's
# own memory accounting meaningless.
if [[ "$(stat -f -c %T "$work_dir" 2>/dev/null || echo unknown)" == tmpfs ]]; then
    printf 'refusing to run in %s: it is a tmpfs\n' "$work_dir" >&2
    printf 'Pass --work-dir pointing at a real filesystem.\n' >&2
    exit 2
fi

serial_log="$work_dir/$arch-serial.log"
console_log="$work_dir/$arch-console.log"
: >"$serial_log"

log() { printf '%s\n' "$*" | tee -a "$console_log"; }

printf 'clean-vm: arch=%s mode=%s\n' "$arch" "$mode" | tee "$console_log"

# --- Base image -------------------------------------------------------------
if [[ ! -f "$image" ]]; then
    mkdir -p "$(dirname "$image")"
    log "downloading $default_image (recorded digest $digest)"
    part="$image.part"
    rm -f "$part"
    curl --fail --location --silent --show-error \
        "$image_url_base/$default_image" --output "$part"
    mv "$part" "$image"
fi

actual=$(sha256sum "$image" | awk '{print $1}')
if [[ "$actual" != "$digest" ]]; then
    log "base image digest mismatch"
    log "  expected $digest"
    log "  actual   $actual"
    log "  image    $image"
    exit 1
fi
log "base image ok: $image ($digest)"

# --- Overlay, seed, keys ----------------------------------------------------
# The base image is never booted or written: every run gets a disposable
# overlay, so an interrupted run cannot corrupt the next one.
disk="$work_dir/$arch-disk.qcow2"
rm -f "$disk"
qemu-img create -q -f qcow2 -F qcow2 -b "$image" "$disk" "${disk_gb}G" >/dev/null

# --- Raise the guest's device-unit timeout in the overlay -------------------
# The guest inherits systemd's default 90 s device-unit timeout, and under
# emulation (arm64 in particular) udev coldplug is CPU-bound and can outlast it:
# the boot partition's device times out, local-fs.target fails by dependency,
# and the guest lands in emergency mode before SSH ever answers. Measured from
# the guest's own journal at 4 vCPUs on a nearly idle host: coldplug ran
# 223.0 -> 324.0 s while the timeout fired at 302.3 s.
#
# The timeout is settable only from the kernel cmdline or an in-image drop-in.
# The guest boots shim -> GRUB -> /grub/grub.cfg on the BOOT partition, and that
# config is reachable in the overlay without root, so the guest keeps its own
# boot path — AAVMF firmware, the distribution's shim and GRUB — rather than
# booting the kernel directly with -kernel/-append, which would bypass the
# firmware the real machine uses and test a path no user has.
#
# The BOOT partition is copied out of the overlay, edited, and copied back. The
# base image, whose digest is checked above, is never written; the overlay is
# deleted at the end of the run unless --keep.
grub_cfg_timeout_sec=600
partition_offsets=$(
    # Partition start and length, in 512-byte sectors, straight from the GPT:
    # hardcoding them would silently patch the wrong region on a re-published
    # image whose layout moved.
    gpt=$(mktemp)
    qemu-img dd -f qcow2 -O raw if="$disk" of="$gpt" bs=512 count=40 >/dev/null 2>&1
    python3 -I - "$gpt" <<'PY'
import struct, sys
with open(sys.argv[1], 'rb') as fh:
    data = fh.read()
entries_lba = struct.unpack('<Q', data[512 + 72:512 + 80])[0]
count = struct.unpack('<I', data[512 + 80:512 + 84])[0]
entry_size = struct.unpack('<I', data[512 + 84:512 + 88])[0]
base = entries_lba * 512
# The EFI System and XBOOTLDR type GUIDs. BOOT is the XBOOTLDR partition that
# holds /grub/grub.cfg; the root partition is neither, and copying 2.5 GiB out
# of the overlay to look for a file it cannot contain is pure waste.
ESPS = (b'\x28\x73\x2a\xc1\x1f\xf8\xd2\x11\xba\x4b\x00\xa0\xc9\x3e\xc9\x3b',
        b'\xff\xc2\x13\xbc\xe6\x59\x62\x42\xa3\x52\xb2\x75\xfd\x6f\x71\x72')
candidates = []
for index in range(count):
    entry = data[base + index * entry_size:base + (index + 1) * entry_size]
    if entry[:16] == b'\0' * 16:
        continue
    if entry[:16] not in ESPS:
        continue
    first, last = struct.unpack('<QQ', entry[32:48])
    candidates.append((last - first + 1, first))
# Largest first. On this image that is the 923 MiB XBOOTLDR BOOT partition, and
# the 99 MiB ESP that shares the same two type GUIDs is tried only if BOOT has
# no grub.cfg. The order matters: the ESP is vfat, and `debugfs` reports that on
# stdout while still exiting 0, so a smaller-first order makes the wrong
# partition look like a successful patch (see the verification below).
for length, first in sorted(candidates, reverse=True):
    print(first, length)
PY
    rm -f "$gpt"
)

boot_img=$(mktemp)
grub_cfg=$(mktemp)
grub_cfg_new=$(mktemp)

# The ESP and the BOOT partition share the EFI System / XBOOTLDR type GUIDs but
# not a filesystem: BOOT is ext4, the ESP is vfat. `debugfs` on a vfat image
# exits 0 and prints "<image> contains a vfat file system labelled 'UEFI'" on
# stdout, so its output is not evidence the file was read — the ext2/3/4
# superblock magic (0xEF53, at byte 1080) is checked instead, before anything is
# written and before debugfs is trusted to have failed.
is_ext4() {
    python3 -I - "$1" <<'PY'
import sys
with open(sys.argv[1], 'rb') as fh:
    fh.seek(1080)
    sys.exit(0 if fh.read(2) == b'\x53\xef' else 1)
PY
}

# The copy is done in 1 MiB blocks, not 512-byte sectors: a 923 MiB partition is
# 1.9 million sector reads through the qcow2 chain and takes minutes, while the
# same data in whole MiB takes seconds. GPT places every partition on a 1 MiB
# boundary, which is what makes the block-aligned skip valid; a layout that is
# not aligned is left unpatched rather than silently patched at the wrong offset.
sectors_per_mib=2048
patched=0
while read -r start length; do
    [[ -n "$start" ]] || continue
    if (( start % sectors_per_mib != 0 )); then
        log "partition at sector $start is not 1 MiB aligned; skipping the timeout edit"
        continue
    fi
    skip_mib=$(( start / sectors_per_mib ))
    # Read up to the next whole MiB so the file's tail is included, but write back
    # exactly the partition's length so the edit cannot spill into its neighbour.
    read_mib=$(( (length + sectors_per_mib - 1) / sectors_per_mib ))
    write_bytes=$(( length * 512 ))
    qemu-img dd -f qcow2 -O raw if="$disk" of="$boot_img" \
        bs=1M skip="$skip_mib" count="$read_mib" >/dev/null 2>&1
    is_ext4 "$boot_img" || continue
    debugfs -R 'cat /grub/grub.cfg' "$boot_img" >"$grub_cfg" 2>/dev/null || continue
    [[ -s "$grub_cfg" ]] || continue
    sed -E "s|^([[:space:]]*linux[[:space:]].*)\$|\1 systemd.default_device_timeout_sec=$grub_cfg_timeout_sec|" \
        "$grub_cfg" >"$grub_cfg_new"
    # A grub.cfg whose `linux` lines do not match leaves the sed output
    # byte-identical; writing that back would be a no-op reported as a patch.
    cmp -s "$grub_cfg" "$grub_cfg_new" && continue
    debugfs -w -R 'rm /grub/grub.cfg' "$boot_img" >/dev/null 2>&1
    debugfs -w -R "write $grub_cfg_new /grub/grub.cfg" "$boot_img" >/dev/null 2>&1
    # Read the edit back out of the image before it goes into the overlay: if the
    # write did not take, the overlay is left byte-identical to the base image.
    # Redirected to a file rather than piped to grep: `set -o pipefail` plus
    # `grep -q` closing the pipe early would turn a successful match into 141.
    debugfs -R 'cat /grub/grub.cfg' "$boot_img" >"$grub_cfg" 2>/dev/null || continue
    grep -qF "systemd.default_device_timeout_sec=$grub_cfg_timeout_sec" "$grub_cfg" || continue
    qemu-io -f qcow2 \
        -c "write -s $boot_img $((start * 512)) $write_bytes" \
        "$disk" >/dev/null 2>&1
    patched=1
    break
done <<<"$partition_offsets"
if [[ "$patched" -ne 1 ]]; then
    # Not a warning: without the raised timeout an emulated guest fails its
    # device units and boots to emergency mode, where SSH never answers. A run
    # that proceeds anyway spends its whole boot timeout to report a harness
    # defect as a guest failure.
    rm -f "$boot_img" "$grub_cfg" "$grub_cfg_new"
    log "could not patch an ext4 grub.cfg in the overlay: no EFI/XBOOTLDR partition carried one"
    log "  an emulated guest would time out its device units and land in emergency mode"
    exit 1
fi
log "raised the guest's device-unit timeout to ${grub_cfg_timeout_sec}s in the overlay"

key="$work_dir/$arch-id_ed25519"
rm -f "$key" "$key.pub"
ssh-keygen -q -t ed25519 -N '' -C ofi-clean-vm -f "$key"
public_key=$(cat "$key.pub")

seed_dir="$work_dir/$arch-seed"
rm -rf "$seed_dir"
mkdir -p "$seed_dir"
cat >"$seed_dir/user-data" <<EOF
#cloud-config
users:
  - name: ofi
    gecos: OpenForensic clean-VM guest
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    lock_passwd: true
    ssh_authorized_keys:
      - $public_key
ssh_pwauth: false
disable_root: true
package_update: false
EOF
cat >"$seed_dir/meta-data" <<EOF
instance-id: ofi-$arch-$(date -u +%Y%m%dT%H%M%SZ)
local-hostname: ofi-$arch
EOF
seed="$work_dir/$arch-seed.iso"
rm -f "$seed"
xorriso -as mkisofs -output "$seed" -volid cidata -joliet -rock \
    "$seed_dir/user-data" "$seed_dir/meta-data" >/dev/null 2>&1

# A high port is picked rather than a fixed one so two runs, or a stale run,
# do not silently share a forward. The port is only bound on the loopback.
port=$(shuf -i 20000-45000 -n 1)

# --- Firmware ---------------------------------------------------------------
qemu_args=()
case "$arch" in
    arm64)
        # AAVMF is the arm64 UEFI firmware. CODE is read-only and VARS must
        # stay writable, so the vars store is a copy per run.
        vars="$work_dir/$arch-vars.fd"
        rm -f "$vars"
        cp /usr/share/AAVMF/AAVMF_VARS.fd "$vars"
        qemu_args+=(
            -machine virt
            -cpu cortex-a72
            -drive "if=pflash,format=raw,readonly=on,file=/usr/share/AAVMF/AAVMF_CODE.fd"
            -drive "if=pflash,format=raw,file=$vars"
        )
        ;;
    amd64)
        qemu_args+=(-machine q35 -cpu max)
        ;;
esac

qemu_pid=
cleanup() {
    if [[ -n "$qemu_pid" ]] && kill -0 "$qemu_pid" 2>/dev/null; then
        kill "$qemu_pid" 2>/dev/null || true
        wait "$qemu_pid" 2>/dev/null || true
    fi
    # The grub.cfg patch's temporaries live here rather than in a trap of their
    # own: a later `trap ... EXIT` replaces an earlier one, so a second trap
    # would have leaked every one of these files.
    rm -f "$boot_img" "$grub_cfg" "$grub_cfg_new"
    if [[ "$keep" -eq 0 ]]; then
        # The overlay is the largest thing this script creates — tens of GiB at
        # a --disk-gb that fits a Rust build — and every run makes a fresh one.
        # Leaving it behind contradicted the note above the overlay creation,
        # which promises it is deleted unless --keep, and repeated runs quietly
        # filled the disk. `$disk` is guarded because an exit before the overlay
        # is created (a base-image digest mismatch) reaches this with it unset.
        [[ -n "${disk:-}" ]] && rm -f "$disk"
        rm -rf "$seed_dir" "$seed" "$key" "$key.pub" "$work_dir/$arch-vars.fd"
    fi
}
trap cleanup EXIT INT TERM

# --- Boot -------------------------------------------------------------------
# The guest's device-unit timeout has already been raised in the overlay (see
# above), so an emulated guest's coldplug can take its time without the boot
# failing. The warning below is for the residual case: a host so loaded that the
# guest gets almost no CPU at all, where even 600 s is not enough. Treat a
# timeout there as inconclusive rather than as an artifact failure.
host_cpus=$(nproc 2>/dev/null || echo 1)
load_scaled=$(awk '{printf "%d", $1*100}' /proc/loadavg 2>/dev/null || echo 0)
if [[ "$load_scaled" -ge "$((host_cpus * 75))" ]]; then
    log "WARNING: host load average is $((load_scaled / 100)).$((load_scaled % 100)) over ${host_cpus} CPUs"
    log "  an emulated guest under this load may miss udev/device timeouts and boot to emergency mode"
    log "  the guest is still booted; treat a timeout as inconclusive rather than as an artifact failure"
fi

log "booting $qemu_binary (ssh on 127.0.0.1:$port, serial -> $serial_log)"
"$qemu_binary" \
    "${qemu_args[@]}" \
    -smp "$cpus" -m "$memory_mb" \
    -drive "if=virtio,format=qcow2,file=$disk" \
    -drive "if=virtio,format=raw,readonly=on,file=$seed" \
    -netdev "user,id=n0,hostfwd=tcp:127.0.0.1:$port-:22" \
    -device virtio-net-pci,netdev=n0 \
    -display none -monitor none \
    -serial "file:$serial_log" \
    >>"$console_log" 2>&1 &
qemu_pid=$!

ssh_opts=(
    -o StrictHostKeyChecking=no
    -o UserKnownHostsFile=/dev/null
    -o BatchMode=yes
    -o ConnectTimeout=10
    -o LogLevel=ERROR
    -o ServerAliveInterval=30
    -o ServerAliveCountMax=20
    -p "$port"
    -i "$key"
)
guest() { ssh "${ssh_opts[@]}" ofi@127.0.0.1 "$@"; }

# `-P`, not ssh's `-p`: the two tools spell the port differently, and scp reads a
# lowercase `-p` as "preserve times", so passing ssh_opts here made the port
# number a source file ("cannot stat '38320'") and the copy failed before it
# connected. Only the port differs between the two invocations; the rest of
# ssh_opts applies.
scp_opts=()
for opt in "${ssh_opts[@]}"; do
    case "$opt" in
        -p) scp_opts+=(-P) ;;
        *) scp_opts+=("$opt") ;;
    esac
done

# Wait for SSH, then for cloud-init. The two are separate because SSH answering
# is not the same as the guest being configured: cloud-init's user and key
# setup is what makes the login work at all, and its remaining units can still
# be running when the port opens.
deadline=$((SECONDS + boot_timeout))
until guest true >/dev/null 2>&1; do
    if ! kill -0 "$qemu_pid" 2>/dev/null; then
        log "qemu exited before SSH came up; last serial output:"
        tail -n 40 "$serial_log" | tee -a "$console_log"
        exit 1
    fi
    if [[ "$SECONDS" -ge "$deadline" ]]; then
        log "timed out after ${boot_timeout}s waiting for SSH; last serial output:"
        tail -n 40 "$serial_log" | tee -a "$console_log"
        exit 1
    fi
    sleep 5
done
log "ssh is up after $((SECONDS))s from boot start"

log "waiting for cloud-init to finish"
# The ssh invocation is repeated rather than wrapped as `timeout ... guest ...`:
# `timeout` execs a program, `guest` is a shell function, and the first run to
# reach this line died here with "failed to execute process" (exit 127) — every
# earlier failure had stopped at the boot, so the line had never run. Same
# budget as the SSH wait: cloud-init's own units are the slow part of an
# emulated first boot, not the port opening.
if ! timeout "$boot_timeout" ssh "${ssh_opts[@]}" ofi@127.0.0.1 'sudo cloud-init status --wait'; then
    log "cloud-init did not report success"
    exit 1
fi
log "guest ready: $(guest 'lsb_release -ds 2>/dev/null || . /etc/os-release && echo "$PRETTY_NAME"')"
log "guest kernel: $(guest uname -r)"
log "guest arch:   $(guest uname -m)"

run_in_guest() {
    local label=$1
    shift
    log "--- guest: $label"
    local status=0
    if [[ "$work_timeout" -gt 0 ]]; then
        # `timeout` execs a program, and `guest` is a shell function, so the
        # ssh invocation is repeated here rather than wrapping `guest`.
        timeout "$work_timeout" ssh "${ssh_opts[@]}" ofi@127.0.0.1 "$@" 2>&1 \
            | tee -a "$console_log" || status=${PIPESTATUS[0]}
    else
        guest "$@" 2>&1 | tee -a "$console_log" || status=${PIPESTATUS[0]}
    fi
    if [[ "$status" -eq 124 ]]; then
        log "--- guest: $label STOPPED at the ${work_timeout}s timeout"
        log "an emulated guest is slow, and a run stopped at a deadline is incomplete, not a pass"
    fi
    return "$status"
}

# The native libraries the guest provides, and their versions: this is the arm64
# answer to "which TSK and libewf does the distribution ship", which the
# distribution matrix records for the other four. It is bounded because dpkg's
# own database read can stall behind an unresponsive guest; this is a report,
# so a timeout here must not fail the run.
#
# It is a function because *when* it runs decides what it reports. In `boot`
# mode the stock image is the answer, so it runs before the mode dispatch. In
# `build` mode the artifacts link against the libraries the build arm installs,
# so reporting before that install would describe a guest that ships none of
# them and print no rows — the question asked before the answer exists. The
# build arm therefore calls it again after installing.
report_native_dependencies() {
    run_in_guest "native dependency versions" \
        "dpkg-query -W -f='\${db:Status-Abbrev}|\${Package}|\${Version}\n' 'libtsk*' 'libewf*' libqt6core6t64 qt6-base-dev 2>/dev/null | awk -F'|' '\$1 ~ /^ii/ { print \$2, \$3 }' | sort" \
        || true
}

if [[ "$mode" == boot ]]; then
    report_native_dependencies
fi

# Assert once that the guest really is the architecture under test. A guest that
# came up as the host's architecture would make every later result meaningless,
# and it is the one thing an emulated run can silently get wrong.
guest_arch=$(guest uname -m)
log "asserting guest is the requested architecture"
case "$arch" in
    arm64) expected_guest_arch=aarch64 ;;
    amd64) expected_guest_arch=x86_64 ;;
esac
if [[ "$guest_arch" != "$expected_guest_arch" ]]; then
    log "guest reports uname -m = $guest_arch, expected $expected_guest_arch"
    exit 1
fi
log "guest is $guest_arch, as requested"

# The guest's mounts are the harness's own success condition, not a
# configuration detail: `cloudimg-rootfs` resolving proves nothing about
# `/boot` and `/boot/efi`, and a guest that missed them is in emergency mode
# with a partially configured system. This is what the first boot here failed.
log "asserting the guest's local filesystems are mounted"
# Two separate `findmnt` runs, not one with two `--mountpoint` options: findmnt
# evaluates only the last one (util-linux 2.41.3), so `--mountpoint /boot
# --mountpoint /boot/efi` checked /boot/efi alone and passed on exactly the
# emergency-mode guest this assertion exists to catch.
if ! guest 'findmnt --mountpoint /boot >/dev/null && findmnt --mountpoint /boot/efi >/dev/null'; then
    log "the guest did not mount /boot and /boot/efi"
    guest 'systemctl --failed --no-pager' 2>&1 | tee -a "$console_log" || true
    exit 1
fi
log "guest /boot and /boot/efi are mounted"

status=0
case "$mode" in
    boot)
        # Nothing to do: the identity lines above are the result. This mode
        # exists so the harness can be proven on its own, which is the first
        # half of C5 step 1; a build failure in `build` mode should not be
        # confused with a guest that never came up.
        ;;
    build)
        # The workspace is copied in over SSH rather than shared with virtfs:
        # a read-only share cannot hold `target/`, and a writable share lets a
        # bug in the guest write into the working tree.
        log "copying the workspace into the guest"
        # The excludes are the difference between a 3 GiB and a 0.5 GiB
        # transfer, and this copy crosses SSH into a TCG-emulated guest, so it
        # is the slowest part of a build run that is not the build itself.
        # `tests/fuzz/target` is a cargo target directory like `./target` and is
        # excluded for the same reason. `build/` and `FTK-Imager-*/` are
        # untracked developer state: the first is the host's CMake tree, whose
        # object files would be the host architecture and are never read in the
        # guest, and the second is a supplied installer that must not be copied
        # off this host. An allow-list is used instead of more --exclude lines
        # because a deny-list misses the next large directory someone creates,
        # and what the guest build needs is exactly the tracked tree.
        # Guarded rather than left as a bare pipeline. Under `set -o pipefail`
        # either side failing aborts the script, which skips the `clean-vm …
        # FAILED` verdict line at the bottom and leaves a non-zero exit whose
        # only explanation is the absence of a log line. Catching the status
        # here turns a failed copy into the same verdict every other step
        # produces.
        git -C "$repo_root" ls-files -z | tar -C "$repo_root" -cf - --null -T - \
            | guest 'mkdir -p ~/openforensic-imager && tar -C ~/openforensic-imager -xf -' \
            && status=0 || status=$?
        # `$status` from a failed copy must stop the install step below rather
        # than being overwritten by it, so the whole arm is gated on it.
        if [[ "$status" -eq 0 ]]; then
            run_in_guest "install build dependencies" \
                'sudo apt-get update && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates build-essential cmake ninja-build pkg-config curl git file libtsk-dev libewf-dev qt6-base-dev qt6-declarative-dev qt6-svg-dev e2fsprogs ewf-tools mtools dosfstools ntfs-3g exfatprogs strace util-linux gdisk fdisk imagemagick desktop-file-utils' \
                && status=0 || status=$?
            # Now that the libraries the artifacts link against are installed,
            # this reports the dependency set that matters rather than the stock
            # image's, which ships none of them.
            report_native_dependencies
        fi
        if [[ "$status" -eq 0 ]]; then
            # The Ubuntu cloud image ships no Rust, and the workspace pins a
            # toolchain in rust-toolchain.toml, so rustup is the way to get the
            # exact version rather than whatever the distribution packages. It
            # installs into the login user's home, so the PATH is set per call
            # rather than relying on a profile file a non-interactive SSH
            # session would not read.
            run_in_guest "install the pinned Rust toolchain" \
                'export RUSTUP_HOME=$HOME/.rustup CARGO_HOME=$HOME/.cargo; curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.93.1 --component rustfmt --component clippy && $HOME/.cargo/bin/rustc --version && $HOME/.cargo/bin/cargo --version' \
                && status=0 || status=$?
        fi
        # `cargo` is not on the default PATH for a non-interactive SSH command;
        # the install step wrote it to ~/.cargo/bin, so every later command
        # exports it. This is the same reason the guest has no login profile.
        cargo_path='export PATH=$HOME/.cargo/bin:$PATH'
        if [[ "$status" -eq 0 ]]; then
            run_in_guest "cargo test --workspace" \
                "cd ~/openforensic-imager && $cargo_path && export OFI_TEST_DEADLINE_SCALE=$deadline_scale && mkdir -p packaging/build && cargo test --workspace >packaging/build/vm-workspace.log 2>&1; s=\$?; tail -n 20 packaging/build/vm-workspace.log; exit \$s" \
                && status=0 || status=$?
        fi
        if [[ "$status" -eq 0 ]]; then
            run_in_guest "package-deb" \
                "cd ~/openforensic-imager && $cargo_path && scripts/package-deb.sh" \
                && status=0 || status=$?
        fi
        if [[ "$status" -eq 0 ]]; then
            run_in_guest "package-appimage" \
                "cd ~/openforensic-imager && $cargo_path && scripts/package-appimage.sh --skip-build" \
                && status=0 || status=$?
        fi
        if [[ "$status" -eq 0 ]]; then
            # Guarded like its neighbours: without the `|| status=$?`, a failed
            # `sha256sum` here would abort the script under `set -euo pipefail`
            # with a non-zero exit but no `clean-vm … FAILED` verdict line, so
            # the run would read as a crash rather than as a failed check.
            run_in_guest "artifact digests" \
                'cd ~/openforensic-imager && sha256sum packaging/output/*' \
                && status=0 || status=$?
        fi
        # The artifacts exist only in the guest, and the guest is disposable: a
        # digest printed into a log is not an artifact, so a run that stopped
        # here could not satisfy "copy the arm64 artifacts out". They are
        # copied into a directory of their own so the host's own amd64 builds
        # under packaging/output cannot be confused with them.
        if [[ "$status" -eq 0 ]]; then
            copy_dir="$work_dir/$arch-artifacts"
            rm -rf "$copy_dir"
            mkdir -p "$copy_dir"
            log "--- guest: copy the artifacts out"
            scp -q "${scp_opts[@]}" \
                'ofi@127.0.0.1:~/openforensic-imager/packaging/output/*' \
                "$copy_dir/" 2>&1 | tee -a "$console_log" || status=${PIPESTATUS[0]}
        fi
        if [[ "$status" -eq 0 ]]; then
            # Every packaging script writes a .sha256 beside its artifact, and
            # those files were produced inside the guest. Checking them here is
            # what makes the copy trustworthy: it proves the bytes the host now
            # holds are the bytes the guest built, rather than a truncated or
            # re-encoded transfer. `shopt -s nullglob` because a glob that
            # matches nothing is passed through literally by bash and would make
            # `sha256sum -c` fail on a filename rather than report no checks.
            shopt -s nullglob
            checks=("$copy_dir"/*.sha256)
            shopt -u nullglob
            if [[ "${#checks[@]}" -eq 0 ]]; then
                log "the guest produced no .sha256 file; the copy cannot be verified"
                status=1
            else
                ( cd "$copy_dir" && sha256sum -c ./*.sha256 ) 2>&1 \
                    | tee -a "$console_log" || status=${PIPESTATUS[0]}
            fi
        fi
        if [[ "$status" -eq 0 ]]; then
            log "artifacts copied to $copy_dir and verified against the guest's own digests"
        fi
        # Step 4 of the plan ticks the ARM64 item only if the packages install
        # and self-test inside the guest, so the run proves that itself rather
        # than leaving it to a later manual pass over a guest that no longer
        # exists. Done here because this is the only place the artifact and the
        # clean system coexist. `./` matters: without it apt reads the argument
        # as a package name rather than a path to a .deb.
        if [[ "$status" -eq 0 ]]; then
            run_in_guest "install the built package and self-test" \
                'cd ~/openforensic-imager && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y ./packaging/output/*.deb && openforensic-imager --self-test && openforensic-imager --dark --self-test' \
                && status=0 || status=$?
        fi
        ;;
    install)
        if [[ -z "$artifact" ]]; then
            artifact=$(ls -t "$repo_root"/packaging/output/*.deb 2>/dev/null | head -1 || true)
        fi
        [[ -n "$artifact" && -f "$artifact" ]] \
            || { log "no artifact to install; pass --artifact or build one first"; exit 2; }
        log "installing $(basename "$artifact")"
        guest 'mkdir -p ~/artifact'
        scp -q "${scp_opts[@]}" "$artifact" ofi@127.0.0.1:~/artifact/ || { log "scp failed"; exit 1; }
        run_in_guest "install the package and self-test" \
            "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y ./artifact/$(basename "$artifact") && openforensic-imager --self-test && openforensic-imager --dark --self-test" \
            && status=0 || status=$?
        ;;
esac

if [[ "$status" -eq 0 ]]; then
    log "clean-vm $arch $mode PASSED"
else
    log "clean-vm $arch $mode FAILED (status $status)"
    log "serial log: $serial_log"
fi

if [[ "$keep" -eq 1 ]]; then
    log "work directory kept: $work_dir"
fi

exit "$status"
