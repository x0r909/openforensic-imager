# Installation, Removal, and Recovery

Status: Phase 8 development packages, 2026-10-31.

The project license is undecided. The generated packages are development
artifacts and are not licensed for redistribution. The icon and maintainer
address are placeholders.

## Artifact scope

The Phase 8 build produces:

| Artifact | Intended use | Tested host |
|---|---|---|
| `openforensic-imager_0.1.0_amd64.deb` | System installation | Ubuntu 26.04 x86_64 |
| `OpenForensic-Imager-0.1.0-x86_64.AppImage` | Portable installation | Ubuntu 26.04 x86_64 |
| `openforensic-imager-0.1.0-x86_64.flatpak` | Analysis only | Ubuntu 26.04 x86_64 |

Debian, Arch Linux, Fedora, ARM64, and clean virtual-machine installation are
not yet verified. The Flatpak is not an acquisition deployment. It has no
physical-device access.

## Verify the downloaded artifacts

Run these commands from the directory containing the artifacts:

```bash
sha256sum -c openforensic-imager_0.1.0_amd64.deb.sha256
sha256sum -c OpenForensic-Imager-0.1.0-x86_64.AppImage.sha256
sha256sum -c openforensic-imager-0.1.0-x86_64.flatpak.sha256
```

A failed checksum means the artifact must not be used.

## DEB

Install:

```bash
sudo apt install ./openforensic-imager_0.1.0_amd64.deb
```

The package resolves Qt, The Sleuth Kit, libewf, and QML runtime dependencies
from the Ubuntu archive. It does not install a hardware write blocker or grant
device permissions.

Run the desktop application:

```bash
openforensic-imager
```

Run the packaged dependency and QML smoke test without a display:

```bash
QT_QPA_PLATFORM=offscreen openforensic-imager --self-test
```

Remove the package:

```bash
sudo apt remove openforensic-imager
```

Package removal deletes package-owned files under `/usr`. It does not delete
case databases, audit logs, reports, source evidence, or acquisition outputs
because those locations are selected by the examiner.

## AppImage

Make the artifact executable and run the smoke test:

```bash
chmod 0755 OpenForensic-Imager-0.1.0-x86_64.AppImage
QT_QPA_PLATFORM=offscreen \
  ./OpenForensic-Imager-0.1.0-x86_64.AppImage --self-test
```

Run the desktop application:

```bash
./OpenForensic-Imager-0.1.0-x86_64.AppImage
```

If FUSE is unavailable, set `APPIMAGE_EXTRACT_AND_RUN=1` for the same command.
The AppImage bundles host-built Qt and native forensic libraries. It does not
add a sandbox and inherits the host process permissions.

Removing the AppImage file removes the application. The AppImage does not
install a system service or own examiner-created case and evidence directories.

## Flatpak analysis build

Install the required runtime if it is not already present:

```bash
flatpak install org.kde.Sdk//6.10 org.kde.Platform//6.10
flatpak install --user ./openforensic-imager-0.1.0-x86_64.flatpak
```

Run the analysis workbench:

```bash
flatpak run io.github.x0r909.OpenForensicImager
```

Run its smoke test:

```bash
flatpak run --env=QT_QPA_PLATFORM=offscreen \
  io.github.x0r909.OpenForensicImager --self-test
```

The manifest grants home-directory access for opening case and image files. It
does not grant `/dev` access. Physical acquisition with this build is
unsupported.

Remove the Flatpak:

```bash
flatpak uninstall io.github.x0r909.OpenForensicImager
```

Flatpak removal does not delete files the user placed in the home directory.
Check `~/.var/app/io.github.x0r909.OpenForensicImager` before any manual cleanup.
Never delete a case directory until its database, audit log, reports, and
evidence references have been preserved according to the case retention policy.

## Case data and recovery

The application stores a case database at the path selected by the examiner.
The adjacent `audit.jsonl` is the case audit log. SQLite may also create
`case.db-wal` and `case.db-shm`. Reports and acquired images are written to
locations selected in the application.

For backup or recovery:

1. Close the application so no job is writing the case.
2. Copy the complete case directory, including the database sidecars and audit
   log, to separate media.
3. Verify the copied files before changing the original directory.
4. Work on the copy when checking a database or audit log.
5. Keep the original evidence read-only and do not restore a database over it.

To inspect a copied SQLite database:

```bash
sqlite3 case-copy.db 'PRAGMA integrity_check;'
```

The audit log is append-only and hash chained. Do not edit it to repair a
reported chain error. Preserve the complete log and the verification report for
review.

If an acquisition or verification reports a partial or failed state, preserve
the manifest, audit records, and error output. Do not label the output as a
complete image or overwrite it with a retry.

## Remaining release restrictions

- The project license is not selected.
- The application icon is a text placeholder.
- Evidence parsing runs in a same-user worker process. It is not a privilege
  boundary against kernel or sandbox escapes.
- The packages have not been installed on a clean VM or a secondary
  distribution.
