# Reproducible Build Instructions

Status: Phase 8, Ubuntu 26.04 x86_64.

The reproducibility claim is limited to the tested host and package set. It does
not claim bit-identical output across distributions, Qt versions, compiler
versions, or native library versions.

## Build dependencies

The currently tested Ubuntu 26.04 package set is:

```bash
sudo apt install \
  build-essential cmake ninja-build cargo rustc \
  qt6-base-dev qt6-declarative-dev qt6-svg-dev \
  libtsk-dev libewf-dev libglx-dev libvulkan-dev \
  dpkg-dev fakeroot squashfs-tools imagemagick \
  desktop-file-utils appstream jq curl flatpak
```

Install the Flatpak SDK when building the analysis bundle:

```bash
flatpak install org.kde.Sdk//6.10 org.kde.Platform//6.10
```

The Rust toolchain is pinned by `rust-toolchain.toml` to 1.93.1. Cargo
dependencies are pinned by `Cargo.lock`. The AppImage tool URL and SHA-256 are
pinned in `packaging/tools.lock`.

## Build all packages

```bash
scripts/package-deb.sh
scripts/package-appimage.sh --skip-build
scripts/package-flatpak.sh --skip-build
scripts/generate-sbom.sh \
  packaging/output/openforensic-imager-0.1.0.cdx.json \
  packaging/output/openforensic-imager_0.1.0_amd64.deb \
  packaging/output/OpenForensic-Imager-0.1.0-x86_64.AppImage
scripts/test-phase7.sh
```

Run the Flatpak test before or after `test-phase7.sh`. `test-phase7.sh` includes
the Flatpak self-test and an isolated user installation when the Flatpak build
tree exists.

## Digest and size comparison

The reproducibility test performs two clean CMake and Cargo builds in separate
directories. Each package script reports the artifact size and SHA-256 that it
created; the runner requires both values to match:

```bash
SOURCE_DATE_EPOCH=1793318400 scripts/verify-reproducible-packages.sh
```

`1793318400` is `2026-10-30T00:00:00Z`, the fixed timestamp used by the Phase 7
artifacts. The script also sets `CARGO_INCREMENTAL=0`.

The Release desktop target omits the GNU build ID. Before installation, the
executable records a build-directory-specific RPATH; the install step replaces
that RPATH, but a linker-generated build ID would preserve entropy from the
discarded path. No debug symbols are shipped in the current development
packages.

The comparison covers DEB and AppImage only. The Flatpak repository and bundle
are functionally tested but are not yet part of the reproducibility claim. The
SBOM is generated from the final artifacts but is not separately compared across
two builds.

## Test the generated artifacts

```bash
scripts/test-phase7.sh
```

The test validates desktop metadata, AppStream metadata, package contents,
shared-library resolution, Qt platform plugins, QML modules, native bindings,
the application self-test, SBOM structure, checksum files, and an isolated
Flatpak user installation.

## Release recording

Record all of the following with a release:

- Git commit identifier.
- `SOURCE_DATE_EPOCH`.
- Operating-system image or clean-VM identifier.
- Rust, Cargo, Qt, CMake, Ninja, TSK, libewf, and Flatpak versions.
- SHA-256 for every artifact.
- SBOM and its SHA-256.
- Test command output.

Do not publish a reproducibility claim when any of these inputs differ and the
result has not been rebuilt and compared.
