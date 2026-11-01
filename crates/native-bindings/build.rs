//! Gives `ofi-parser-worker` an RPATH.
//!
//! The worker links libtsk and libewf, and the desktop spawns it with
//! `.env_clear()` plus a short allow-list that deliberately excludes
//! `LD_LIBRARY_PATH`. It therefore cannot inherit a search path from its
//! caller, and a packaged worker that carries no RPATH can only resolve its
//! libraries from the system loader paths — which the Flatpak sandbox does not
//! have and an AppImage on a clean host does not either.
//!
//! Three entries cover every layout the worker is built or installed into:
//!
//! - `$ORIGIN` — cargo's own target directory, where the build puts the
//!   libraries beside the binary.
//! - `$ORIGIN/../lib` — the AppImage, which collects the whole `ldd` closure
//!   into `usr/lib` and keeps the binary in `usr/bin`.
//! - `$ORIGIN/../lib/openforensic-imager` — the DEB, the Flatpak, and the
//!   CMake install tree, all of which put the libraries in an
//!   `openforensic-imager` subdirectory beside the binary.
//!
//! These are relative to the executable and are searched *before* the system
//! paths, so a bundled library wins over one of the same soname installed on
//! the host — which is what the AppImage and the Flatpak need, and what the DEB
//! is unaffected by, since it bundles none of them and its rpath directories are
//! empty. Directories that do not exist are skipped, so the same binary serves
//! both layouts.
//!
//! `--disable-new-dtags` is not a detail: it emits `DT_RPATH` rather than the
//! modern `DT_RUNPATH`, and the two differ in exactly the case that matters
//! here. `DT_RUNPATH` applies only to an object's own direct dependencies,
//! while `DT_RPATH` is inherited by everything loaded beneath it. The worker
//! needs libtsk and libewf directly, but libtsk in turn needs libvmdk, libvhdi,
//! and libafflib — so with `DT_RUNPATH` the worker's path resolves libtsk and
//! libewf and then fails on the three that live one level further out, which is
//! the failure this was written to fix.
fn main() {
    println!("cargo:rustc-link-arg-bins=-Wl,--disable-new-dtags");
    for directory in [
        "$ORIGIN",
        "$ORIGIN/../lib",
        "$ORIGIN/../lib/openforensic-imager",
    ] {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,{directory}");
    }
    // A change to this file has to rebuild the binary it configures.
    println!("cargo:rerun-if-changed=build.rs");
}
