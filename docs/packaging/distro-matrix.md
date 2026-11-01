# Distribution matrix evidence

Status: all four target distributions PASSED on 2026-11-01. This document is the
evidence record for the Phase 9 claim that the tree builds and tests on
Ubuntu, Debian, Arch, and Fedora, and the decision record for what the matrix
does and does not establish.

Reproduce with:

```
scripts/check-distro-build.sh
```

The script builds in `ubuntu:26.04`, `debian:trixie`, `archlinux:latest`, and
`fedora:latest` in turn. Each container gets that distribution's build
toolchain, Qt 6 development packages, and its packaged TSK and libewf, then runs
`cargo test --workspace` and `cmake -G Ninja -S apps/desktop -B /tmp/build`.
Logs are written to `packaging/build/distro/` (git-ignored).

## Result

| Distribution | `cargo test --workspace` | `cmake configure apps/desktop` | Log |
|---|---|---|---|
| `ubuntu:26.04` | 70 suites, 182 passed, 0 failed | exit 0 | `ubuntu_26.04.log` |
| `debian:trixie` | 70 suites, 182 passed, 0 failed | exit 0 | `debian_trixie.log` |
| `archlinux:latest` | 70 suites, 182 passed, 0 failed | exit 0 | `archlinux_latest.log` |
| `fedora:latest` | 70 suites, 182 passed, 0 failed | exit 0 | `fedora_latest.log` |

The counts are identical to the reference host, so no distribution needed a
test skipped, a feature disabled, or a package substituted.

## Native library versions

The matrix is also the measurement of which TSK and libewf each distribution
actually ships, because the tree links both.

| Distribution | TSK | libewf | rustc |
|---|---|---|---|
| `ubuntu:26.04` | `libtsk-dev 4.12.1+dfsg-3build1` | `libewf-dev 20140816-2build1` | 1.93.1 |
| `debian:trixie` | `libtsk-dev 4.12.1+dfsg-3` | `libewf-dev 20140816-1+b1` | 1.93.1 |
| `archlinux:latest` | `sleuthkit 4.15.0-1` | `libewf 20140816-1` | 1.93.1 |
| `fedora:latest` | `sleuthkit-devel 4.15.0-2.fc44` | `libewf-devel 20140608-33.fc44` | 1.93.1 |

Two facts fall out, and both are decisions this matrix was run to support.

**TSK differs by a minor series.** Debian and Ubuntu carry the 4.12 line;
Arch and Fedora carry 4.15. The suite passes on both, which is the evidence
that the partition and filesystem code does not depend on a behavior that
changed between them. A future failure that appears on only one side of this
divide should be read as a TSK version difference first, not as a packaging
problem — that is what the split looks like here.

**libewf is the same legacy line everywhere.** Every distribution packages
20140816 or older (Fedora is 20140608), which is `README.md`'s claim under
"E01 acquisition requires a pinned, tested build of libewf" — the distribution
packages are old, and the pinned build is the supported path for E01 work. The
matrix is independent confirmation of that sentence: it is not one
distribution lagging, it is all four.

## What this establishes

- The workspace builds and its whole test suite passes on the four
  distributions the project claims to support.
- The Qt desktop configures against each distribution's Qt 6 without a
  distribution-specific CMake branch.
- The TSK 4.12 / 4.15 split and the uniform legacy libewf are measured, not
  assumed.

## What this does not establish

- **Not a clean-machine test.** Each container installs the build dependencies
  first. A distribution's *runtime* dependencies are exercised by the package
  artifacts, not here; the DEB, AppImage, and Flatpak are checked by
  `scripts/verify-reproducible-packages.sh` and their own artifact tests.
- **No distribution-version coverage beyond the named tags.** `ubuntu:26.04`
  and `debian:trixie` are the current releases at the time of writing;
  `archlinux:latest` and `fedora:latest` are rolling and will drift. A re-run
  that fails should be dated before it is treated as a regression.
- **No arm64 and no VM.** This is x86_64 in a container, not a virtual
  machine, so it says nothing about kernel-version-dependent behavior —
  Landlock, seccomp, and the resource-limit paths are exercised by the host
  suite, not here.
- **The desktop is configured, not built or launched.** `cmake` configure is
  the step the plan names; a full Qt build and a headless launch are not part
  of this matrix.

## Package name differences

Three names differ from the Debian spelling, and getting them wrong is silent:
on Arch a single unknown name makes `pacman` abort the batch with exit 1 while
having already installed the earlier names in it, so a wrong `gdisk`/`fdisk`
dropped most of the set without a clear error.

| Tool | Debian/Ubuntu | Arch | Fedora |
|---|---|---|---|
| `sgdisk`, `sfdisk` | `gdisk`, `util-linux` | `gptfdisk`, `util-linux` | `gdisk`, `util-linux` |
| EWF tools (`ewfacquire`, `ewfverify`) | `libewf-dev` | `libewf` | `libewf-tools` |
| NTFS tools (`mkntfs`, `ntfscp`) | `ntfs-3g` | `ntfsprogs` | `ntfsprogs` |

On Arch, `ntfs-3g` is the driver package and does not contain `mkntfs`;
`ntfsprogs` is the one that does. The test suite shells out to the filesystem
creation tools (`mke2fs`, `debugfs`, `mkfs.vfat`, `mcopy`, `mkntfs`, `ntfscp`,
`mkfs.exfat`) and to `ewfacquire`/`ewfverify`, so a missing name surfaces as a
raw `NotFound` from a fixture rather than as a packaging error — which is why
each name above was verified by installing it and confirming the binary exists,
not by reading documentation.
