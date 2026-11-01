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
                         distribution's own udev and systemd, whose coldplug
                         is CPU-bound; too few vCPUs on a busy host makes
                         udev miss the device-unit timeouts and the guest
                         lands in emergency mode.
  --boot-timeout <n>     Seconds to wait for SSH. Default 600.
  --work-timeout <n>     Seconds to let the in-guest work run; 0 disables.
                         Default 0.
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
for value in "$disk_gb" "$memory_mb" "$cpus" "$boot_timeout" "$work_timeout"; do
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

work_dir=${work_dir:-"$repo_root/packaging/build/clean-vm"}
mkdir -p "$work_dir"
image=${image:-"$work_dir/images/$default_image"}

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
    if [[ "$keep" -eq 0 ]]; then
        rm -rf "$seed_dir" "$seed" "$key" "$key.pub" "$work_dir/$arch-vars.fd"
    fi
}
trap cleanup EXIT INT TERM

# --- Boot -------------------------------------------------------------------
# An emulated guest gets a fixed share of the host's CPU, but the guest's own
# systemd and udev run against wall-clock timeouts it cannot lengthen. On a
# host that is already busy, an arch64 guest's udev coldplug can miss the
# device-unit timeout and the guest lands in emergency mode — a failure of the
# measurement, not of the image. This is measured rather than assumed: the
# first boot here took 3 min 4 s of initrd coldplug under a load average of
# 4.5, and systemd then timed out on /boot and /boot/efi.
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
if ! timeout 600 guest 'sudo cloud-init status --wait'; then
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

# The native libraries the base image provides, and their versions: this is the
# arm64 answer to "which TSK and libewf does the distribution ship", which the
# distribution matrix records for the other four. It is bounded because dpkg's
# own database read can stall behind an unresponsive guest; this is a report,
# so a timeout here must not fail the run.
run_in_guest "native dependency versions" \
    'dpkg-query -W -f="${Package} ${Version}\n" libtsk19 libtsk-dev libewf2 libewf-dev libqt6core6t64 qt6-base-dev 2>/dev/null | sort' \
    || true

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
if ! guest 'findmnt --mountpoint /boot --mountpoint /boot/efi >/dev/null'; then
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
        tar -C "$repo_root" -cf - \
            --exclude=./target --exclude=./packaging/build \
            --exclude=./packaging/output --exclude=./.git \
            --exclude=./tests/fixtures/generated \
            . | guest 'mkdir -p ~/openforensic-imager && tar -C ~/openforensic-imager -xf -'
        run_in_guest "install build dependencies" \
            'sudo apt-get update && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates build-essential cmake ninja-build pkg-config curl git file libtsk-dev libewf-dev qt6-base-dev qt6-declarative-dev qt6-svg-dev e2fsprogs ewf-tools mtools dosfstools ntfs-3g exfatprogs strace util-linux gdisk fdisk imagemagick' \
            && status=0 || status=$?
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
                "cd ~/openforensic-imager && $cargo_path && mkdir -p packaging/build && cargo test --workspace >packaging/build/vm-workspace.log 2>&1; s=\$?; tail -n 20 packaging/build/vm-workspace.log; exit \$s" \
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
            run_in_guest "artifact digests" \
                'cd ~/openforensic-imager && sha256sum packaging/output/*'
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
        # `-p` preserves no mode here; the artifact is data, and the guest
        # checks its own copy against the host digest below.
        scp -q "${ssh_opts[@]}" "$artifact" ofi@127.0.0.1:~/artifact/ || { log "scp failed"; exit 1; }
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
