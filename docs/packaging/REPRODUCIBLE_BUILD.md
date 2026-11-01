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

The comparison covers the DEB, the AppImage, and the SBOM that ships inside the
DEB, by byte identity: same size and same SHA-256 in both build sets.

The Flatpak is compared by content, not by bytes. A bundle is not
byte-reproducible by design — `flatpak build-export` records a timestamp per
exported object and `build-bundle` writes a summary with its own mtime — so a
byte comparison would fail for reasons that have nothing to do with the build.
What the runner compares instead is the exported app content: the `metadata`
file `build-finish` writes (permissions, finish args, command) and a SHA-256
manifest of every path under `files/`, symlinks recorded by their targets. This
is weaker than a byte comparison and is stated as such: two bundles that differ
only in an archive mtime pass, and two that differ in one shipped byte fail. The
runner prints the content digests and the reason for the bundle-level exemption
on every run, so a passing result never reads as more than it is.

## Where the parser worker finds TSK and libewf

`ofi-parser-worker` links libtsk and libewf, and the desktop starts it with
`.env_clear()` plus an allow-list that deliberately excludes
`LD_LIBRARY_PATH` — an environment variable reaching the worker is an
environment variable a compromised worker could have been given. The worker
therefore cannot inherit a search path, and `crates/native-bindings/build.rs`
gives it a `DT_RPATH` of `$ORIGIN`, `$ORIGIN/../lib`, and
`$ORIGIN/../lib/openforensic-imager`.

It has to be `DT_RPATH` and not the modern `DT_RUNPATH`: a run path applies
only to an object's own direct dependencies, while an rpath is inherited by
everything loaded beneath it. The worker needs libtsk and libewf directly, but
libtsk in turn needs libvmdk, libvhdi, and libafflib. Under a run path the
worker resolves the first two and then fails on the three that live one level
further out.

An rpath is searched before `LD_LIBRARY_PATH` and before the system paths, so
where a directory exists its copy wins. This is what lets each artifact resolve
the right thing:

- the AppImage and the Flatpak bundle the closure and resolve it from their own
  tree, which is a requirement rather than a preference — the Flatpak sandbox
  has no system libtsk or libewf at all, and an AppImage on a clean host has no
  host copy to fall back to;
- the DEB bundles none of them, so all three rpath directories are empty and are
  skipped, and it resolves the distribution's packages, which is what its
  `Depends` field names. The same binary serves both layouts.

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
