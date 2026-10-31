# Third-Party Components and Notices

Status: Phase 8 development package. OpenForensic Imager is licensed under the
GNU General Public License version 3 or later (`GPL-3.0-or-later`); see
[`LICENSE`](../../LICENSE). The native dependency pinning and clean-system
release gates remain open, so these packages are still development builds.

## Primary native dependencies

| Component | Host version | License in package metadata | Use |
|---|---|---|---|
| The Sleuth Kit | 4.12.1 | Mixed component licenses shipped by the distribution | Partition and filesystem parsing |
| libewf | 20140816 | LGPL-3.0-or-later | EWF1 E01 read and write adapter |
| Qt 6 | 6.10.2 | LGPL/GPL/commercial options | Desktop, widgets, QML, and models |
| SQLite | 3.46.1 | Public domain | Case database |
| libafflib | 3.7.22 | Distribution package license | Transitive TSK support |
| libvhdi | 20240509 | LGPL | Transitive TSK support |
| libvmdk | 20240510 | LGPL | Transitive TSK support |

The DEB records its runtime library dependencies instead of copying replacement
system copies. The AppImage bundles host runtime libraries, including Qt and the
native forensic libraries, so its notices must be reviewed before
redistribution.

The current AppImage is reproducible only with the exact Ubuntu 26.04 package
set used to build it. It is not a fully pinned native dependency bundle. The
Flatpak analysis build uses Qt from `org.kde.Platform` 6.10 and bundles the
forensic libraries listed above.

## Rust crates

The generated CycloneDX SBOM contains every Cargo workspace package and resolved
dependency with its declared license expression. `Cargo.lock` is the
authoritative crate version set.

OpenForensic Imager is licensed `GPL-3.0-or-later`. That is compatible with
every dependency listed above without additional relinking obligations: Qt 6 is
used under its GPL option rather than LGPL, and libewf's LGPL-3.0 terms are
satisfied by the project's own GPL-3.0-or-later grant.

## Packaging tools

AppImage generation uses the pinned `appimagetool-x86_64.AppImage` download in
`packaging/tools.lock`. Its SHA-256 digest is checked before execution.
The build does not commit the tool binary or generated AppImage.

Release desktop binaries omit the GNU build ID so that the temporary
build-directory RPATH cannot change the installed artifact. The current
development packages do not ship separate debug symbols.

## Analysis-only Flatpak build

The analysis-only Flatpak build uses the installed org.kde.Sdk and
org.kde.Platform 6.10 runtimes. It does not grant device access. Physical
acquisition from this Flatpak is unsupported unless the runtime and permission
model are separately reviewed.

The Flatpak bundle is functionally tested but is not included in the current
reproducibility comparison.
