# OpenForensic Imager

Linux-native digital forensics acquisition and evidence analysis platform, written in Rust with a Qt 6 desktop shell.

OpenForensic Imager acquires disk images and analyzes evidence on Linux. It reads RAW and EWF1 E01 images, discovers partitions and filesystems through The Sleuth Kit, and runs every evidence-facing job inside an isolated sandboxed worker process. It is free software released under GPL-3.0-or-later.

## Status

Phases 0 through 8 are implemented and tested at the local development level. The supported features are documented below. The production release gate is not yet passed: clean-VM, secondary-distribution, lintian, and physical-device tests have not run, and the DEB and AppImage currently resolve host versions of TSK and libewf rather than the pinned release build described in the architecture. See the [implementation plan](IMPLEMENTATION_PLAN.md) for the roadmap.

## Features

Imaging and verification:

- RAW/DD acquisition with staged output and a commit manifest.
- EWF1 E01 acquisition through libewf, including segmented output.
- Multi-segment E01 random-access reading.
- MD5, SHA-1, SHA-256, and SHA-512 acquisition hashes.
- Post-acquisition verification with explicit match, mismatch, and unavailable outcomes.
- Cancellation and unreadable-range accounting with partial status.
- JSON, CSV, and HTML acquisition and verification reports.

Analysis:

- MBR and GPT partition discovery through The Sleuth Kit.
- Read-only NTFS, FAT32, exFAT, and ext4 filesystem exploration.
- File metadata, bounded file reads, and deleted-name status.
- Bounded hex and text previews with encoding detection.
- File-signature identification with image/PDF preflight limits.
- Capability-based extraction with traversal, symlink, conflict, and resource protection.
- Source/output SHA-256 verification for every successful export.

Sandboxing and integrity:

- Token `ofi-parser-worker` process for image, partition, filesystem, acquisition, verification, preview, signature, and extraction jobs.
- Landlock and seccomp isolation with rlimits, timeout enforcement, and crash containment.
- Append-only, hash-chained audit log with exclusive writer locking.
- SQLite case and evidence metadata with UTC timestamps.

Desktop application:

- Qt 6 shell with QML evidence tree, file list, properties, viewer, task panel, and case/acquisition panel.
- Working light and dark themes.

## Installation

Packages for version 0.1.0 are built for x86_64 and published with each release. They are signed only by their SHA-256 checksums; no GPG signature is issued.

Verify a downloaded artifact before installing:

```bash
sha256sum -c openforensic-imager_0.1.0_amd64.deb.sha256
sha256sum -c OpenForensic-Imager-0.1.0-x86_64.AppImage.sha256
sha256sum -c openforensic-imager-0.1.0-x86_64.flatpak.sha256
```

A failed checksum means the artifact must not be used.

### Debian package

```bash
sudo apt install ./openforensic-imager_0.1.0_amd64.deb
```

Run the desktop app:

```bash
openforensic-imager
```

Run the offscreen self-test:

```bash
QT_QPA_PLATFORM=offscreen openforensic-imager --self-test
```

Remove the package:

```bash
sudo apt remove openforensic-imager
```

The DEB resolves Qt, The Sleuth Kit, libewf, and QML runtime dependencies from the Ubuntu archive. It does not install a hardware write blocker or grant device permissions. Removal deletes package-owned files under `/usr` only; examiner-selected case databases, logs, reports, evidence, and outputs are not deleted.

### AppImage

```bash
chmod 0755 OpenForensic-Imager-0.1.0-x86_64.AppImage
QT_QPA_PLATFORM=offscreen ./OpenForensic-Imager-0.1.0-x86_64.AppImage --self-test
./OpenForensic-Imager-0.1.0-x86_64.AppImage
```

If FUSE is unavailable, set `APPIMAGE_EXTRACT_AND_RUN=1` for the same command. The AppImage bundles host-built Qt and native forensic libraries, has no sandbox, and inherits host permissions. Removing the file removes the app.

