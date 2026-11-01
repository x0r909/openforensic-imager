#!/usr/bin/env bash
set -euo pipefail

# Does this source tree build and test on a distribution other than the
# reference host? Answer that with a container per distribution rather than a
# guess: install that distribution's build dependencies and TSK/libewf
# packages, then run the Rust workspace tests and the desktop CMake configure.
#
# The repository is bind-mounted read-only, so a run cannot modify it. Each
# distribution gets a writable copy inside the container for cargo's target
# directory and CMake's build tree.
#
# This script records what happened. It does not promise a native package for
# every distribution it can reach; docs/packaging/distro-matrix.md carries that
# decision.

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

output_dir=${OFI_DISTRO_OUTPUT_DIR:-"$repo_root/packaging/build/distro"}
rust_toolchain=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' \
    "$repo_root/rust-toolchain.toml")
if [[ -z "$rust_toolchain" ]]; then
    printf 'cannot read the pinned toolchain from rust-toolchain.toml\n' >&2
    exit 1
fi

images=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        --image)
            images+=("$2")
            shift 2
            ;;
        --help|-h)
            printf 'Usage: %s [--image <docker-image>]...\n' "$(basename "$0")"
            exit 0
            ;;
        *)
            printf 'unknown option: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done
if [[ ${#images[@]} -eq 0 ]]; then
    images=(ubuntu:26.04 debian:trixie archlinux:latest fedora:latest)
fi

# One entry per distribution: the package-manager invocation that installs the
# toolchain, the two native libraries, and the external tools the test suite
# shells out to. Package names differ enough between these four that guessing a
# common set would produce a failure that says nothing about the code.
#
# The test suite is not self-contained: it runs mkfs and friends to build the
# images it then parses, and asks libewf's own tool to verify the EWF image it
# produced. A distribution missing any of those fails a test whose failure mode
# (`NotFound`) reads like a bug in the crate. The tools are therefore part of
# what "builds and tests on this distribution" means, and each set below was
# checked by installing it and confirming every name resolves to a binary.
distro_packages() {
    case "$1" in
        ubuntu:*|debian:*)
            printf '%s\n' \
                'apt-get update' \
                'DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates build-essential cmake ninja-build pkg-config curl git file libtsk-dev libewf-dev qt6-base-dev qt6-declarative-dev qt6-svg-dev e2fsprogs ewf-tools mtools dosfstools ntfs-3g exfatprogs strace util-linux gdisk fdisk'
            ;;
        archlinux:*)
            # `gptfdisk` and `util-linux` carry sgdisk and sfdisk here; the
            # Debian names for those packages (`gdisk`, `fdisk`) do not exist,
            # and a single unknown name makes pacman abort the whole batch.
            printf '%s\n' \
                'pacman -Sy --noconfirm --needed ca-certificates base-devel cmake ninja pkgconf curl git file sleuthkit libewf qt6-base qt6-declarative qt6-svg e2fsprogs mtools dosfstools ntfs-3g ntfsprogs exfatprogs strace gptfdisk util-linux'
            ;;
        fedora:*)
            printf '%s\n' \
                'dnf install -y ca-certificates gcc gcc-c++ cmake ninja-build pkgconf-pkg-config curl git file sleuthkit-devel libewf-devel qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtsvg-devel e2fsprogs libewf-tools mtools dosfstools ntfsprogs exfatprogs strace util-linux gdisk'
            ;;
        *)
            printf '' ;;
    esac
}

# rustup is the only toolchain installer all four images can share, and the
# pinned version is what the repository builds with. The distro's own rust
# package is an older stable on every one of these images.
rust_setup() {
    cat <<EOF
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none
. "\$HOME/.cargo/env"
rustup toolchain install $rust_toolchain --profile minimal
rustup default $rust_toolchain
rustc --version
cargo --version
EOF
}

in_container() {
    cat <<'EOF'
set -x
mkdir -p /work
# The source is read-only, so build inside a copy. Exclude the host's build
# outputs: they are large, architecture-specific, and would make the result say
# something about this machine rather than about the distribution.
tar -C /src --exclude=./target --exclude=./build \
    --exclude=./packaging/build --exclude=./.git -cf - . \
    | tar -C /work -xf -
cd /work
export CARGO_TARGET_DIR=/work/target
export CARGO_INCREMENTAL=0
export CARGO_NET_GIT_FETCH_WITH_CLI=true

echo "OFI_STEP cargo test --workspace"
cargo test --workspace
echo "OFI_STEP cargo test --workspace: exit 0"

echo "OFI_STEP cmake configure apps/desktop"
cmake -G Ninja -S apps/desktop -B /work/build
echo "OFI_STEP cmake configure apps/desktop: exit 0"
EOF
}

mkdir -p "$output_dir"
declare -a summary=()
overall=0

for image in "${images[@]}"; do
    slug=${image//[:\/]/_}
    log="$output_dir/$slug.log"
    packages=$(distro_packages "$image")
    if [[ -z "$packages" ]]; then
        printf '%s: no package set defined, skipping\n' "$image" >&2
        summary+=("$image: not run (no package set)")
        continue
    fi

    printf 'Building on %s ...\n' "$image"
    script=$(
        printf 'set -e\n'
        printf '%s\n' "$packages"
        rust_setup
        in_container
    )
    # `timeout` bounds a distribution whose package manager hangs on a missing
    # mirror rather than failing. The log is kept either way.
    if timeout 5400 docker run --rm \
        --mount "type=bind,source=$repo_root,target=/src,readonly" \
        "$image" bash -c "$script" >"$log" 2>&1; then
        summary+=("$image: PASSED (cargo test --workspace and cmake configure)")
        printf '%s: PASSED\n' "$image"
    else
        status=$?
        overall=1
        summary+=("$image: FAILED (exit $status; see $log)")
        printf '%s: FAILED (exit %s)\n' "$image" "$status"
    fi
done

printf '\nDistribution matrix summary\n'
printf '%s\n' "${summary[@]}"
printf '\nLogs under %s\n' "$output_dir"
exit "$overall"
