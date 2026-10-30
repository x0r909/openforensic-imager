use std::collections::BTreeMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use forensic_core::{ByteRange, JobId, UtcTimestamp};
use hashing::{HashAlgorithm, HashDigest, HashScope, StreamingHasher};
use image_reader::{ImageReader, SourceFingerprint, open_image};
use libewf_bindings::{EwfCompression, EwfWriteOptions, EwfWriter};
use serde::{Deserialize, Serialize};

const HASH_ALGORITHMS: [HashAlgorithm; 4] = [
    HashAlgorithm::Md5,
    HashAlgorithm::Sha1,
    HashAlgorithm::Sha256,
    HashAlgorithm::Sha512,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Raw,
    Ewf1E01,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionCompression {
    None,
    Fast,
    Best,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquisitionOptions {
    pub block_size: u64,
    pub maximum_segment_size: u64,
    pub compression: AcquisitionCompression,
    pub verify_after: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquisitionRequest {
    pub source_path: PathBuf,
    pub destination_dir: PathBuf,
    pub base_name: String,
    pub format: OutputFormat,
    pub options: AcquisitionOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionStatus {
    Succeeded,
    Partial,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashRecord {
    pub algorithm: HashAlgorithm,
    pub scope: HashScope,
    pub digest_hex: String,
    pub byte_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub path: String,
    pub length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRangeRecord {
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquisitionManifest {
    pub schema_version: u32,
    pub job_id: JobId,
    pub status: AcquisitionStatus,
    pub format: OutputFormat,
    pub source_path: String,
    pub source: SourceFingerprint,
    pub options: AcquisitionOptions,
    pub started_at: UtcTimestamp,
    pub completed_at: UtcTimestamp,
    pub source_bytes_read: u64,
    pub bytes_written: u64,
    pub hashes: Vec<HashRecord>,
    pub artifacts: Vec<ArtifactRecord>,
    pub unreadable_ranges: Vec<ByteRangeRecord>,
    pub errors: Vec<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquisitionResult {
    pub status: AcquisitionStatus,
    pub manifest: AcquisitionManifest,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquisitionErrorCode {
    InvalidRequest,
    SourceOpen,
    Read,
    Write,
    Finalize,
    Verification,
    Io,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquisitionError {
    code: AcquisitionErrorCode,
    message: String,
}

impl AcquisitionError {
    pub(crate) fn new(code: AcquisitionErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> AcquisitionErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for AcquisitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for AcquisitionError {}

pub fn acquire_path(request: AcquisitionRequest) -> Result<AcquisitionResult, AcquisitionError> {
    acquire_path_with_cancel(request, &CancellationToken::default())
}

pub fn acquire_path_with_cancel(
    request: AcquisitionRequest,
    cancellation: &CancellationToken,
) -> Result<AcquisitionResult, AcquisitionError> {
    validate_request(&request)?;
    let reader = open_image(&request.source_path).map_err(|error| {
        AcquisitionError::new(AcquisitionErrorCode::SourceOpen, error.to_string())
    })?;
    acquire_reader(&*reader, request, cancellation)
}

pub fn acquire_from_reader(
    reader: &dyn ImageReader,
    request: AcquisitionRequest,
    cancellation: &CancellationToken,
) -> Result<AcquisitionResult, AcquisitionError> {
    validate_request(&request)?;
    acquire_reader(reader, request, cancellation)
}

fn acquire_reader(
    reader: &dyn ImageReader,
    request: AcquisitionRequest,
    cancellation: &CancellationToken,
) -> Result<AcquisitionResult, AcquisitionError> {
    let job_id = JobId::new_v4();
    let started_at = UtcTimestamp::now();
    let metadata = reader.metadata();
    let source_length = reader.logical_length();
    let staging = request
        .destination_dir
        .join(format!(".{}.partial-{}", request.base_name, job_id));
    std::fs::create_dir(&staging).map_err(|error| {
        AcquisitionError::new(
            AcquisitionErrorCode::Io,
            format!("unable to create staging directory: {error}"),
        )
    })?;

    let mut sink = Sink::create(&staging, &request, source_length)?;
    let mut output_hasher = StreamingHasher::new(&HASH_ALGORITHMS);
    let mut source_hasher = StreamingHasher::new(&HASH_ALGORITHMS);
    let mut offset = 0_u64;
    let mut source_bytes_read = 0_u64;
    let mut unreadable_ranges = Vec::new();
    let mut errors = Vec::new();
    let mut status = AcquisitionStatus::Succeeded;
    let mut block = vec![0_u8; request.options.block_size as usize];

    while offset < source_length {
        if cancellation.is_cancelled() {
            status = AcquisitionStatus::Cancelled;
            errors.push("acquisition cancelled".to_owned());
            break;
        }
        let count = (source_length - offset).min(request.options.block_size);
        let buffer = &mut block[..count as usize];
        let range = ByteRange::new(offset, count, source_length).map_err(|error| {
            AcquisitionError::new(AcquisitionErrorCode::Read, error.to_string())
        })?;

        match reader.read_exact_at(range, buffer) {
            Ok(()) => {
                source_hasher.update(buffer);
                source_bytes_read = source_bytes_read
                    .checked_add(count)
                    .ok_or_else(source_read_count_overflow)?;
            }
            Err(error) => {
                status = AcquisitionStatus::Partial;
                unreadable_ranges.push(ByteRangeRecord {
                    offset,
                    length: count,
                });
                errors.push(format!("read error at offset {offset}: {error}"));
                buffer.fill(0);
            }
        }
        output_hasher.update(buffer);
        sink.write_all(buffer)?;
        offset = offset
            .checked_add(count)
            .ok_or_else(source_read_count_overflow)?;
    }

    let staged_artifacts = if status == AcquisitionStatus::Cancelled {
        sink.abort()?;
        Vec::new()
    } else {
        sink.finish()?
    };
    let output_hashes = output_hasher.finish();
    let source_hashes = source_hasher.finish();
    let mut hashes = hash_records(&output_hashes, HashScope::OutputLogical, offset);
    hashes.extend(hash_records(
        &source_hashes,
        HashScope::SourceRead,
        source_bytes_read,
    ));

    let mut artifacts = Vec::new();
    for path in &staged_artifacts {
        let length = std::fs::metadata(path)
            .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))?
            .len();
        artifacts.push(ArtifactRecord {
            path: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            length,
            sha256: hash_file_sha256(path)?,
        });
    }

    let mut verified = false;
    if request.options.verify_after && !staged_artifacts.is_empty() {
        let expected = output_hashes
            .get(&HashAlgorithm::Sha256)
            .map(HashDigest::to_hex)
            .unwrap_or_default();
        let actual = hash_logical_image_sha256(&staged_artifacts[0])?;
        verified = actual == expected;
        if !verified {
            status = AcquisitionStatus::Failed;
            errors.push("post-acquisition SHA-256 verification mismatch".to_owned());
        }
    } else if !request.options.verify_after && status == AcquisitionStatus::Succeeded {
        status = AcquisitionStatus::Partial;
        errors.push("verification was disabled; acquisition is not release-verified".to_owned());
    }

    let completed_at = UtcTimestamp::now();
    let manifest = AcquisitionManifest {
        schema_version: 1,
        job_id,
        status,
        format: request.format,
        source_path: request.source_path.display().to_string(),
        source: metadata.source,
        options: request.options,
        started_at,
        completed_at,
        source_bytes_read,
        bytes_written: offset,
        hashes,
        artifacts,
        unreadable_ranges,
        errors,
        verified,
    };

    let manifest_dir = if status == AcquisitionStatus::Succeeded {
        let final_paths = staged_artifacts
            .iter()
            .map(|path| {
                request
                    .destination_dir
                    .join(path.file_name().unwrap_or_default())
            })
            .collect::<Vec<_>>();
        if let Some(existing) = final_paths.iter().find(|path| path.exists()) {
            return Err(AcquisitionError::new(
                AcquisitionErrorCode::InvalidRequest,
                format!(
                    "destination artifact already exists: {}",
                    existing.display()
                ),
            ));
        }
        for (path, final_path) in staged_artifacts.into_iter().zip(final_paths) {
            std::fs::rename(&path, &final_path).map_err(|error| {
                AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string())
            })?;
        }
        let _ = std::fs::remove_dir(&staging);
        request.destination_dir.clone()
    } else {
        staging.clone()
    };
    let manifest_path = manifest_dir.join(format!("{}.acquisition.json", request.base_name));
    write_json_atomic(&manifest_path, &manifest)?;

    Ok(AcquisitionResult {
        status,
        manifest,
        manifest_path,
    })
}

fn validate_request(request: &AcquisitionRequest) -> Result<(), AcquisitionError> {
    if request.options.block_size == 0 || request.options.block_size > 16 * 1024 * 1024 {
        return Err(AcquisitionError::new(
            AcquisitionErrorCode::InvalidRequest,
            "block size must be between 1 and 16 MiB",
        ));
    }
    if request.options.maximum_segment_size < 1024 * 1024 {
        return Err(AcquisitionError::new(
            AcquisitionErrorCode::InvalidRequest,
            "maximum segment size must be at least 1 MiB",
        ));
    }
    if request.base_name.is_empty()
        || Path::new(&request.base_name).components().count() != 1
        || request.base_name == "."
        || request.base_name == ".."
    {
        return Err(AcquisitionError::new(
            AcquisitionErrorCode::InvalidRequest,
            "base name must be a single path component",
        ));
    }
    if !request.destination_dir.is_dir() {
        return Err(AcquisitionError::new(
            AcquisitionErrorCode::InvalidRequest,
            "destination directory does not exist",
        ));
    }
    Ok(())
}

enum Sink {
    Raw { file: File, path: PathBuf },
    Ewf(EwfWriter),
}

impl Sink {
    fn create(
        directory: &Path,
        request: &AcquisitionRequest,
        media_size: u64,
    ) -> Result<Self, AcquisitionError> {
        match request.format {
            OutputFormat::Raw => {
                let path = directory.join(format!("{}.raw", request.base_name));
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&path)
                    .map_err(|error| {
                        AcquisitionError::new(AcquisitionErrorCode::Write, error.to_string())
                    })?;
                Ok(Self::Raw { file, path })
            }
            OutputFormat::Ewf1E01 => {
                let target = directory.join(&request.base_name);
                let writer = EwfWriter::create(
                    &target,
                    media_size,
                    EwfWriteOptions {
                        bytes_per_sector: 512,
                        sectors_per_chunk: 64,
                        maximum_segment_size: request.options.maximum_segment_size,
                        compression: map_compression(request.options.compression),
                    },
                )
                .map_err(|error| {
                    AcquisitionError::new(AcquisitionErrorCode::Write, error.to_string())
                })?;
                Ok(Self::Ewf(writer))
            }
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), AcquisitionError> {
        match self {
            Self::Raw { file, .. } => file.write_all(bytes).map_err(|error| {
                AcquisitionError::new(AcquisitionErrorCode::Write, error.to_string())
            }),
            Self::Ewf(writer) => writer.write_all(bytes).map_err(|error| {
                AcquisitionError::new(AcquisitionErrorCode::Write, error.to_string())
            }),
        }
    }

    fn finish(self) -> Result<Vec<PathBuf>, AcquisitionError> {
        match self {
            Self::Raw { mut file, path } => {
                file.flush().map_err(|error| {
                    AcquisitionError::new(AcquisitionErrorCode::Write, error.to_string())
                })?;
                file.sync_all().map_err(|error| {
                    AcquisitionError::new(AcquisitionErrorCode::Write, error.to_string())
                })?;
                Ok(vec![path])
            }
            Self::Ewf(writer) => writer.finalize().map_err(|error| {
                AcquisitionError::new(AcquisitionErrorCode::Finalize, error.to_string())
            }),
        }
    }

    fn abort(self) -> Result<(), AcquisitionError> {
        match self {
            Self::Raw { mut file, .. } => file.flush().map_err(|error| {
                AcquisitionError::new(AcquisitionErrorCode::Write, error.to_string())
            }),
            Self::Ewf(writer) => {
                drop(writer);
                Ok(())
            }
        }
    }
}

fn map_compression(value: AcquisitionCompression) -> EwfCompression {
    match value {
        AcquisitionCompression::None => EwfCompression::None,
        AcquisitionCompression::Fast => EwfCompression::Fast,
        AcquisitionCompression::Best => EwfCompression::Best,
    }
}

fn hash_records(
    hashes: &BTreeMap<HashAlgorithm, HashDigest>,
    scope: HashScope,
    byte_count: u64,
) -> Vec<HashRecord> {
    hashes
        .iter()
        .map(|(algorithm, digest)| HashRecord {
            algorithm: *algorithm,
            scope,
            digest_hex: digest.to_hex(),
            byte_count,
        })
        .collect()
}

fn hash_file_sha256(path: &Path) -> Result<String, AcquisitionError> {
    let mut file = File::open(path)
        .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))?;
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = std::io::Read::read(&mut file, &mut buffer)
            .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finish()[&HashAlgorithm::Sha256].to_hex())
}

fn hash_logical_image_sha256(path: &Path) -> Result<String, AcquisitionError> {
    let reader = open_image(path).map_err(|error| {
        AcquisitionError::new(AcquisitionErrorCode::Verification, error.to_string())
    })?;
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
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
    Ok(hasher.finish()[&HashAlgorithm::Sha256].to_hex())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), AcquisitionError> {
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))?;
    file.write_all(&bytes)
        .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))?;
    file.sync_all()
        .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))?;
    std::fs::rename(&temporary, path)
        .map_err(|error| AcquisitionError::new(AcquisitionErrorCode::Io, error.to_string()))
}

fn source_read_count_overflow() -> AcquisitionError {
    AcquisitionError::new(AcquisitionErrorCode::Read, "source byte count overflowed")
}
