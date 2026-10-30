use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use libc::ssize_t;

#[repr(C)]
struct TskImageHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct TskVolumeSystemHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct TskFilesystemHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct TskDirectoryHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct TskFileHandle {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PartitionInfo {
    pub address: u64,
    pub start_sector: u64,
    pub sector_count: u64,
    pub offset_bytes: u64,
    pub length_bytes: u64,
    pub flags: u32,
    pub table_number: i8,
    pub slot_number: i8,
    pub description: [c_char; 256],
}

impl Default for PartitionInfo {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl PartitionInfo {
    pub fn description(&self) -> String {
        unsafe { CStr::from_ptr(self.description.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    pub fn is_allocated(&self) -> bool {
        self.flags & 0x01 != 0
    }

    pub fn is_metadata(&self) -> bool {
        self.flags & 0x04 != 0
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FileInfo {
    pub name: [c_char; 4096],
    pub name_truncated: u32,
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
    pub atime: i64,
    pub mtime: i64,
    pub ctime: i64,
    pub crtime: i64,
    pub atime_nano: u32,
    pub mtime_nano: u32,
    pub ctime_nano: u32,
    pub crtime_nano: u32,
}

impl Default for FileInfo {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl fmt::Debug for FileInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileInfo")
            .field("name", &self.name())
            .field("metadata_address", &self.metadata_address)
            .field("size", &self.size)
            .field("metadata_flags", &self.metadata_flags)
            .finish()
    }
}

impl FileInfo {
    pub fn name(&self) -> String {
        unsafe { CStr::from_ptr(self.name.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    }

    pub fn name_was_truncated(&self) -> bool {
        self.name_truncated != 0
    }

    pub fn name_deleted(&self) -> bool {
        self.name_flags & 0x02 != 0
    }

    pub fn metadata_deleted(&self) -> bool {
        self.metadata_flags & 0x02 != 0
    }

    pub fn is_directory(&self) -> bool {
        self.metadata_type == 2 || self.name_type == 3 || self.name_type == 11
    }
}

#[link(name = "ofi_tsk_shim", kind = "static")]
unsafe extern "C" {
    fn ofi_tsk_last_error(buffer: *mut c_char, size: usize);
    fn ofi_tsk_image_open(path: *const c_char, image: *mut *mut TskImageHandle) -> c_int;
    fn ofi_tsk_image_close(image: *mut TskImageHandle);
    fn ofi_tsk_image_size(image: *const TskImageHandle) -> u64;
    fn ofi_tsk_image_sector_size(image: *const TskImageHandle) -> u32;
    fn ofi_tsk_vs_open(
        image: *const TskImageHandle,
        type_id: u8,
        volume_system: *mut *mut TskVolumeSystemHandle,
    ) -> c_int;
    fn ofi_tsk_vs_close(volume_system: *mut TskVolumeSystemHandle);
    fn ofi_tsk_vs_type(volume_system: *const TskVolumeSystemHandle) -> u32;
    fn ofi_tsk_vs_block_size(volume_system: *const TskVolumeSystemHandle) -> u32;
    fn ofi_tsk_vs_partition_count(volume_system: *const TskVolumeSystemHandle) -> u32;
    fn ofi_tsk_vs_partition(
        volume_system: *const TskVolumeSystemHandle,
        index: u32,
        partition: *mut PartitionInfo,
    ) -> c_int;
    fn ofi_tsk_fs_open(
        image: *const TskImageHandle,
        offset_bytes: u64,
        filesystem: *mut *mut TskFilesystemHandle,
    ) -> c_int;
    fn ofi_tsk_fs_close(filesystem: *mut TskFilesystemHandle);
    fn ofi_tsk_fs_type(filesystem: *const TskFilesystemHandle) -> u32;
    fn ofi_tsk_fs_block_size(filesystem: *const TskFilesystemHandle) -> u32;
    fn ofi_tsk_fs_inode_count(filesystem: *const TskFilesystemHandle) -> u64;
    fn ofi_tsk_fs_root_inode(filesystem: *const TskFilesystemHandle) -> u64;
    fn ofi_tsk_dir_open(
        filesystem: *const TskFilesystemHandle,
        inode: u64,
        directory: *mut *mut TskDirectoryHandle,
    ) -> c_int;
    fn ofi_tsk_dir_close(directory: *mut TskDirectoryHandle);
    fn ofi_tsk_dir_count(directory: *const TskDirectoryHandle) -> usize;
    fn ofi_tsk_dir_entry(
        directory: *const TskDirectoryHandle,
        index: usize,
        entry: *mut FileInfo,
    ) -> c_int;
    fn ofi_tsk_file_open(
        filesystem: *const TskFilesystemHandle,
        inode: u64,
        file: *mut *mut TskFileHandle,
    ) -> c_int;
    fn ofi_tsk_file_close(file: *mut TskFileHandle);
    fn ofi_tsk_file_read(
        file: *const TskFileHandle,
        offset: u64,
        buffer: *mut c_void,
        length: usize,
    ) -> ssize_t;
    fn ofi_tsk_file_info(file: *const TskFileHandle, entry: *mut FileInfo) -> c_int;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TskError {
    message: String,
}

impl TskError {
    fn last() -> Self {
        let mut buffer = [0_i8; 1024];
        unsafe {
            ofi_tsk_last_error(buffer.as_mut_ptr(), buffer.len());
        }
        Self {
            message: unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for TskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TskError {}

pub struct TskImage {
    handle: *mut TskImageHandle,
}

impl TskImage {
    pub fn open(path: &Path) -> Result<Self, TskError> {
        let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| TskError {
            message: "image path contains NUL".to_owned(),
        })?;
        let mut handle = ptr::null_mut();
        let result = unsafe { ofi_tsk_image_open(path.as_ptr(), &mut handle) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(Self { handle })
    }

    pub fn size(&self) -> u64 {
        unsafe { ofi_tsk_image_size(self.handle) }
    }

    pub fn sector_size(&self) -> u32 {
        unsafe { ofi_tsk_image_sector_size(self.handle) }
    }
}

impl Drop for TskImage {
    fn drop(&mut self) {
        unsafe { ofi_tsk_image_close(self.handle) };
    }
}

pub struct TskVolumeSystem {
    handle: *mut TskVolumeSystemHandle,
}

impl TskVolumeSystem {
    pub fn open(image: &TskImage) -> Result<Self, TskError> {
        let mut handle = ptr::null_mut();
        let result = unsafe { ofi_tsk_vs_open(image.handle, 0, &mut handle) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(Self { handle })
    }

    pub fn type_id(&self) -> u32 {
        unsafe { ofi_tsk_vs_type(self.handle) }
    }

    pub fn block_size(&self) -> u32 {
        unsafe { ofi_tsk_vs_block_size(self.handle) }
    }

    pub fn partition_count(&self) -> u32 {
        unsafe { ofi_tsk_vs_partition_count(self.handle) }
    }

    pub fn partition(&self, index: u32) -> Result<PartitionInfo, TskError> {
        let mut partition = PartitionInfo::default();
        let result = unsafe { ofi_tsk_vs_partition(self.handle, index, &mut partition) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(partition)
    }
}

impl Drop for TskVolumeSystem {
    fn drop(&mut self) {
        unsafe { ofi_tsk_vs_close(self.handle) };
    }
}

pub struct TskFilesystem {
    handle: *mut TskFilesystemHandle,
}

impl TskFilesystem {
    pub fn open(image: &TskImage, offset_bytes: u64) -> Result<Self, TskError> {
        let mut handle = ptr::null_mut();
        let result = unsafe { ofi_tsk_fs_open(image.handle, offset_bytes, &mut handle) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(Self { handle })
    }

    pub fn type_id(&self) -> u32 {
        unsafe { ofi_tsk_fs_type(self.handle) }
    }

    pub fn block_size(&self) -> u32 {
        unsafe { ofi_tsk_fs_block_size(self.handle) }
    }

    pub fn inode_count(&self) -> u64 {
        unsafe { ofi_tsk_fs_inode_count(self.handle) }
    }

    pub fn root_inode(&self) -> u64 {
        unsafe { ofi_tsk_fs_root_inode(self.handle) }
    }

    pub fn directory(&self, inode: u64) -> Result<TskDirectory<'_>, TskError> {
        let mut handle = ptr::null_mut();
        let result = unsafe { ofi_tsk_dir_open(self.handle, inode, &mut handle) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(TskDirectory {
            handle,
            _filesystem: std::marker::PhantomData,
        })
    }

    pub fn file(&self, inode: u64) -> Result<TskFile<'_>, TskError> {
        let mut handle = ptr::null_mut();
        let result = unsafe { ofi_tsk_file_open(self.handle, inode, &mut handle) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(TskFile {
            handle,
            _filesystem: std::marker::PhantomData,
        })
    }
}

impl Drop for TskFilesystem {
    fn drop(&mut self) {
        unsafe { ofi_tsk_fs_close(self.handle) };
    }
}

pub struct TskDirectory<'a> {
    handle: *mut TskDirectoryHandle,
    _filesystem: std::marker::PhantomData<&'a TskFilesystem>,
}

impl TskDirectory<'_> {
    pub fn len(&self) -> usize {
        unsafe { ofi_tsk_dir_count(self.handle) }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn entry(&self, index: usize) -> Result<FileInfo, TskError> {
        let mut entry = FileInfo::default();
        let result = unsafe { ofi_tsk_dir_entry(self.handle, index, &mut entry) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(entry)
    }
}

impl Drop for TskDirectory<'_> {
    fn drop(&mut self) {
        unsafe { ofi_tsk_dir_close(self.handle) };
    }
}

pub struct TskFile<'a> {
    handle: *mut TskFileHandle,
    _filesystem: std::marker::PhantomData<&'a TskFilesystem>,
}

impl TskFile<'_> {
    pub fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, TskError> {
        let result = unsafe {
            ofi_tsk_file_read(
                self.handle,
                offset,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if result < 0 {
            Err(TskError::last())
        } else {
            Ok(result as usize)
        }
    }

    pub fn info(&self) -> Result<FileInfo, TskError> {
        let mut info = FileInfo::default();
        let result = unsafe { ofi_tsk_file_info(self.handle, &mut info) };
        if result != 0 {
            return Err(TskError::last());
        }
        Ok(info)
    }
}

impl Drop for TskFile<'_> {
    fn drop(&mut self) {
        unsafe { ofi_tsk_file_close(self.handle) };
    }
}
