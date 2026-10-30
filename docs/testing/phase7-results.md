# Phase 7 Packaging Results

Status: local development packaging increment passed on 2026-10-31. This is not
a clean-system or production distribution claim.

## Environment

| Item | Observed value |
|---|---|
| Host | Ubuntu 26.04.1 LTS, resolute |
| Kernel | 7.0.0-42-generic |
| Architecture | x86_64 |
| Rust and Cargo | 1.93.1 |
| CMake | 4.2.3 |
| Ninja | 1.13.2 |
| Flatpak | 1.16.6 |
| Qt runtime | 6.10.2 |
| The Sleuth Kit | 4.12.1+dfsg-3build1 |
| libewf | 20140816-2build1 |
| Recorded source epoch | `1793318400` (`2026-10-30T00:00:00Z`) |

The DEB and AppImage use the host Ubuntu TSK and libewf packages. They do not
contain the pinned release builds described as the production dependency gate.

## Final artifacts

| Artifact | Size | SHA-256 |
|---|---:|---|
| `openforensic-imager_0.1.0_amd64.deb` | 1,672,928 bytes | `8db9ef10265f100a177bd8f8c403e171249640d1ecdcc41d762f1e364076070d` |
| `OpenForensic-Imager-0.1.0-x86_64.AppImage` | 65,497,592 bytes | `8eaaeac12bf9893df2e08b6b962df1577ac1f2791fcb1ce1905025fab29d09b2` |
| `openforensic-imager-0.1.0-x86_64.flatpak` | 2,914,368 bytes | `808e604f24a0b383022be1f0f07ed7667e8e41941d3751aa7d73759b44f1ce5e` |
| `openforensic-imager-0.1.0.cdx.json` | 59,236 bytes | `d940d14143fb1d41de12e10a802c71b813a1a2dfb957ff24c57c4e8a4e6cea61` |

Each package has a matching `.sha256` sidecar in `packaging/output`.

## Reproducibility

`scripts/verify-reproducible-packages.sh` performed two clean CMake and Cargo
builds in `packaging/build/repro-a` and `packaging/build/repro-b`. Both builds
reported:

```text
DEB size 1672928 bytes, SHA-256 8db9ef10265f100a177bd8f8c403e171249640d1ecdcc41d762f1e364076070d
AppImage size 65497592 bytes, SHA-256 8eaaeac12bf9893df2e08b6b962df1577ac1f2791fcb1ce1905025fab29d09b2
Reproducible package comparison passed
```

Direct `cmp` also returned zero for both matching artifact pairs. The claim is
limited to this host, package set, source revision, and fixed source epoch.
Flatpak and SBOM output were not part of the two-build comparison.

## Packaging tests

`scripts/test-phase7.sh` passed all of these checks:

- Desktop entry and AppStream metadata validation.
- Required packaging files, executable bits, and shell syntax.
- DEB contents, dependency declarations, installed RPATH, and offscreen
  self-test without an injected `LD_LIBRARY_PATH`.
- AppImage extraction, bundled native bindings, Qt platform plugin, QML modules,
  notices, and offscreen self-test.
- Flatpak analysis build, absence of physical device nodes, bundle install in an
  isolated user data directory, and self-test.
- CycloneDX 1.6 SBOM structure, 136 components, artifact checksum sidecars, and
  the fixed SBOM timestamp `2026-10-30T00:00:00Z`.

Observed tail:

```text
DEB artifact passed
AppImage artifact passed
Flatpak analysis build passed
Phase 7 packaging tests passed
```

The Flatpak self-test printed a host `libcurl.so.4` version warning from
`libafflib.so.0` and a D-Bus portal disconnect warning. Both self-tests still
returned success. These warnings remain release review items.

## Regression gates

All commands below were run after the final RPATH and packaging changes:

| Gate | Result |
|---|---|
| Rust test binaries | 45 binaries, 145 passed, 0 failed, 0 ignored |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| CMake build | Passed |
| CTest | 4 of 4 passed |
| QML lint | Passed |
| `cargo-audit` | 1,395 advisories loaded, 116 dependencies scanned, no findings |
| `git diff --check` | Passed |

CMake emitted warnings that several Qt QML plugin link targets do not exist in
the current CMake scope. The installed package tests and CTest passed despite
those configure-time warnings.

## Remaining release restrictions

- The project license is undecided, and the packaged icon is a text placeholder.
- The DEB was self-tested from an extracted root, not installed on a clean Ubuntu
  virtual machine.
- Debian, Arch Linux, Fedora, and ARM64 were not tested.
- `lintian` and downstream distribution policy review were not run.
- No packaged acquisition test used a physical device or hardware write blocker.
- The Flatpak is analysis-only and has no physical-device access.
- Parser dispatch remains in-process. The Phase 6 worker-isolation release
  blocker remains open.
- Sanitizer and scheduled fuzz runs remain open.

## Reproduction commands

```bash
SOURCE_DATE_EPOCH=1793318400 scripts/verify-reproducible-packages.sh
scripts/package-deb.sh
scripts/package-appimage.sh --skip-build
scripts/package-flatpak.sh --skip-build
scripts/test-phase7.sh
```

The artifact digest in this report only applies to the recorded source revision
and environment above.
