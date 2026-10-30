use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tsk_bindings::{FileInfo, TskError, TskFile, TskFilesystem, TskImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSystemKind {
    Ntfs,
    Fat12,
    Fat16,
    Fat32,
    Exfat,
    Ext2,
    Ext3,
    Ext4,
    Other(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampSemantics {
    Utc,
    LocalWallClock,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceTimestamp {
    pub seconds_since_epoch: i64,
    pub nanoseconds: u32,
    pub semantics: TimestampSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub name_truncated: bool,
    pub metadata_address: u64,
    pub parent_address: u64,
    pub size: u64,
    pub name_flags: u32,
    pub metadata_flags: u32,
    pub name_type: u32,
    pub metadata_type: u32,
    pub mode: u32,
    pub link_count: i32,
    pub uid: u32,
    pub gid: u32,
    pub deleted: bool,
    pub is_directory: bool,
    pub atime: Option<EvidenceTimestamp>,
    pub mtime: Option<EvidenceTimestamp>,
    pub ctime: Option<EvidenceTimestamp>,
    pub crtime: Option<EvidenceTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSystemError {
    message: String,
}

impl FileSystemError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FileSystemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FileSystemError {}

impl From<TskError> for FileSystemError {
    fn from(error: TskError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

pub struct FileSystem {
    filesystem: TskFilesystem,
    _image: TskImage,
    kind: FileSystemKind,
}

pub struct EvidenceFile<'a> {
    file: TskFile<'a>,
    size: u64,
}

impl EvidenceFile<'_> {
    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, FileSystemError> {
        if offset > self.size {
            return Err(FileSystemError {
                message: "file read offset exceeds the evidence file size".to_owned(),
            });
        }
        if buffer.is_empty() {
            return Ok(0);
        }
        let available =
            usize::try_from((self.size - offset).min(buffer.len() as u64)).map_err(|_| {
                FileSystemError {
                    message: "file read length does not fit address space".to_owned(),
                }
            })?;
        if available == 0 {
            return Ok(0);
        }
        let count = self.file.read_at(offset, &mut buffer[..available])?;
        if count == 0 {
            return Err(FileSystemError {
                message: "TSK returned an incomplete file read".to_owned(),
            });
        }
        Ok(count)
    }
}

impl fmt::Debug for FileSystem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileSystem")
            .field("kind", &self.kind)
            .field("block_size", &self.block_size())
            .field("root_inode", &self.root_inode())
            .finish_non_exhaustive()
    }
}

impl FileSystem {
    pub fn open(path: &Path, offset_bytes: u64) -> Result<Self, FileSystemError> {
        let image = TskImage::open(path)?;
        let filesystem = TskFilesystem::open(&image, offset_bytes)?;
        let kind = map_kind(filesystem.type_id());
        Ok(Self {
            filesystem,
            _image: image,
            kind,
        })
    }

    pub fn kind(&self) -> FileSystemKind {
        self.kind
    }

    pub fn block_size(&self) -> u32 {
        self.filesystem.block_size()
    }

    pub fn inode_count(&self) -> u64 {
        self.filesystem.inode_count()
    }

    pub fn root_inode(&self) -> u64 {
        self.filesystem.root_inode()
    }

    pub fn list_directory(&self, inode: u64) -> Result<Vec<FileEntry>, FileSystemError> {
        let directory = self.filesystem.directory(inode)?;
        let mut entries = Vec::with_capacity(directory.len());
        for index in 0..directory.len() {
            let info = directory.entry(index)?;
            if matches!(info.name().as_str(), "." | "..") {
                continue;
            }
            entries.push(self.convert_entry(info));
        }
        Ok(entries)
    }

    pub fn read_file(
        &self,
        metadata_address: u64,
        max_bytes: u64,
    ) -> Result<Vec<u8>, FileSystemError> {
        let file = self.filesystem.file(metadata_address)?;
        let info = file.info()?;
        if info.size > max_bytes {
            return Err(FileSystemError {
                message: format!(
                    "file size {} exceeds configured read limit {}",
                    info.size, max_bytes
                ),
            });
        }
        let size = usize::try_from(info.size).map_err(|_| FileSystemError {
            message: "file size does not fit memory address size".to_owned(),
        })?;
        let mut output = vec![0_u8; size];
        let mut offset = 0_usize;
        while offset < size {
            let count = file.read_at(offset as u64, &mut output[offset..])?;
            if count == 0 {
                return Err(FileSystemError {
                    message: "TSK returned an incomplete file read".to_owned(),
                });
            }
            offset += count;
        }
        Ok(output)
    }

    pub fn open_file(&self, metadata_address: u64) -> Result<EvidenceFile<'_>, FileSystemError> {
        let file = self.filesystem.file(metadata_address)?;
        let size = file.info()?.size;
        Ok(EvidenceFile { file, size })
    }

    fn convert_entry(&self, info: FileInfo) -> FileEntry {
        let semantics = timestamp_semantics(self.kind);
        FileEntry {
            name: info.name(),
            name_truncated: info.name_was_truncated(),
            metadata_address: info.metadata_address,
            parent_address: info.parent_address,
            size: info.size,
            name_flags: info.name_flags,
            metadata_flags: info.metadata_flags,
            name_type: info.name_type,
            metadata_type: info.metadata_type,
            mode: info.mode,
            link_count: info.link_count,
            uid: info.uid,
            gid: info.gid,
            deleted: info.name_deleted() || info.metadata_deleted(),
            is_directory: info.is_directory(),
            atime: evidence_time(info.atime, info.atime_nano, semantics),
            mtime: evidence_time(info.mtime, info.mtime_nano, semantics),
            ctime: evidence_time(info.ctime, info.ctime_nano, semantics),
            crtime: evidence_time(info.crtime, info.crtime_nano, semantics),
        }
    }
}

fn map_kind(type_id: u32) -> FileSystemKind {
    match type_id {
        0x0001 => FileSystemKind::Ntfs,
        0x0002 => FileSystemKind::Fat12,
        0x0004 => FileSystemKind::Fat16,
        0x0008 => FileSystemKind::Fat32,
        0x000a => FileSystemKind::Exfat,
        0x0080 => FileSystemKind::Ext2,
        0x0100 => FileSystemKind::Ext3,
        0x2000 => FileSystemKind::Ext4,
        other => FileSystemKind::Other(other),
    }
}

fn timestamp_semantics(kind: FileSystemKind) -> TimestampSemantics {
    match kind {
        FileSystemKind::Ntfs
        | FileSystemKind::Ext2
        | FileSystemKind::Ext3
        | FileSystemKind::Ext4 => TimestampSemantics::Utc,
        FileSystemKind::Fat12
        | FileSystemKind::Fat16
        | FileSystemKind::Fat32
        | FileSystemKind::Exfat => TimestampSemantics::LocalWallClock,
        FileSystemKind::Other(_) => TimestampSemantics::Unknown,
    }
}

fn evidence_time(
    seconds: i64,
    nanoseconds: u32,
    semantics: TimestampSemantics,
) -> Option<EvidenceTimestamp> {
    if seconds == 0 && nanoseconds == 0 {
        None
    } else {
        Some(EvidenceTimestamp {
            seconds_since_epoch: seconds,
            nanoseconds,
            semantics,
        })
    }
}
