# Physical-device acquisition runbook

This is the procedure for the one acquisition path the automated suite cannot
exercise on its own: reading a real block device through a hardware write
blocker with a release artifact. It exists because `REQUIREMENTS.md` ACQ-001
("MUST support physical block-device acquisition when the operating system
grants read access") is a `Must`, and no test on the development host can
satisfy it — a block device needs a privilege the test host does not have.

It is written to be executed by a person, and it is written so that the result
is evidence rather than an assertion. Run it before a release claims physical
acquisition support; do not tick the box in `IMPLEMENTATION_PLAN.md` from a
development build.

## What is already proven, and what is not

Stating this first, because a runbook that implies more coverage than exists is
how a device path stays untested while looking tested.

**Proven.** The block-device capacity path works inside the worker's seccomp
filter. `RawReader::source_length` reads a device's size with
`lseek(fd, 0, SEEK_END)`, which the worker's default-deny allow-list permits. It
used to use `ioctl(BLKGETSIZE64)`, which the allow-list omits, so **every**
physical acquisition failed at the first read of the device's capacity with
`EPERM` — with no evidence read. This was confirmed on a real 16 MiB loop
device, and the end-to-end acquisition that followed produced an image whose
SHA-256 equalled an independent `sha256sum` of the device. The regression test
is `a_block_device_is_acquired_through_the_worker_filter` in
`crates/native-bindings/tests/worker_isolation.rs`; it is opt-in, because a
block device needs a privilege the suite lacks:

```sh
OFI_BLOCK_DEVICE_TEST_PATH=/dev/loopN cargo test -p native-bindings \
    --test worker_isolation a_block_device_is_acquired_through_the_worker_filter
```

A loop device is not a physical device, and this proves only the capacity probe
and the read path.

**Not proven, and what this runbook is for.**

- Acquisition of real physical media through a hardware write blocker.
- A device whose size exceeds the worker's file-size budget interplay at
  multi-terabyte scale.
- Any of the three requirements below, which are **not implemented in the
  product**: mount detection (ACQ-007), the write-blocker attestation warning
  and report field (ACQ-006, `SECURITY_MODEL.md` §6), and same-device rejection
  (ACQ-018). A grep over `crates/` and `apps/` finds no mount-table read, no
  "write blocker" string, and no `st_rdev` or `st_dev` comparison on the
  acquisition path. ACQ-007 additionally requires that a mounted or
  write-held device "MUST require an explicit, audited decision"; the product
  asks for no such decision today, so the operator must supply it out of band
  — which is what step 3 below does.

## Prerequisites

- A release artifact: DEB, AppImage, or Flatpak, built from the commit under
  test, with its reproducibility digest recorded.
- A hardware write blocker (a physical bridge, not the software `blockdev
  --setro` or a read-only mount).
- A source device that is expendable and contains no unique evidence. Never
  use original evidence for a test run.
- A destination filesystem with more free space than the source device's full
  capacity. A raw acquisition writes every byte: the sink opens with
  `create_new` and uses sequential `write_all`, with no `seek` or `SEEK_HOLE`.
  Sparse sources save nothing on the write side.
- Read access to the device node for the account running the acquisition.
  Running as root is discouraged; if the device requires it, limit the
  privilege to opening the device, as `SECURITY_MODEL.md` §6 says.

Do not disable the worker's sandbox for this procedure. The sandbox is what the
run is meant to test.

## Step 1: Attach the device through the write blocker

1. Power the host down or detach the destination drive first, so the source
   cannot be confused with the destination.
2. Connect the source device through the write blocker. Do not connect it
   directly to verify "just this once"; a write during a test run invalidates
   the whole run and possibly the device.
3. Record the write blocker's make, model, serial, and firmware version, and a
   photograph of its read-only indicator or switch. This is the attestation: it
   is manual, because no universal Linux API reports a hardware write blocker
   (recorded as unknown `U-016` in `docs/reverse-engineering/unknowns.md`).
4. Boot with the device attached, if the blocker permits hot-plug; otherwise
   attach and re-detect.

## Step 2: Identify the device, and identify the destination

Capture the commands and their output verbatim. `lsblk` and `findmnt` are the
identification tools; neither is part of the product, so these are the
operator's own records.

```sh
lsblk -o NAME,MAJ:MIN,SIZE,TYPE,MODEL,SERIAL,RO,MOUNTPOINT
lsblk -dno NAME,SIZE,MODEL,SERIAL /dev/sdX
sudo blockdev --getro /dev/sdX          # 1 means the kernel sees it read-only
findmnt --source /dev/sdX --target /dev/sdX
```

Record:

- The device path, and its `/dev/disk/by-id` stable alias.
- Capacity in bytes, and its sector size.
- The `RO` column and `blockdev --getro`: the logical read-only flag of the
  device and of the write blocker. `RawReader::open_read_only` rejects a
  descriptor that is not `O_RDONLY`, but it cannot see a blocker's state, so
  the number is recorded rather than inferred.
- Whether anything is mounted from the device, and whether that mount is
  writable. The product does not check this (ACQ-007); you are the check.

**Abort if** the `RO` flag is `0` and you cannot account for it, or if the
device is mounted writable and you have not made and recorded a deliberate
decision to proceed. State that decision in the record.

## Step 3: Record the pre-acquisition state

```sh
DEV=/dev/sdX
ls -l "$DEV"
sudo blockdev --getsize64 "$DEV" > blockdev-getsize64.txt
sudo head -c 1048576 "$DEV" | sha256sum > head-1MiB-before.sha256
sudo dd if="$DEV" bs=1M count=1 status=none | sha256sum
cat /proc/partitions > partitions.txt
```

The first-megabyte digest is a cheap second witness: if the source changed,
this digest changes, and the acquisition's own fingerprint (`st_dev`, `st_ino`,
`st_size`, `st_mtime`, `st_ctime`, read-only flag — `SourceFingerprint` in
`crates/image-reader/src/source.rs`) covers only metadata, not content.

