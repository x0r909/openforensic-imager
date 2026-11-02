# ADR-001 decision note: the native dependency linking model

Status: proposed by Task C6 Step 1. Amends [ADR-001](decisions.md) by
narrowing what "build and ship from pinned source" means per artifact; it does
not overturn the decision.

## The question ADR-001 left open

ADR-001 decided the *policy* — build TSK and libewf from pinned source, record
hashes and build flags, never silently fall back to an arbitrary distribution
version — but it did not say how the result reaches a user. Three models were
open:

1. **Pinned static link** — compile the pinned sources into the binary.
2. **Bundled shared objects** — build the pinned sources as `.so` files and ship
   them inside the artifact, with an RPATH.
3. **Distribution dependencies with a documented minimum** — link the system
   libraries and let the package manager satisfy them.

This note picks one model and records why the others were rejected. The choice
is not uniform across artifacts, and pretending otherwise is what produced
ADR-001's unqualified promise.

## Decision

**Model 3 for the DEB; model 2 for the AppImage and the Flatpak; model 1 for
none of them.**

- **DEB:** `Depends` on the distribution's `libtsk19t64` and `libewf2` with the
  version Debian/Ubuntu records, and a documented minimum version. The DEB is a
  distribution package; substituting a private copy of a library the
  distribution already ships and maintains is the thing distribution policy
  exists to prevent. The worker resolves those libraries from the system loader
  paths through the RPATH mechanism described in the C1 work.
- **AppImage and Flatpak:** build the pinned sources in the artifact's own build
  and ship the resulting shared objects inside it. These artifacts claim to run
  on a distribution they were not built on, so they may not rely on the
  distribution's versions; that is the whole reason they bundle.
- **Static link:** rejected. It cannot be implemented on this tree, and the
  reason is measured rather than assumed: TSK's own static link list is
  `-ltsk -lsqlite3 -lafflib -lz -lbfio -lewf -lvhdi -lvmdk`, and while
  `libtsk.a`, `libewf.a`, `libsqlite3.a`, `libz.a`, `libbfio.a`, `libvhdi.a`,
  `libvmdk.a`, `libcrypto.a`, and `libpthread.a` are all present on the build
  host, **`libafflib.a` is not** — the distribution does not ship a static
  libafflib. A static link is therefore not a stricter version of the plan; it
  is a different distribution.

## What "pinned" means here, and what it does not

The pin is on the *source*: a version, a URL, and a SHA-256 in
`packaging/tools.lock`'s style, plus the build flags, recorded per artifact.
For the AppImage and Flatpak that pin is load-bearing, because those artifacts
compile the sources they ship. For the DEB the pin is a **floor**, not a
substitute: the DEB records the version it was built and tested against and
requires at least it, and the pin of the source is what lets the other two
artifacts be rebuilt from a known tree.

This is a narrower promise than ADR-001's wording. It is recordable and
testable, which the current promise is not.

## The platform these versions must cover

Three facts constrain the choice and are worth recording once:

- **Both bundling artifacts nominally cover two architectures, and neither is
  proven on the second.** `scripts/lib/packaging.sh` maps `amd64 -> x86_64` and
  `arm64 -> aarch64` for the AppImage tool selection and for `ofi_flatpak_arch`,
  so both scripts can be asked for either. `packaging/tools.lock` pins
  appimagetool for `x86_64` and `aarch64` separately, because both come from the
  same `continuous` release tag and therefore move independently — a single
  digest would silently pin one architecture's tool and not the other's. The
  Flatpak takes its Qt and its base from `org.kde.Sdk`/`org.kde.Platform` 6.10,
  which is a *runtime* the user's machine provides, not something the bundle
  carries, so the Flatpak's native dependency story is shorter than the
  AppImage's: it bundles only the forensic libraries from model 2 and inherits
  Qt from the runtime. `scripts/package-flatpak.sh` supports the aarch64
  argument path, but no aarch64 Flatpak has been built or installed here, so
  this note claims the x86_64 bundle and the arm64 *code path* only.
- **TSK is not a single license.** Upstream's own
  `licenses/README.md` (in `sleuthkit-4.12.1.tar.gz`) says the `tsk/` code came
  from TCT under IBM-PL-1.0, was adopted by TASK in 2002-2003 under **CPL-1.0**,
  and was adopted into TSK in 2003 under "the existing license" — CPL-1.0. The
  distribution's `/usr/share/doc/libtsk-dev/copyright` records 21 stanzas, among
  them `CPL-1.0`, `IBM-PL-1.0`, `Apache-2.0`, `MIT`, `BSD-3-Clause`, `GPL-2+`,
  `GPL-3+ with Autoconf exception`, and `CC-BY-SA-3.0`.
  `packaging/common/THIRD_PARTY.md` recorded this as "Mixed component licenses
  shipped by the distribution", which is accurate but is not a review.
  **A redistribution review of TSK must enumerate the licenses by path**, because
  the AppImage and Flatpak redistribute the compiled result. `CPL-1.0` is not on
  the FSF's list of GPL-compatible licenses, and the project is
  `GPL-3.0-or-later`; the review must either establish the compatibility
  argument for the files under CPL-1.0 — which is the library core, not a
  peripheral — or record it as a blocker. This note does not guess the answer,
  and no artifact should claim a reviewed dependency set until it is answered.
- **libewf is LGPL-3+** (the `libewf-legacy` line, `20140816`). Bundling it as
  a shared object, as model 2 does, satisfies the LGPL's relinking requirement
  without any further obligation; static-linking it would not, which is an
  independent reason the static model was the wrong default.

## What this note does not decide

- Which exact TSK and libewf releases are pinned. That is Step 2, with a digest
  and build flags per entry.
- Whether the pinned source build should enable TSK's own optional inputs
  (afflib, vhdi, vmdk) or configure them out. Configuring them out would
  *reduce* the dependency set the AppImage and Flatpak bundle, which is a real
  option once the review above exists; it would also narrow the formats those
  artifacts can parse. That trade is a product decision, not a packaging one.

## Consequences

- ADR-001's sentence "build and ship reviewed TSK and libewf versions from
  pinned source" is now **specified** for two of the three artifacts and amended
  for the third. Specified, not implemented: this note decides the model and
  Step 2 pins the sources, but the AppImage and Flatpak do not yet build the
  pinned sources — they bundle the distribution's libraries today. The ADR gains
  a pointer to this note.
- The DEB keeps its distribution dependency model, so
  `packaging/common/THIRD_PARTY.md`'s claim that the DEB "records its runtime
  library dependencies instead of copying replacement system copies" stays
  true and stays the reason the DEB is exempt from the pin.
- The redistribution review of TSK's license stanzas (21 of them in the
  distribution's copyright file alone) is now an explicit, named blocker rather
  than an unstated assumption. It gates the AppImage and Flatpak, not the DEB.