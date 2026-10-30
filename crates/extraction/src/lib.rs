use std::ffi::CString;
use std::fmt;
use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use filesystem::{FileEntry, FileSystem, FileSystemError};
use forensic_core::UtcTimestamp;
use hashing::{HashAlgorithm, StreamingHasher};
use serde::{Deserialize, Serialize};

const COPY_BUFFER_SIZE: usize = 1024 * 1024;
const MAX_COMPONENT_BYTES: usize = 255;
const MAX_RENAME_ATTEMPTS: u32 = 1000;
const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Fail,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionOptions {
    pub conflict: ConflictPolicy,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub max_depth: usize,
    pub include_deleted: bool,
}

impl Default for ExtractionOptions {
    fn default() -> Self {
        Self {
            conflict: ConflictPolicy::Fail,
            max_file_bytes: 8 * 1024 * 1024 * 1024,
            max_total_bytes: 64 * 1024 * 1024 * 1024,
            max_depth: 256,
            include_deleted: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionRecord {
    pub original_name: String,
    pub relative_path: String,
    pub entry: FileEntry,
    pub exported_size: u64,
    pub source_sha256: String,
    pub output_sha256: String,
    pub extracted_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionFailure {
    pub original_name: String,
    pub relative_path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestStatus {
    Complete,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionManifest {
    pub schema_version: u32,
    pub generated_at: UtcTimestamp,
    pub output_root: PathBuf,
    pub directory: String,
    pub status: ManifestStatus,
    pub records: Vec<ExtractionRecord>,
    pub failures: Vec<ExtractionFailure>,
}

impl ExtractionManifest {
    pub fn to_json_pretty(&self) -> Result<String, ExtractionError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ExtractionError::new(format!("manifest JSON failed: {error}")))
    }

    pub fn write_json(&self, path: &Path) -> Result<(), ExtractionError> {
        let json = self.to_json_pretty()?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(ExtractionError::io)?;
        file.write_all(json.as_bytes())
            .map_err(ExtractionError::io)?;
        file.sync_all().map_err(ExtractionError::io)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionError {
    message: String,
}

impl ExtractionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn io(error: std::io::Error) -> Self {
        Self::new(error.to_string())
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ExtractionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractionError {}

impl From<FileSystemError> for ExtractionError {
    fn from(error: FileSystemError) -> Self {
        Self::new(error.to_string())
    }
}

pub struct Extractor {
    root: OwnedFd,
    root_path: PathBuf,
    options: ExtractionOptions,
    total_bytes: u64,
}

impl Extractor {
    pub fn create(root: &Path, options: ExtractionOptions) -> Result<Self, ExtractionError> {
        validate_options(&options)?;
        DirBuilder::new()
            .recursive(false)
            .mode(0o700)
            .create(root)
            .map_err(ExtractionError::io)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(root)
            .map_err(ExtractionError::io)?;
        let metadata = file.metadata().map_err(ExtractionError::io)?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(ExtractionError::new(
                "extraction root must be owned by the current user with mode 0700",
            ));
        }
        Ok(Self {
            root: file.into(),
            root_path: root.to_path_buf(),
            options,
            total_bytes: 0,
        })
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn extract_file(
        &mut self,
        filesystem: &FileSystem,
        entry: &FileEntry,
    ) -> Result<ExtractionRecord, ExtractionError> {
        let record = self.extract_entry(filesystem, self.root.as_fd(), entry, "")?;
        self.total_bytes = self
            .total_bytes
            .checked_add(record.exported_size)
            .ok_or_else(|| ExtractionError::new("total extraction byte count overflowed"))?;
        Ok(record)
    }

    pub fn extract_directory(
        &mut self,
        filesystem: &FileSystem,
        inode: u64,
        directory_name: &str,
    ) -> Result<ExtractionManifest, ExtractionError> {
        let safe_name = sanitize_component(directory_name)?;
        let directory = ensure_directory(self.root.as_fd(), &safe_name)?;
        let relative_root = safe_name;
        let mut records = Vec::new();
        let mut failures = Vec::new();
        self.walk_directory(
            filesystem,
            inode,
            directory.as_fd(),
            &relative_root,
            1,
            &mut records,
            &mut failures,
        )?;
        let status = if failures.is_empty() {
            ManifestStatus::Complete
        } else {
            ManifestStatus::Partial
        };
        Ok(ExtractionManifest {
            schema_version: 1,
            generated_at: UtcTimestamp::now(),
            output_root: self.root_path.clone(),
            directory: relative_root,
            status,
            records,
            failures,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_directory(
        &mut self,
        filesystem: &FileSystem,
        inode: u64,
        directory: BorrowedFd<'_>,
        relative_parent: &str,
        depth: usize,
        records: &mut Vec<ExtractionRecord>,
        failures: &mut Vec<ExtractionFailure>,
    ) -> Result<(), ExtractionError> {
        let entries = filesystem.list_directory(inode)?;
        for entry in entries {
            if entry.deleted && !self.options.include_deleted {
                continue;
            }
            let safe_name = match sanitize_component(&entry.name) {
                Ok(name) => name,
                Err(error) => {
                    failures.push(ExtractionFailure {
                        original_name: entry.name,
                        relative_path: None,
                        message: error.message,
                    });
                    continue;
                }
            };
            if entry.is_directory {
                if entry.deleted {
                    continue;
                }
                if depth >= self.options.max_depth {
                    failures.push(ExtractionFailure {
                        original_name: entry.name,
                        relative_path: Some(join_relative(relative_parent, &safe_name)),
                        message: "maximum extraction path depth exceeded".to_owned(),
                    });
                    continue;
                }
                match ensure_directory(directory, &safe_name) {
                    Ok(child) => {
                        let child_path = join_relative(relative_parent, &safe_name);
                        if let Err(error) = self.walk_directory(
                            filesystem,
                            entry.metadata_address,
                            child.as_fd(),
                            &child_path,
                            depth + 1,
                            records,
                            failures,
                        ) {
                            failures.push(ExtractionFailure {
                                original_name: entry.name,
                                relative_path: Some(child_path),
                                message: error.message,
                            });
                        }
                    }
                    Err(error) => failures.push(ExtractionFailure {
                        original_name: entry.name,
                        relative_path: Some(join_relative(relative_parent, &safe_name)),
                        message: error.message,
                    }),
                }
                continue;
            }
            if entry.metadata_type != 1 {
                failures.push(ExtractionFailure {
                    original_name: entry.name,
                    relative_path: Some(join_relative(relative_parent, &safe_name)),
                    message: format!("unsupported filesystem object type {}", entry.metadata_type),
                });
                continue;
            }
            match self.extract_entry(filesystem, directory, &entry, relative_parent) {
                Ok(record) => {
                    self.total_bytes = self
                        .total_bytes
                        .checked_add(record.exported_size)
                        .ok_or_else(|| {
                            ExtractionError::new("total extraction byte count overflowed")
                        })?;
                    records.push(record);
                }
                Err(error) => failures.push(ExtractionFailure {
                    original_name: entry.name,
                    relative_path: Some(join_relative(relative_parent, &safe_name)),
                    message: error.message,
                }),
            }
        }
        Ok(())
    }

    fn extract_entry(
        &self,
        filesystem: &FileSystem,
        parent: BorrowedFd<'_>,
        entry: &FileEntry,
        relative_parent: &str,
    ) -> Result<ExtractionRecord, ExtractionError> {
        if entry.metadata_type != 1 || entry.is_directory {
            return Err(ExtractionError::new(
                "only regular files can be exported as evidence files",
            ));
        }
        let safe_name = sanitize_component(&entry.name)?;
        if entry.size > self.options.max_file_bytes {
            return Err(ExtractionError::new(format!(
                "file size {} exceeds the extraction limit of {} bytes",
                entry.size, self.options.max_file_bytes
            )));
        }
        let projected_total = self
            .total_bytes
            .checked_add(entry.size)
            .ok_or_else(|| ExtractionError::new("total extraction byte count overflowed"))?;
        if projected_total > self.options.max_total_bytes {
            return Err(ExtractionError::new(format!(
                "extraction total exceeds the limit of {} bytes",
                self.options.max_total_bytes
            )));
        }
        let source = filesystem.open_file(entry.metadata_address)?;
        if source.size() != entry.size {
            return Err(ExtractionError::new(
                "evidence file size changed between metadata lookup and export",
            ));
        }

        let (mut destination, actual_name) =
            open_destination(parent, &safe_name, self.options.conflict)?;
        let destination_metadata = destination.metadata().map_err(ExtractionError::io)?;
        if !destination_metadata.is_file() || destination_metadata.nlink() != 1 {
            let _ = remove_file(parent, &actual_name);
            return Err(ExtractionError::new(
                "destination is not an exclusively created regular file",
            ));
        }

        let copy_result = (|| -> Result<(String, String), ExtractionError> {
            let mut source_hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
            let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
            let mut offset = 0_u64;
            while offset < entry.size {
                let wanted = usize::try_from((entry.size - offset).min(COPY_BUFFER_SIZE as u64))
                    .map_err(|_| ExtractionError::new("copy chunk does not fit address space"))?;
                let count = source.read_at(offset, &mut buffer[..wanted])?;
                if count == 0 {
                    return Err(ExtractionError::new(
                        "evidence file ended before its declared size",
                    ));
                }
                destination
                    .write_all(&buffer[..count])
                    .map_err(ExtractionError::io)?;
                source_hasher.update(&buffer[..count]);
                offset = offset
                    .checked_add(count as u64)
                    .ok_or_else(|| ExtractionError::new("source read offset overflowed"))?;
            }
            destination.sync_all().map_err(ExtractionError::io)?;
            drop(destination);
            sync_directory(parent)?;

            let source_sha256 = source_hasher.finish()[&HashAlgorithm::Sha256].to_hex();
            let (output_sha256, output_size) = hash_output(parent, &actual_name)?;
            if output_size != entry.size || output_sha256 != source_sha256 {
                return Err(ExtractionError::new(
                    "exported file hash or size does not match the evidence read",
                ));
            }
            Ok((source_sha256, output_sha256))
        })();

        let (source_sha256, output_sha256) = match copy_result {
            Ok(hashes) => hashes,
            Err(error) => {
                let _ = remove_file(parent, &actual_name);
                return Err(error);
            }
        };
        Ok(ExtractionRecord {
            original_name: entry.name.clone(),
            relative_path: join_relative(relative_parent, &actual_name),
            entry: entry.clone(),
            exported_size: entry.size,
            source_sha256,
            output_sha256,
            extracted_at: UtcTimestamp::now(),
        })
    }
}

fn validate_options(options: &ExtractionOptions) -> Result<(), ExtractionError> {
    if options.max_file_bytes == 0 || options.max_total_bytes == 0 {
        return Err(ExtractionError::new(
            "extraction byte limits must be positive",
        ));
    }
    if options.max_depth == 0 || options.max_depth > 256 {
        return Err(ExtractionError::new(
            "maximum extraction depth must be between 1 and 256",
        ));
    }
    Ok(())
}

fn sanitize_component(name: &str) -> Result<String, ExtractionError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(ExtractionError::new(format!(
            "unsafe evidence name: {name:?}"
        )));
    }
    let mut output = String::new();
    for character in name.chars() {
        if character.is_control() || matches!(character, '%' | '\\') {
            for byte in character.to_string().as_bytes() {
                output.push_str(&format!("%{byte:02X}"));
            }
        } else {
            output.push(character);
        }
    }
    if output.len() > MAX_COMPONENT_BYTES {
        return Err(ExtractionError::new(format!(
            "sanitized evidence name exceeds {MAX_COMPONENT_BYTES} bytes"
        )));
    }
    Ok(output)
}

fn join_relative(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

fn renamed_component(name: &str, attempt: u32) -> String {
    if attempt == 0 {
        return name.to_owned();
    }
    match name.rfind('.') {
        Some(index) if index > 0 => {
            format!("{} ({attempt}).{}", &name[..index], &name[index + 1..])
        }
        _ => format!("{name} ({attempt})"),
    }
}

fn open_destination(
    parent: BorrowedFd<'_>,
    name: &str,
    conflict: ConflictPolicy,
) -> Result<(File, String), ExtractionError> {
    let mut candidate = name.to_owned();
    for attempt in 0..=MAX_RENAME_ATTEMPTS {
        if attempt > 0 {
            if conflict == ConflictPolicy::Fail {
                break;
            }
            candidate = renamed_component(name, attempt);
            if candidate.len() > MAX_COMPONENT_BYTES {
                return Err(ExtractionError::new(
                    "renamed extraction path exceeds the name limit",
                ));
            }
        }
        match openat2(
            parent,
            &candidate,
            libc::O_CREAT | libc::O_EXCL | libc::O_WRONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        ) {
            Ok(fd) => return Ok((fd.into(), candidate)),
            Err(error) if error.raw_os_error() == Some(libc::EEXIST) => {
                if conflict == ConflictPolicy::Fail {
                    return Err(ExtractionError::new(format!(
                        "destination file exists: {candidate}"
                    )));
                }
            }
            Err(error) => return Err(ExtractionError::io(error)),
        }
    }
    Err(ExtractionError::new(
        "unable to choose a conflict-free extraction name",
    ))
}

fn ensure_directory(parent: BorrowedFd<'_>, name: &str) -> Result<OwnedFd, ExtractionError> {
    let directory_flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
    match openat2(parent, name, directory_flags | libc::O_NOFOLLOW, 0) {
        Ok(directory) => return Ok(directory),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => {}
        Err(error) => return Err(ExtractionError::io(error)),
    }
    let name_c = c_name(name)?;
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name_c.as_ptr(), 0o700) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EEXIST) {
            return Err(ExtractionError::io(error));
        }
    }
    sync_directory(parent)?;
    openat2(parent, name, directory_flags | libc::O_NOFOLLOW, 0).map_err(ExtractionError::io)
}

fn hash_output(parent: BorrowedFd<'_>, name: &str) -> Result<(String, u64), ExtractionError> {
    let fd = openat2(
        parent,
        name,
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        0,
    )
    .map_err(ExtractionError::io)?;
    let mut file: File = fd.into();
    let metadata = file.metadata().map_err(ExtractionError::io)?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(ExtractionError::new(
            "exported path is not a regular file with one link",
        ));
    }
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    let mut size = 0_u64;
    loop {
        let count = file.read(&mut buffer).map_err(ExtractionError::io)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| ExtractionError::new("exported file size overflowed"))?;
    }
    Ok((hasher.finish()[&HashAlgorithm::Sha256].to_hex(), size))
}

fn remove_file(parent: BorrowedFd<'_>, name: &str) -> std::io::Result<()> {
    let name_c = CString::new(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in name"))?;
    let result = unsafe { libc::unlinkat(parent.as_raw_fd(), name_c.as_ptr(), 0) };
    if result == 0 {
        sync_directory(parent).map_err(|error| {
            std::io::Error::other(format!("unable to sync extraction directory: {error}"))
        })
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn sync_directory(directory: BorrowedFd<'_>) -> Result<(), ExtractionError> {
    let result = unsafe { libc::fsync(directory.as_raw_fd()) };
    if result == 0 {
        Ok(())
    } else {
        Err(ExtractionError::io(std::io::Error::last_os_error()))
    }
}

fn c_name(name: &str) -> Result<CString, ExtractionError> {
    CString::new(name).map_err(|_| ExtractionError::new("extraction name contains a NUL byte"))
}

#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

fn openat2(parent: BorrowedFd<'_>, name: &str, flags: i32, mode: u32) -> std::io::Result<OwnedFd> {
    let name = CString::new(name)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "NUL in name"))?;
    let how = OpenHow {
        flags: flags as u64,
        mode: mode as u64,
        resolve: RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_SYMLINKS,
    };
    let result = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            parent.as_raw_fd(),
            name.as_ptr(),
            &how,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if result < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        let fd = result as RawFd;
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}