The acquisition manifest records the source fingerprint, the options, the
hashes (`output_logical`, and the stored-format hash for EWF), the unreadable
ranges, the verification result, and the manifest path. It does **not** record
whether a write blocker was attested, because ACQ-006's report requirement is
not implemented — so the attestation must be kept in this record alongside the
manifest, not inside it.

## Step 4: Run the acquisition

Start the acquisition from the release artifact's UI or CLI against the device
node, with the destination on a different physical device from the source.

Expected: a job that reports `succeeded` (or `partial`, if unreadable ranges
were found and recorded as such), a manifest path, and hashes for the output.

A job that fails with `SourceOpen: Io: Operation not permitted (os error 1)` is
the exact signature of the `ioctl`/seccomp defect this runbook's first section
describes. If it appears on a build that should contain the fix, the artifact
is stale — stop and rebuild rather than working around it.

## Step 5: Independently verify the image

The acquisition's own hash is a claim. Verify it against the device with a
different tool, and compare the image to the device byte for byte.

```sh
IMAGE=/path/to/device.raw
cmp -n "$(sudo blockdev --getsize64 "$DEV")" "$DEV" "$IMAGE" && echo "image equals device"
sha256sum "$IMAGE"
sudo sha256sum "$DEV"          # may take hours on real media; it is the point
sudo dd if="$DEV" bs=1M status=none | sha256sum
```

Record:

- The image's SHA-256 from the manifest and from `sha256sum`, and whether they
  agree.
- The device's SHA-256 from an independent read, and whether it agrees with the
  image.
- The `cmp` result over the device's full length.
- `ewfverify` and `ewfinfo` output if the artifact wrote EWF.

ACQ-020 makes this pairing mandatory: "Every acquisition completion test MUST
pair successful creation with an independent verification."

**Abort and record a failure if** the two digests differ, if the image length
differs from the device capacity, or if `cmp` reports a difference at any
offset. Do not average over ranges or report the acquisition as successful with
a caveat; a mismatch is a failed run.

## Step 6: Record the result

Write, as a phase results document under `docs/testing/`:

- Artifact name, version, and digest; commit under test.
- Write blocker identity and the attestation photograph.
- Every command above and its verbatim output.
- The manifest, and the image's digests from both the manifest and the
  independent tool.
- The mount and read-only state observed in step 2, and the recorded decision
  if the device was mounted.
- The abort criteria that were tested and did **not** fire, and any that did.
- Wall-clock time, throughput, and peak RSS of the acquisition.

Hashes, `cmp` output, and command transcripts are the evidence. A screenshot of
the UI alone is not: it shows what the application claimed, not what the device
held.

## Abort criteria

Stop the run and record it as failed if any of these hold. None of them is a
"note and continue":

- The device's read-only flag is not what the write blocker should produce.
- The device is mounted writable and the examiner has not recorded a
  deliberate, audited decision to proceed (ACQ-007's requirement, which the
  operator satisfies here because the product does not).
- The source and destination are the same writable block device. ACQ-018
  requires rejecting this; the product does not implement the check, so the
  operator must. On a single-device host this is easy to do by accident.
- The write blocker's indicator shows writes, or its log records any.
- The image length differs from the device capacity.
- The image's SHA-256 differs from the device's independently read SHA-256.
- `cmp` reports a difference.
- Any command in step 5 could not be run and you are about to report success
  without it.

## Release-gate statement

Physical block-device acquisition is release-gated on this runbook having been
executed, against a release artifact, on real media through a hardware write
blocker, with its evidence recorded as step 6 requires.

Until then:

- The product must not claim tested physical-device acquisition. The
  compatibility matrix records the device path's actual state, and the
  loop-device regression test above covers the capacity probe and the read path
  only.
- ACQ-006, ACQ-007, and ACQ-018 are open requirements, not satisfied ones.
  ACQ-006 and ACQ-007 are `Must` and are not implemented; the operator is
  currently their only enforcement point, which is a gap in the product rather
  than in this procedure. They should be scheduled as work, not left to a
  runbook.
- `IMPLEMENTATION_PLAN.md` records the device test as blocked on hardware,
  naming exactly what must exist before the box can be ticked.