### Flatpak (analysis only)

```bash
flatpak install org.kde.Sdk//6.10 org.kde.Platform//6.10
flatpak install --user ./openforensic-imager-0.1.0-x86_64.flatpak
flatpak run io.github.x0r909.OpenForensicImager
```

Run the smoke test:

```bash
flatpak run --env=QT_QPA_PLATFORM=offscreen \
  io.github.x0r909.OpenForensicImager --self-test
```

The Flatpak manifest grants home-directory access for opening case files and images. It grants no `/dev` access and does not support physical acquisition.

## Building from source

Rust toolchain is pinned by `rust-toolchain.toml` to 1.93.1. Cargo dependencies are pinned by `Cargo.lock`. Build dependencies (tested on Ubuntu 26.04):

```bash
sudo apt install \
  build-essential cmake ninja-build cargo rustc \
  qt6-base-dev qt6-declarative-dev qt6-svg-dev \
  libtsk-dev libewf-dev libglx-dev libvulkan-dev \
  dpkg-dev fakeroot squashfs-tools imagemagick \
  desktop-file-utils appstream jq curl flatpak
```

Build all packages:

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

Reproducibility comparison:

```bash
SOURCE_DATE_EPOCH=1793318400 scripts/verify-reproducible-packages.sh
```

`1793318400` is the fixed Phase 7 timestamp (`2026-10-30T00:00:00Z`). The comparison covers the DEB, AppImage, and the SBOM inside the DEB by byte identity. The Flatpak is compared by content because `flatpak build-export` records per-object timestamps.

## Testing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Sanitizer and fuzz runs extend the workspace tests. See `scripts/run-sanitizers.sh` and `scripts/fuzz-budget.sh`, and the per-phase result documents listed below.

## Documentation

- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md): phased roadmap.
- [ARCHITECTURE.md](ARCHITECTURE.md): component, process, data, and deployment design.
- [REQUIREMENTS.md](REQUIREMENTS.md): normative functional and integrity requirements.
- [SECURITY_MODEL.md](SECURITY_MODEL.md): threat model and mandatory controls.
- [TEST_STRATEGY.md](TEST_STRATEGY.md): fixtures, differential testing, fuzzing, and acceptance gates.
- [FEASIBILITY_REPORT.md](FEASIBILITY_REPORT.md): evidence-backed technical assessment.
- [docs/packaging/INSTALL.md](docs/packaging/INSTALL.md): installation, removal, and Flatpak limits.
- [docs/packaging/REPRODUCIBLE_BUILD.md](docs/packaging/REPRODUCIBLE_BUILD.md): dependency set, fixed timestamp, and reproducibility scope.
- [docs/testing/phase1-results.md](docs/testing/phase1-results.md) through [docs/testing/phase8-results.md](docs/testing/phase8-results.md): per-phase verification commands, results, and limitations.
- [docs/security/phase6-review.md](docs/security/phase6-review.md) and [docs/security/phase8-review.md](docs/security/phase8-review.md): security findings and residual risks.
- [docs/reverse-engineering/](docs/reverse-engineering/): FTK Imager evidence and compatibility status.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution workflow, coding standards, and testing requirements.

## Independence

This project is independent. It does not copy FTK Imager source, assets, or protected implementation details, and it does not claim FTK Imager compatibility without differential test evidence. The FTK Imager installer used for authorized investigation input is untracked and is not redistributed.

## License

OpenForensic Imager is free software, licensed under the GNU General Public License, version 3 or later (`GPL-3.0-or-later`). See [LICENSE](LICENSE) for the full text.

Forensic tools are licensed this way on purpose: an examiner should be able to audit the exact code that produced their evidence, and every recipient of a binary keeps the right to do the same. Third-party components and their license terms are listed in [packaging/common/THIRD_PARTY.md](packaging/common/THIRD_PARTY.md).
