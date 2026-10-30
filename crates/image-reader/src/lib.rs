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
    match libewf_bindings::is_ewf(path) {
        Ok(true) => {
            EwfImageReader::open(path).map(|reader| Box::new(reader) as Box<dyn ImageReader>)
        }
        Ok(false) => {
            RawReader::open_read_only(path).map(|reader| Box::new(reader) as Box<dyn ImageReader>)
        }
        Err(error) => Err(ImageError::new(
            ImageErrorCode::Backend,
            format!("{}: {}", error.operation(), error.message()),
        )),
    }
}
