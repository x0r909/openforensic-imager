use std::ffi::{CStr, CString, c_char, c_int};
use std::fmt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Mutex;

use libc::{size_t, ssize_t};

#[repr(C)]
struct LibewfHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct LibewfError {
    _private: [u8; 0],
}

#[link(name = "ewf")]
unsafe extern "C" {
    fn libewf_get_access_flags_read() -> c_int;
    fn libewf_get_access_flags_write() -> c_int;
    fn libewf_check_file_signature(filename: *const c_char, error: *mut *mut LibewfError) -> c_int;
    fn libewf_glob(
        filename: *const c_char,
        filename_length: size_t,
        format: u8,
        filenames: *mut *mut *mut c_char,
        number_of_filenames: *mut c_int,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_glob_free(
        filenames: *mut *mut c_char,
        number_of_filenames: c_int,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_error_free(error: *mut *mut LibewfError);
    fn libewf_error_sprint(error: *mut LibewfError, string: *mut c_char, size: size_t) -> c_int;
    fn libewf_handle_initialize(
        handle: *mut *mut LibewfHandle,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_free(handle: *mut *mut LibewfHandle, error: *mut *mut LibewfError) -> c_int;
    fn libewf_handle_open(
        handle: *mut LibewfHandle,
        filenames: *mut *mut c_char,
        number_of_filenames: c_int,
        access_flags: c_int,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_close(handle: *mut LibewfHandle, error: *mut *mut LibewfError) -> c_int;
    fn libewf_handle_set_bytes_per_sector(
        handle: *mut LibewfHandle,
        bytes_per_sector: u32,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_get_bytes_per_sector(
        handle: *mut LibewfHandle,
        bytes_per_sector: *mut u32,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_set_sectors_per_chunk(
        handle: *mut LibewfHandle,
        sectors_per_chunk: u32,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_set_maximum_segment_size(
        handle: *mut LibewfHandle,
        maximum_segment_size: u64,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_set_media_size(
        handle: *mut LibewfHandle,
        media_size: u64,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_get_media_size(
        handle: *mut LibewfHandle,
        media_size: *mut u64,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_set_media_type(
        handle: *mut LibewfHandle,
        media_type: u8,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_set_media_flags(
        handle: *mut LibewfHandle,
        media_flags: u8,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_set_format(
        handle: *mut LibewfHandle,
        format: u8,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_set_compression_values(
        handle: *mut LibewfHandle,
        compression_level: i8,
        compression_flags: u8,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_write_buffer(
        handle: *mut LibewfHandle,
        buffer: *const libc::c_void,
        buffer_size: size_t,
        error: *mut *mut LibewfError,
    ) -> ssize_t;
    fn libewf_handle_write_finalize(
        handle: *mut LibewfHandle,
        error: *mut *mut LibewfError,
    ) -> ssize_t;
    fn libewf_handle_read_random(
        handle: *mut LibewfHandle,
        buffer: *mut libc::c_void,
        buffer_size: size_t,
        offset: i64,
        error: *mut *mut LibewfError,
    ) -> ssize_t;
    fn libewf_handle_segment_files_corrupted(
        handle: *mut LibewfHandle,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_get_md5_hash(
        handle: *mut LibewfHandle,
        hash: *mut u8,
        size: size_t,
        error: *mut *mut LibewfError,
    ) -> c_int;
    fn libewf_handle_get_sha1_hash(
        handle: *mut LibewfHandle,
        hash: *mut u8,
        size: size_t,
        error: *mut *mut LibewfError,
    ) -> c_int;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EwfCompression {
    None,
    Fast,
    Best,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EwfWriteOptions {
    pub bytes_per_sector: u32,
    pub sectors_per_chunk: u32,
    pub maximum_segment_size: u64,
    pub compression: EwfCompression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EwfError {
    operation: &'static str,
    message: String,
}

impl EwfError {
    fn new(operation: &'static str, message: impl Into<String>) -> Self {
        Self {
            operation,
            message: message.into(),
        }
    }

    pub fn operation(&self) -> &'static str {
        self.operation
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for EwfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.message)
    }
}

impl std::error::Error for EwfError {}

pub fn is_ewf(path: &Path) -> Result<bool, EwfError> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| EwfError::new("check_file_signature", error.to_string()))?;
    if metadata.len() < 8 {
        return Ok(false);
    }
    let filename = path_c_string(path, "check_file_signature")?;
    let mut error = ptr::null_mut();
    let result = unsafe { libewf_check_file_signature(filename.as_ptr(), &mut error) };
    match result {
        1 => {
            discard_error(error);
            Ok(true)
        }
        0 => {
            discard_error(error);
            Ok(false)
        }
        _ => Err(take_error("check_file_signature", error)),
    }
}

pub struct EwfWriter {
    handle: *mut LibewfHandle,
    target: PathBuf,
    media_size: u64,
    written: u64,
    finalized: bool,
}

impl EwfWriter {
    pub fn create(
        target: &Path,
        media_size: u64,
        options: EwfWriteOptions,
    ) -> Result<Self, EwfError> {
        if options.bytes_per_sector == 0
            || !media_size.is_multiple_of(u64::from(options.bytes_per_sector))
        {
            return Err(EwfError::new(
                "create",
                "media size must be a non-zero multiple of the sector size",
            ));
        }

        let mut handle = ptr::null_mut();
        let mut error = ptr::null_mut();
        let result = unsafe { libewf_handle_initialize(&mut handle, &mut error) };
        if result != 1 {
            return Err(take_error("initialize", error));
        }

        let writer = Self {
            handle,
            target: target.to_path_buf(),
            media_size,
            written: 0,
            finalized: false,
        };

        writer.open_for_write(target)?;
        writer.configure(media_size, options)?;
        Ok(writer)
    }

    fn configure(&self, media_size: u64, options: EwfWriteOptions) -> Result<(), EwfError> {
        call_set(
            "set_bytes_per_sector",
            options.bytes_per_sector as c_int,
            |error| unsafe {
                libewf_handle_set_bytes_per_sector(self.handle, options.bytes_per_sector, error)
            },
        )?;
        call_set(
            "set_sectors_per_chunk",
            options.sectors_per_chunk as c_int,
            |error| unsafe {
                libewf_handle_set_sectors_per_chunk(self.handle, options.sectors_per_chunk, error)
            },
        )?;
        call_set(
            "set_maximum_segment_size",
            options.maximum_segment_size as c_int,
            |error| unsafe {
                libewf_handle_set_maximum_segment_size(
                    self.handle,
                    options.maximum_segment_size,
                    error,
                )
            },
        )?;
        call_set("set_media_type", 1, |error| unsafe {
            libewf_handle_set_media_type(self.handle, 1, error)
        })?;
        call_set("set_media_flags", 2, |error| unsafe {
            libewf_handle_set_media_flags(self.handle, 2, error)
        })?;
        call_set("set_format", 6, |error| unsafe {
            libewf_handle_set_format(self.handle, 6, error)
        })?;
        let compression_level: i8 = match options.compression {
            EwfCompression::None => 0,
            EwfCompression::Fast => 1,
            EwfCompression::Best => 2,
        };
        call_set(
            "set_compression_values",
            compression_level.into(),
            |error| unsafe {
                libewf_handle_set_compression_values(self.handle, compression_level, 0, error)
            },
        )?;
        call_set("set_media_size", media_size as c_int, |error| unsafe {
            libewf_handle_set_media_size(self.handle, media_size, error)
        })
    }

    fn open_for_write(&self, target: &Path) -> Result<(), EwfError> {
        let filename = path_c_string(target, "open")?;
        let mut names = [filename.as_ptr() as *mut c_char];
        let flags = unsafe { libewf_get_access_flags_write() };
        let mut error = ptr::null_mut();
        let result =
            unsafe { libewf_handle_open(self.handle, names.as_mut_ptr(), 1, flags, &mut error) };
        ensure_one("open", result, error)
    }

    pub fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), EwfError> {
        while !bytes.is_empty() {
            let mut error = ptr::null_mut();
            let count = unsafe {
                libewf_handle_write_buffer(
                    self.handle,
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    &mut error,
                )
            };
            if count <= 0 {
                return Err(take_error("write_buffer", error));
            }
            let count = count as usize;
            self.written = self
                .written
                .checked_add(count as u64)
                .ok_or_else(|| EwfError::new("write_buffer", "written byte count overflowed"))?;
            bytes = &bytes[count..];
        }
        Ok(())
    }

    pub fn finalize(mut self) -> Result<Vec<PathBuf>, EwfError> {
        if self.written != self.media_size {
            return Err(EwfError::new(
                "finalize",
                format!(
                    "wrote {} bytes but media size is {}",
                    self.written, self.media_size
                ),
            ));
        }
        let mut error = ptr::null_mut();
        let result = unsafe { libewf_handle_write_finalize(self.handle, &mut error) };
        if result < 0 {
            return Err(take_error("write_finalize", error));
        }
        self.close()?;
        self.finalized = true;
        discover_segments(&self.target)
    }

    fn close(&mut self) -> Result<(), EwfError> {
        if self.handle.is_null() {
            return Ok(());
        }
        let mut error = ptr::null_mut();
        let result = unsafe { libewf_handle_close(self.handle, &mut error) };
        if result != 0 {
            let failure = take_error("close", error);
            self.free_handle();
            return Err(failure);
        }
        self.free_handle();
        Ok(())
    }

    fn free_handle(&mut self) {
        if !self.handle.is_null() {
            let mut error = ptr::null_mut();
            unsafe {
                libewf_handle_free(&mut self.handle, &mut error);
            }
            discard_error(error);
        }
    }
}

impl Drop for EwfWriter {
    fn drop(&mut self) {
        if !self.finalized {
            let _ = self.close();
        }
    }
}

struct ReaderHandle(*mut LibewfHandle);

unsafe impl Send for ReaderHandle {}

impl Drop for ReaderHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            let mut error = ptr::null_mut();
            unsafe {
                libewf_handle_close(self.0, &mut error);
            }
            discard_error(error);
            let mut error = ptr::null_mut();
            unsafe {
                libewf_handle_free(&mut self.0, &mut error);
            }
            discard_error(error);
        }
    }
}

pub struct EwfReader {
    handle: Mutex<ReaderHandle>,
    media_size: u64,
    bytes_per_sector: u32,
    stored_md5: Option<String>,
    stored_sha1: Option<String>,
}

impl fmt::Debug for EwfReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EwfReader")
            .field("media_size", &self.media_size)
            .field("bytes_per_sector", &self.bytes_per_sector)
            .finish_non_exhaustive()
    }
}

impl EwfReader {
    pub fn open(path: &Path) -> Result<Self, EwfError> {
        let filename = path_c_string(path, "open")?;
        let mut handle = ptr::null_mut();
        let mut error = ptr::null_mut();
        let result = unsafe { libewf_handle_initialize(&mut handle, &mut error) };
        if result != 1 {
            return Err(take_error("initialize", error));
        }

        let flags = unsafe { libewf_get_access_flags_read() };
        let mut globbed_names = ptr::null_mut();
        let mut globbed_count = 0;
        let mut error = ptr::null_mut();
        let result = unsafe {
            libewf_glob(
                filename.as_ptr(),
                filename.as_bytes().len(),
                0,
                &mut globbed_names,
                &mut globbed_count,
                &mut error,
            )
        };
        if result != 1 {
            unsafe {
                libewf_handle_free(&mut handle, ptr::null_mut());
            }
            return Err(take_error("glob", error));
        }
        let mut error = ptr::null_mut();
        let result =
            unsafe { libewf_handle_open(handle, globbed_names, globbed_count, flags, &mut error) };
        let mut glob_free_error = ptr::null_mut();
        unsafe {
            libewf_glob_free(globbed_names, globbed_count, &mut glob_free_error);
        }
        discard_error(glob_free_error);
        if result != 1 {
            if !handle.is_null() {
                let mut free_error = ptr::null_mut();
                unsafe {
                    libewf_handle_free(&mut handle, &mut free_error);
                }
                discard_error(free_error);
            }
            return Err(take_error("open", error));
        }

        let mut media_size = 0_u64;
        let mut error = ptr::null_mut();
        let result = unsafe { libewf_handle_get_media_size(handle, &mut media_size, &mut error) };
        if result != 1 {
            unsafe {
                libewf_handle_close(handle, ptr::null_mut());
                libewf_handle_free(&mut handle, ptr::null_mut());
            }
            return Err(take_error("get_media_size", error));
        }

        let mut bytes_per_sector = 0_u32;
        let mut error = ptr::null_mut();
        let result = unsafe {
            libewf_handle_get_bytes_per_sector(handle, &mut bytes_per_sector, &mut error)
        };
        if result != 1 {
            unsafe {
                libewf_handle_close(handle, ptr::null_mut());
                libewf_handle_free(&mut handle, ptr::null_mut());
            }
            return Err(take_error("get_bytes_per_sector", error));
        }

        let stored_md5 = read_binary_hash(handle, 16, libewf_handle_get_md5_hash)?;
        let stored_sha1 = read_binary_hash(handle, 20, libewf_handle_get_sha1_hash)?;

        Ok(Self {
            handle: Mutex::new(ReaderHandle(handle)),
            media_size,
            bytes_per_sector,
            stored_md5,
            stored_sha1,
        })
    }

    pub fn media_size(&self) -> u64 {
        self.media_size
    }

    pub fn bytes_per_sector(&self) -> u32 {
        self.bytes_per_sector
    }

    pub fn stored_md5(&self) -> Option<&str> {
        self.stored_md5.as_deref()
    }

    pub fn stored_sha1(&self) -> Option<&str> {
        self.stored_sha1.as_deref()
    }

    pub fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, EwfError> {
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or_else(|| EwfError::new("read_random", "read range overflowed"))?;
        if end > self.media_size {
            return Err(EwfError::new(
                "read_random",
                "read range exceeds media size",
            ));
        }
        let handle = self
            .handle
            .lock()
            .map_err(|_| EwfError::new("read_random", "reader lock is poisoned"))?;
        let mut error = ptr::null_mut();
        let result = unsafe {
            libewf_handle_read_random(
                handle.0,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                offset as i64,
                &mut error,
            )
        };
        if result < 0 {
            Err(take_error("read_random", error))
        } else {
            discard_error(error);
            Ok(result as usize)
        }
    }

    pub fn is_corrupted(&self) -> Result<bool, EwfError> {
        let handle = self
            .handle
            .lock()
            .map_err(|_| EwfError::new("segment_files_corrupted", "reader lock is poisoned"))?;
        let mut error = ptr::null_mut();
        let result = unsafe { libewf_handle_segment_files_corrupted(handle.0, &mut error) };
        match result {
            1 => {
                discard_error(error);
                Ok(true)
            }
            0 => {
                discard_error(error);
                Ok(false)
            }
            _ => Err(take_error("segment_files_corrupted", error)),
        }
    }
}

fn call_set(
    operation: &'static str,
    _value: c_int,
    call: impl FnOnce(*mut *mut LibewfError) -> c_int,
) -> Result<(), EwfError> {
    let mut error = ptr::null_mut();
    let result = call(&mut error);
    ensure_one(operation, result, error)
}

fn ensure_one(
    operation: &'static str,
    result: c_int,
    error: *mut LibewfError,
) -> Result<(), EwfError> {
    if result == 1 {
        discard_error(error);
        Ok(())
    } else {
        Err(take_error(operation, error))
    }
}

fn take_error(operation: &'static str, mut error: *mut LibewfError) -> EwfError {
    let message = if error.is_null() {
        "libewf operation failed".to_owned()
    } else {
        let mut buffer = [0_i8; 4096];
        let count = unsafe { libewf_error_sprint(error, buffer.as_mut_ptr(), buffer.len()) };
        let message = if count > 0 {
            unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        } else {
            "libewf operation failed".to_owned()
        };
        unsafe {
            libewf_error_free(&mut error);
        }
        message
    };
    EwfError::new(operation, message)
}

fn discard_error(mut error: *mut LibewfError) {
    if !error.is_null() {
        unsafe {
            libewf_error_free(&mut error);
        }
    }
}

fn read_binary_hash(
    handle: *mut LibewfHandle,
    size: usize,
    get_hash: unsafe extern "C" fn(
        *mut LibewfHandle,
        *mut u8,
        size_t,
        *mut *mut LibewfError,
    ) -> c_int,
) -> Result<Option<String>, EwfError> {
    let mut value = vec![0_u8; size];
    let mut error = ptr::null_mut();
    let result = unsafe { get_hash(handle, value.as_mut_ptr(), value.len(), &mut error) };
    if result == 0 {
        discard_error(error);
        return Ok(None);
    }
    if result != 1 {
        return Err(take_error("get_hash", error));
    }
    discard_error(error);
    let mut hex = String::with_capacity(value.len() * 2);
    for byte in value {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(Some(hex))
}

fn path_c_string(path: &Path, operation: &'static str) -> Result<CString, EwfError> {
    use std::os::unix::ffi::OsStrExt;

    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| EwfError::new(operation, "path contains a NUL byte"))
}

fn discover_segments(target: &Path) -> Result<Vec<PathBuf>, EwfError> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let file_name = target
        .file_name()
        .ok_or_else(|| EwfError::new("discover_segments", "target has no filename"))?;
    let prefix = file_name.to_string_lossy();
    let mut segments = Vec::new();
    for entry in std::fs::read_dir(parent)
        .map_err(|error| EwfError::new("discover_segments", error.to_string()))?
    {
        let entry = entry.map_err(|error| EwfError::new("discover_segments", error.to_string()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(prefix.as_ref())
            && name
                .rsplit_once('.')
                .is_some_and(|(_, extension)| is_ewf_extension(extension))
        {
            segments.push(entry.path());
        }
    }
    segments.sort();
    if segments.is_empty() {
        return Err(EwfError::new(
            "discover_segments",
            "libewf did not create a segment file",
        ));
    }
    Ok(segments)
}

fn is_ewf_extension(extension: &str) -> bool {
    let bytes = extension.as_bytes();
    bytes.len() == 3
        && bytes[0].eq_ignore_ascii_case(&b'e')
        && (bytes[1].is_ascii_digit() || bytes[1].is_ascii_alphabetic())
        && (bytes[2].is_ascii_digit() || bytes[2].is_ascii_alphabetic())
}
