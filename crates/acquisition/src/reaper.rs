//! Reaping of orphaned acquisition staging directories.
//!
//! A cancelled acquisition is delivered by `SIGKILL`, so `acquire_reader`'s
//! cooperative cancellation check never fires and the process never reaches the
//! code that promotes or removes its staging directory. ACQ-011 requires the
//! retained partial output to stay discoverable, so the directory is left in
//! place; without a reaper, a cancel/retry loop slowly fills the destination
//! volume.
//!
//! The reaper is deliberately conservative. It only considers directories that
//! are both named like staging output *and* carry the marker this crate writes,
//! and it removes one only when nothing else can still be using it.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::acquire::{AcquisitionError, AcquisitionErrorCode};

/// Default age after which an orphaned staging directory may be removed.
pub const DEFAULT_STAGING_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Marker file written inside every staging directory this crate creates.
pub const STAGING_MARKER_FILE: &str = ".ofi-staging.json";

/// The marker's contents. `pid` is what lets the reaper tell a live acquisition
/// from an orphan without guessing from the clock alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagingMarker {
    pub job_id: String,
    pub pid: u32,
    pub started_at_unix_ms: u64,
}

/// What the reaper did, so a caller can report it rather than act blindly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReaperReport {
    pub removed: Vec<PathBuf>,
    pub kept: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

/// Removes orphaned staging directories under `destination_dir`.
///
/// `process_is_alive` is injected so the PID check is testable without racing a
/// real process: production passes a `kill(pid, 0)` probe, a test passes a
/// closure over a fixed set.
pub fn reap_stale_staging(
    destination_dir: &Path,
    max_age: Duration,
    process_is_alive: &dyn Fn(u32) -> bool,
) -> Result<ReaperReport, AcquisitionError> {
    let entries = std::fs::read_dir(destination_dir).map_err(|error| {
        AcquisitionError::new(
            AcquisitionErrorCode::Io,
            format!(
                "unable to scan {} for staging directories: {error}",
                destination_dir.display()
            ),
        )
    })?;

    let mut report = ReaperReport::default();
    for entry in entries {
        let entry = entry.map_err(|error| {
            AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string())
        })?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_staging_name(name) {
            continue;
        }

        match classify(&path, max_age, process_is_alive) {
            Ok(Outcome::Reap) => match std::fs::remove_dir_all(&path) {
                Ok(()) => report.removed.push(path),
                // A directory that vanished between the scan and the removal is
                // not a failure to report; something else already cleaned it.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    report.skipped.push(path);
                    let _ = error;
                }
            },
            Ok(Outcome::Keep) => report.kept.push(path),
            Err(_) => report.skipped.push(path),
        }
    }

    Ok(report)
}

enum Outcome {
    Reap,
    Keep,
}

/// `.<base>.partial-<uuid>` — the shape `acquire_reader` creates. The uuid is
/// required: a directory that merely starts with a dot and contains `.partial-`
/// is not evidence of ours.
fn is_staging_name(name: &str) -> bool {
    if !name.starts_with('.') {
        return false;
    }
    let Some(index) = name.rfind(".partial-") else {
        return false;
    };
    let suffix = &name[index + ".partial-".len()..];
    !suffix.is_empty() && uuid::Uuid::parse_str(suffix).is_ok()
}

fn classify(
    path: &Path,
    max_age: Duration,
    process_is_alive: &dyn Fn(u32) -> bool,
) -> Result<Outcome, std::io::Error> {
    let older_than_max_age = directory_age(path)? >= max_age;

    let marker = read_marker(path);
    match marker {
        // A readable marker that names a live process is authoritative: a
        // long-running acquisition is not stale just because it is old.
        Some(marker) if process_is_alive(marker.pid) => Ok(Outcome::Keep),
        // A readable marker whose process is gone still needs the age check, so
        // a cancelled-then-retried directory gets its full retention window.
        Some(_) => Ok(if older_than_max_age {
            Outcome::Reap
        } else {
            Outcome::Keep
        }),
        // With no usable marker the age of the directory is the only signal, and
        // it has to be enough on its own or an interrupted run that died before
        // writing its marker would leak forever.
        None => Ok(if older_than_max_age {
            Outcome::Reap
        } else {
            Outcome::Keep
        }),
    }
}

fn read_marker(path: &Path) -> Option<StagingMarker> {
    let bytes = std::fs::read(path.join(STAGING_MARKER_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn directory_age(path: &Path) -> Result<Duration, std::io::Error> {
    let modified = std::fs::metadata(path)?.modified()?;
    Ok(SystemTime::now()
        .duration_since(modified)
        .unwrap_or(Duration::ZERO))
}

/// Writes the marker that makes a staging directory attributable. Called
/// immediately after the directory is created, so a run that dies at any later
/// point still leaves a reaper a record of which process owned it.
pub(crate) fn write_marker(
    staging: &Path,
    job_id: &str,
    started_at_unix_ms: u64,
) -> Result<(), AcquisitionError> {
    let marker = StagingMarker {
        job_id: job_id.to_owned(),
        pid: std::process::id(),
        started_at_unix_ms,
    };
    let bytes = serde_json::to_vec(&marker).map_err(|error| {
        AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string())
    })?;
    std::fs::write(staging.join(STAGING_MARKER_FILE), bytes).map_err(|error| {
        AcquisitionError::new(
            AcquisitionErrorCode::Io,
            format!("unable to write the staging marker: {error}"),
        )
    })
}

/// Whether the process that owns `pid` still exists. `EPERM` means it exists but
/// belongs to another user, which still counts as alive.
pub fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}
