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

OpenForensic Imager is licensed `GPL-3.0-or-later`. That is compatible with the
listed dependencies without additional relinking obligations for libewf, whose
LGPL-3.0 terms the project's own GPL grant satisfies; libewf is bundled as a
shared object, so its relinking requirement is met by the artifact itself. Qt 6
is used under its GPL option rather than LGPL.

The Sleuth Kit is **not** reviewed as a single license. The distribution's
`/usr/share/doc/libtsk-dev/copyright` records 21 stanzas, including `CPL-1.0`
(which the FSF does not list as GPL-compatible), `IBM-PL-1.0`, `Apache-2.0`,
`MIT`, `BSD-3-Clause`, `GPL-2+`, `GPL-3+ with Autoconf exception`, and
`CC-BY-SA-3.0`. Because the AppImage and the Flatpak redistribute the compiled
result, they may not be described as a reviewed dependency set until those
stanzas are enumerated and the `CPL-1.0` files are resolved. The DEB is not
affected: it links the distribution's library, which the distribution already
distributes under its own terms. This is recorded as a blocker in
[`docs/architecture/adr-001-linking-model.md`](../../docs/architecture/adr-001-linking-model.md).

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

The Flatpak bundle is functionally tested and is included in the reproducibility
comparison by content manifest (file count, `metadata`, and `files/` digests)
rather than by byte digest: `flatpak build-export` records object timestamps, so
the bundle is not byte-reproducible by design and is exempt from the byte
comparison that covers the DEB and the AppImage.
