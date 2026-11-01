# lintian review of the DEB

Date: 2026-11-01
Host: Ubuntu 26.04.1 LTS, x86_64
Artifact: `packaging/output/openforensic-imager_0.1.0_amd64.deb`

## Command

```
lintian --display-info --pedantic packaging/output/openforensic-imager_0.1.0_amd64.deb
```

## Result

Exit 0. Zero errors, zero warnings from the pedantic set except the one below,
and one informational tag.

```
W: openforensic-imager: initial-upload-closes-no-bugs
   [usr/share/doc/openforensic-imager/changelog.Debian.gz:1]
I: openforensic-imager: extra-license-file
   [usr/share/doc/openforensic-imager/LICENSE.GPL-3.0]
```

## What the first run found

The first pass of the same command, on the artifact as it stood, exited 2 with
seven errors:

```
E: bogus-mail-host Maintainer maintainer@invalid
E: bogus-mail-host-in-debian-changelog maintainer@invalid (for version 0.1.0-1)
E: debian-changelog-file-missing-or-wrong-name
E: shared-library-is-executable 0755 [usr/lib/openforensic-imager/libnative_bindings.so]
E: unstripped-binary-or-object [usr/bin/ofi-parser-worker]
E: unstripped-binary-or-object [usr/bin/openforensic-imager]
E: unstripped-binary-or-object [usr/lib/openforensic-imager/libnative_bindings.so]
W: non-standard-dir-perm 0775 != 0755 × 13
W: non-standard-file-perm 0664 != 0644 × 2
W: no-manual-page × 2 (both binaries)
```

The `0775` and `0664` modes are the diagnostic that matters beyond lintian: the
staging tree is assembled by `cmake --install` and `install`, both of which
apply the builder's umask. The release had been built under umask 002, so a
release built under umask 022 would differ byte-for-byte — the mode is stored
in the archive. The reproducibility check would have caught that; nothing was
checking it before this pass.

## What was fixed

| Tag | Fix |
|---|---|
| `bogus-mail-host`, `bogus-mail-host-in-debian-changelog` | `packaging/debian/control.in` and `packaging/debian/changelog` now use `x0r909 <openforensic-imager@users.noreply.github.com>` instead of `maintainer@invalid`. `invalid` is not a resolvable domain. |
| `debian-changelog-file-missing-or-wrong-name` | `ofi_stage_release` installs the Debian changelog as `changelog.Debian.gz`, not `changelog.gz`. Policy 12.7 reserves the latter for an upstream changelog. |
| `shared-library-is-executable` | `ofi_normalize_stage_modes` sets shared libraries to 0644 (policy 8.1). `cmake --install` still applies `install(PROGRAMS ...)`, which makes them 0755; the normalization runs after it and is what actually decides the shipped mode. The dynamic loader does not need the execute bit. |
| `unstripped-binary-or-object` | New `ofi_strip_stage` in `scripts/lib/packaging.sh`, called by `scripts/package-deb.sh`, runs `strip --strip-unneeded --remove-section=.comment` over `/usr/bin` and `/usr/lib`. The Rust release profile does not strip by default. |
| `non-standard-dir-perm`, `non-standard-file-perm` | `ofi_normalize_stage_modes` normalizes the whole staged tree: directories 0755, shared libraries 0644, other data files 0644, executables 0755. The two SBOM files are installed with an explicit mode because they are written after normalization. |
| `no-manual-page` | `docs/man/openforensic-imager.8` and `docs/man/ofi-parser-worker.8`, installed gzipped at `/usr/share/man/man8` (policy 12.1). |
| `binary-has-unneeded-section .comment` (informational) | Removed by the same `strip` call. `.note.*` is kept: the GNU build-id is what a debug-symbol server is looked up by. |
| `copyright-file-contains-full-gpl-license`, `copyright-not-using-common-license-for-gpl` | These appeared mid-pass, introduced by the fix for the license file. `ofi_stage_release` had been overwriting the DEP-5 `copyright` file that `cmake --install` puts down with the raw GPL text. The full text now goes to `LICENSE.GPL-3.0`, and `copyright` keeps its machine-readable form, which is what policy 12.5 wants because it references `/usr/share/common-licenses/GPL-3`. |

Each of the seven errors has an assertion in
`packaging/tests/test_deb_artifact.sh`, which `scripts/package-deb.sh` runs on
every build: the changelog name, the man pages (present, and rendering without
groff warnings), the maintainer address, the file modes, the strip, and the
DEP-5 `copyright` file. Two are worth naming. The mode check walks the
extracted tree and fails on any file whose mode is not the policy one — this is
the only thing standing between a permissive umask and a release that differs
byte-for-byte from a stricter one. The strip check fails if any shipped ELF
still carries a `.symtab`.

The two licenses tags were caught by re-running lintian after the fix that
introduced them, not by the artifact test; the assertion for them was added
afterwards.

## What remains

`initial-upload-closes-no-bugs` (warning) is not applicable. It asks the first
upload of a package to close its Debian ITP bug, and this package has no ITP:
the DEB is an artifact of this project's release process, not a submission to
the Debian archive. Debian's own guidance allows the warning to be ignored for
a package not intended for Debian.

`extra-license-file` (informational) is intentional. Lintian sees
`LICENSE.GPL-3.0` as redundant with `copyright`. It is not redundant from this
side: the project ships GPL-3.0-or-later and the full license text has to
travel with every artifact, including the AppImage and Flatpak, which have no
`/usr/share/common-licenses`. Keeping the same doc layout in all three
artifacts is worth one informational tag.

## What this does not cover

This is a Debian-policy review of one artifact on one host, and nothing more.

- **Debian (the archive).** Not reviewed. The tags lintian raises against a
  Debian *source* package — `debian/rules`, source format, `debian/watch`,
  `debian/patches`, build reproducibility on the archive's builders,
  `debian/control` `Build-Depends` — are all absent, because this package is
  produced by `scripts/package-deb.sh` and not by `dpkg-buildpackage`.
- **Fedora and RPM.** Not reviewed. No `.spec` file exists.
- **Arch.** Not reviewed. No `PKGBUILD` exists.
- **Ubuntu-specific policy.** Not reviewed beyond what lintian checks on this
  host.
- **The AppImage and Flatpak.** lintian does not read either. Their metadata is
  covered by `packaging/tests/test_appimage_artifact.sh` and
  `scripts/package-flatpak.sh` plus the AppStream validation in
  `scripts/verify-reproducible-packages.sh`.

`docs/packaging/distro-matrix.md` records which distributions are intended to
get native packages at all, which decides how much of the above is a real gap
rather than a non-goal.

## Reproducing

```
scripts/package-deb.sh --skip-build
lintian --display-info --pedantic packaging/output/openforensic-imager_0.1.0_amd64.deb
```

`--skip-build` reuses `build/package-release`. Drop it to rebuild the release
from source.
