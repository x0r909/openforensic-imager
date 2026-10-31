use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use forensic_core::ByteRange;
use hashing::{HashAlgorithm, StreamingHasher};
use image_reader::{ImageFormat, open_image};
use serde::{Deserialize, Serialize};

use crate::{AcquisitionError, AcquisitionErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Match,
    Mismatch,
    NotAvailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub path: PathBuf,
    pub format: ImageFormat,
    pub logical_length: u64,
    pub computed: BTreeMap<HashAlgorithm, String>,
    pub expected: BTreeMap<HashAlgorithm, String>,
    pub outcomes: BTreeMap<HashAlgorithm, VerificationOutcome>,
}

pub fn verify_image(
    path: &Path,
    algorithms: &[HashAlgorithm],
    expected: &BTreeMap<HashAlgorithm, String>,
) -> Result<VerificationReport, AcquisitionError> {
    let report = verify_image_report(path, algorithms, expected)?;
    // A verification only counts as performed when every requested algorithm was
    // actually compared against an expected digest and matched. Reporting
    // NotAvailable as a successful job would let an unverified image read as
    // verified, so the failure is returned to the caller instead.
    if let Some((algorithm, outcome)) = report
        .outcomes
        .iter()
        .find(|(_, outcome)| **outcome != VerificationOutcome::Match)
    {
        let reason = match outcome {
            VerificationOutcome::Mismatch => "digest mismatch",
            VerificationOutcome::NotAvailable => "no expected digest to compare",
            VerificationOutcome::Match => unreachable!(),
        };
        return Err(AcquisitionError::new(
            AcquisitionErrorCode::VerificationNotPerformed,
            format!(
                "{algorithm:?} verification did not match the expected digest: {reason}"
            ),
        ));
    }
    Ok(report)
}

/// Reads the image and reports each algorithm's outcome without judging it.
///
/// Used where the individual outcomes must be preserved, such as report
/// rendering. Callers that present a verification verdict must use
/// [`verify_image`], which fails unless every algorithm matched.
pub fn verify_image_report(
    path: &Path,
    algorithms: &[HashAlgorithm],
    expected: &BTreeMap<HashAlgorithm, String>,
) -> Result<VerificationReport, AcquisitionError> {
    let reader = open_image(path).map_err(|error| {
        AcquisitionError::new(AcquisitionErrorCode::Verification, error.to_string())
    })?;
    let mut hasher = StreamingHasher::new(algorithms);
    let length = reader.logical_length();
    let mut offset = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    while offset < length {
        let count = (length - offset).min(buffer.len() as u64);
        let range = ByteRange::new(offset, count, length).map_err(|error| {
            AcquisitionError::new(AcquisitionErrorCode::Verification, error.to_string())
        })?;
        reader
            .read_exact_at(range, &mut buffer[..count as usize])
            .map_err(|error| {
                AcquisitionError::new(AcquisitionErrorCode::Verification, error.to_string())
            })?;
        hasher.update(&buffer[..count as usize]);
        offset += count;
    }
    let computed_hashes = hasher.finish();
    let computed = computed_hashes
        .iter()
        .map(|(algorithm, digest)| (*algorithm, digest.to_hex()))
        .collect::<BTreeMap<_, _>>();
    let outcomes = computed
        .iter()
        .map(|(algorithm, digest)| {
            let outcome = expected.get(algorithm).map_or(
                VerificationOutcome::NotAvailable,
                |expected_digest| {
                    if expected_digest.eq_ignore_ascii_case(digest) {
                        VerificationOutcome::Match
                    } else {
                        VerificationOutcome::Mismatch
                    }
                },
            );
            (*algorithm, outcome)
        })
        .collect();
    Ok(VerificationReport {
        path: path.to_path_buf(),
        format: reader.metadata().format,
        logical_length: length,
        computed,
        expected: expected.clone(),
        outcomes,
    })
}
