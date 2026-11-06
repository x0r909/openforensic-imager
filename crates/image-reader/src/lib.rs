mod ewf;
mod raw;
mod source;

pub use ewf::EwfImageReader;
pub use raw::RawReader;
pub use source::{
    ImageError, ImageErrorCode, ImageFormat, ImageMetadata, ImageReader, SourceFingerprint,
};

pub type ReadError = ImageError;

pub fn open_image(path: &std::path::Path) -> Result<Box<dyn ImageReader>, ImageError> {
    // Canonicalize before touching libewf: `libewf_check_file_signature`
    // fails with "unable to set filename" when handed a relative path, so a
    // caller that passes `./evidence.raw` or a repo-relative path would get a
    // spurious Backend error instead of a raw read. An absolute path is also
    // what the worker's sandbox grants were built from, so reader and policy
    // agree on the same path.
    let path = path.canonicalize().map_err(|error| {
        ImageError::new(ImageErrorCode::Backend, format!("canonicalize: {error}"))
    })?;
    match libewf_bindings::is_ewf(&path) {
        Ok(true) => {
            EwfImageReader::open(&path).map(|reader| Box::new(reader) as Box<dyn ImageReader>)
        }
        Ok(false) => {
            RawReader::open_read_only(&path).map(|reader| Box::new(reader) as Box<dyn ImageReader>)
        }
        Err(error) => Err(ImageError::new(
            ImageErrorCode::Backend,
            format!("{}: {}", error.operation(), error.message()),
        )),
    }
}
