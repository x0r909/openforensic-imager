use std::fmt;

use serde::{Deserialize, Serialize};

pub const MAX_PAGE_SIZE: usize = 64 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 40_000_000;
pub const MAX_PDF_PREVIEW_BYTES: u64 = 64 * 1024 * 1024;
const MAX_JPEG_HEADER_SCAN: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceError {
    message: String,
}

impl SourceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

pub trait ByteSource {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, SourceError>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ByteSource for [u8] {
    fn len(&self) -> u64 {
        self.len() as u64
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, SourceError> {
        let offset = usize::try_from(offset)
            .map_err(|_| SourceError::new("read offset does not fit address space"))?;
        let available = self.len().saturating_sub(offset);
        let count = available.min(buffer.len());
        buffer[..count].copy_from_slice(&self[offset..offset + count]);
        Ok(count)
    }
}

impl ByteSource for Vec<u8> {
    fn len(&self) -> u64 {
        self.as_slice().len() as u64
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> Result<usize, SourceError> {
        self.as_slice().read_at(offset, buffer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerError {
    message: String,
}

impl ViewerError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn source(error: SourceError) -> Self {
        Self::new(error.message)
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ViewerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ViewerError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexRow {
    pub offset: u64,
    pub hex: String,
    pub ascii: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexPage {
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

impl HexPage {
    pub fn rows(&self) -> Vec<HexRow> {
        self.bytes
            .chunks(16)
            .enumerate()
            .map(|(index, bytes)| HexRow {
                offset: self.offset + (index * 16) as u64,
                hex: bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                ascii: bytes
                    .iter()
                    .map(|byte| {
                        if byte.is_ascii_graphic() || *byte == b' ' {
                            char::from(*byte)
                        } else {
                            '.'
                        }
                    })
                    .collect(),
            })
            .collect()
    }
}

pub fn hex_page<S: ByteSource + ?Sized>(
    source: &S,
    offset: u64,
    requested_length: usize,
) -> Result<HexPage, ViewerError> {
    if requested_length == 0 || requested_length > MAX_PAGE_SIZE {
        return Err(ViewerError::new(format!(
            "hex page request exceeds the limit of {MAX_PAGE_SIZE} bytes"
        )));
    }
    let source_length = source.len();
    if offset > source_length {
        return Err(ViewerError::new("hex page offset is outside the source"));
    }
    let available = source_length - offset;
    let count = usize::try_from(available.min(requested_length as u64))
        .map_err(|_| ViewerError::new("hex page length does not fit address space"))?;
    let bytes = read_window(source, offset, count)?;
    if bytes.len() != count {
        return Err(ViewerError::new("source returned an incomplete hex page"));
    }
    Ok(HexPage {
        offset,
        truncated: count < requested_length,
        bytes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEncoding {
    Ascii,
    Utf8,
    Utf16Le,
    Utf16Be,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewContent {
    Text,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPreview {
    pub content: PreviewContent,
    pub encoding: Option<TextEncoding>,
    pub text: String,
    pub truncated: bool,
}

pub fn preview_text<S: ByteSource + ?Sized>(
    source: &S,
    offset: u64,
    max_bytes: usize,
) -> Result<TextPreview, ViewerError> {
    if max_bytes == 0 || max_bytes > MAX_PAGE_SIZE {
        return Err(ViewerError::new(format!(
            "text preview request exceeds the limit of {MAX_PAGE_SIZE} bytes"
        )));
    }
    let source_length = source.len();
    if offset > source_length {
        return Err(ViewerError::new(
            "text preview offset is outside the source",
        ));
    }
    let available = source_length - offset;
    let probe = usize::try_from(available.min((max_bytes + 4) as u64))
        .map_err(|_| ViewerError::new("text preview length does not fit address space"))?;
    let bytes = read_window(source, offset, probe)?;
    let limited = &bytes[..bytes.len().min(max_bytes)];
    let truncated = available > max_bytes as u64;

    if limited.starts_with(&[0xff, 0xfe]) {
        return decode_utf16(&limited[2..], TextEncoding::Utf16Le, truncated);
    }
    if limited.starts_with(&[0xfe, 0xff]) {
        return decode_utf16(&limited[2..], TextEncoding::Utf16Be, truncated);
    }
    if limited.contains(&0) {
        return Ok(binary_preview());
    }
    let (text_bytes, cut_incomplete_character) = truncate_utf8(limited);
    let Ok(text) = std::str::from_utf8(text_bytes) else {
        return Ok(binary_preview());
    };
    let encoding = if text.is_ascii() {
        TextEncoding::Ascii
    } else {
        TextEncoding::Utf8
    };
    Ok(TextPreview {
        content: PreviewContent::Text,
        encoding: Some(encoding),
        text: text.to_owned(),
        truncated: truncated || cut_incomplete_character,
    })
}

fn read_window<S: ByteSource + ?Sized>(
    source: &S,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, ViewerError> {
    let mut output = vec![0_u8; length];
    let mut read = 0;
    while read < length {
        let current_offset = offset
            .checked_add(read as u64)
            .ok_or_else(|| ViewerError::new("source offset overflowed"))?;
        let count = source
            .read_at(current_offset, &mut output[read..])
            .map_err(ViewerError::source)?;
        if count == 0 {
            break;
        }
        if count > output.len() - read {
            return Err(ViewerError::new("source returned an invalid byte count"));
        }
        read = read
            .checked_add(count)
            .ok_or_else(|| ViewerError::new("source read length overflowed"))?;
    }
    output.truncate(read);
    Ok(output)
}

fn truncate_utf8(bytes: &[u8]) -> (&[u8], bool) {
    if std::str::from_utf8(bytes).is_ok() {
        return (bytes, false);
    }
    let minimum = bytes.len().saturating_sub(3);
    for end in (minimum..bytes.len()).rev() {
        if std::str::from_utf8(&bytes[..end]).is_ok() {
            return (&bytes[..end], true);
        }
    }
    (bytes, false)
}

fn decode_utf16(
    bytes: &[u8],
    encoding: TextEncoding,
    truncated: bool,
) -> Result<TextPreview, ViewerError> {
    let complete_length = bytes.len() - (bytes.len() % 2);
    let units = bytes[..complete_length]
        .chunks_exact(2)
        .map(|pair| match encoding {
            TextEncoding::Utf16Le => u16::from_le_bytes([pair[0], pair[1]]),
            TextEncoding::Utf16Be => u16::from_be_bytes([pair[0], pair[1]]),
            TextEncoding::Ascii | TextEncoding::Utf8 => unreachable!(),
        })
        .collect::<Vec<_>>();
    Ok(TextPreview {
        content: PreviewContent::Text,
        encoding: Some(encoding),
        text: String::from_utf16_lossy(&units),
        truncated: truncated || complete_length != bytes.len(),
    })
}

fn binary_preview() -> TextPreview {
    TextPreview {
        content: PreviewContent::Binary,
        encoding: None,
        text: String::new(),
        truncated: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureKind {
    Png,
    Jpeg,
    Gif,
    Pdf,
    Zip,
    Elf,
    Pe,
    Ewf1,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignatureCategory {
    Image,
    Document,
    Archive,
    Executable,
    ForensicImage,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSignature {
    pub kind: SignatureKind,
    pub category: SignatureCategory,
}

pub fn identify_signature<S: ByteSource + ?Sized>(
    source: &S,
) -> Result<FileSignature, ViewerError> {
    let length = usize::try_from(source.len().min(16))
        .map_err(|_| ViewerError::new("signature length does not fit address space"))?;
    let bytes = read_window(source, 0, length)?;
    let (kind, category) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        (SignatureKind::Png, SignatureCategory::Image)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        (SignatureKind::Jpeg, SignatureCategory::Image)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        (SignatureKind::Gif, SignatureCategory::Image)
    } else if bytes.starts_with(b"%PDF-") {
        (SignatureKind::Pdf, SignatureCategory::Document)
    } else if bytes.starts_with(b"PK\x03\x04") {
        (SignatureKind::Zip, SignatureCategory::Archive)
    } else if bytes.starts_with(b"\x7fELF") {
        (SignatureKind::Elf, SignatureCategory::Executable)
    } else if bytes.starts_with(b"MZ") {
        (SignatureKind::Pe, SignatureCategory::Executable)
    } else if bytes.starts_with(b"EVF\x09\x0d\x0a\xff\0") {
        (SignatureKind::Ewf1, SignatureCategory::ForensicImage)
    } else {
        (SignatureKind::Unknown, SignatureCategory::Unknown)
    };
    Ok(FileSignature { kind, category })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewRenderer {
    Hex,
    Image,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewRenderPlan {
    pub renderer: PreviewRenderer,
    pub isolated: bool,
    pub image_pixels: Option<u64>,
}

pub fn render_plan(signature: FileSignature) -> PreviewRenderPlan {
    match signature.kind {
        SignatureKind::Png | SignatureKind::Jpeg | SignatureKind::Gif => PreviewRenderPlan {
            renderer: PreviewRenderer::Image,
            isolated: true,
            image_pixels: None,
        },
        SignatureKind::Pdf => PreviewRenderPlan {
            renderer: PreviewRenderer::Pdf,
            isolated: true,
            image_pixels: None,
        },
        _ => PreviewRenderPlan {
            renderer: PreviewRenderer::Hex,
            isolated: false,
            image_pixels: None,
        },
    }
}

pub fn plan_preview<S: ByteSource + ?Sized>(
    source: &S,
    signature: FileSignature,
) -> Result<PreviewRenderPlan, ViewerError> {
    let observed = identify_signature(source)?;
    if observed.kind != signature.kind {
        return Err(ViewerError::new(format!(
            "preview signature does not match the source: declared {:?}, observed {:?}",
            signature.kind, observed.kind
        )));
    }
    let pixels = match signature.kind {
        SignatureKind::Png => Some(png_pixels(source)?),
        SignatureKind::Gif => Some(gif_pixels(source)?),
        SignatureKind::Jpeg => Some(jpeg_pixels(source)?),
        SignatureKind::Pdf => {
            if source.len() > MAX_PDF_PREVIEW_BYTES {
                return Err(ViewerError::new(format!(
                    "PDF preview exceeds the limit of {MAX_PDF_PREVIEW_BYTES} bytes"
                )));
            }
            None
        }
        _ => None,
    };
    let mut plan = render_plan(signature);
    plan.image_pixels = pixels;
    Ok(plan)
}

fn png_pixels<S: ByteSource + ?Sized>(source: &S) -> Result<u64, ViewerError> {
    let header = read_exact_at(source, 0, 24, "PNG header")?;
    if &header[12..16] != b"IHDR" {
        return Err(ViewerError::new("PNG header is missing IHDR"));
    }
    checked_pixels(
        u32::from_be_bytes(header[16..20].try_into().expect("fixed slice")),
        u32::from_be_bytes(header[20..24].try_into().expect("fixed slice")),
    )
}

fn gif_pixels<S: ByteSource + ?Sized>(source: &S) -> Result<u64, ViewerError> {
    let header = read_exact_at(source, 0, 10, "GIF header")?;
    checked_pixels(
        u16::from_le_bytes(header[6..8].try_into().expect("fixed slice")) as u32,
        u16::from_le_bytes(header[8..10].try_into().expect("fixed slice")) as u32,
    )
}

fn jpeg_pixels<S: ByteSource + ?Sized>(source: &S) -> Result<u64, ViewerError> {
    let mut offset = 2_u64;
    while offset < source.len().min(MAX_JPEG_HEADER_SCAN as u64) {
        let marker = read_exact_at(source, offset, 2, "JPEG marker")?;
        if marker[0] != 0xff {
            return Err(ViewerError::new("JPEG marker is malformed"));
        }
        let marker_kind = marker[1];
        offset += 2;
        if matches!(marker_kind, 0xd8 | 0xd9 | 0x01) || (0xd0..=0xd7).contains(&marker_kind) {
            continue;
        }
        let length = read_exact_at(source, offset, 2, "JPEG segment length")?;
        let length = u16::from_be_bytes([length[0], length[1]]) as u64;
        if length < 2 {
            return Err(ViewerError::new("JPEG segment length is invalid"));
        }
        if matches!(
            marker_kind,
            0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf
        ) {
            let sof = read_exact_at(source, offset + 2, 5, "JPEG frame header")?;
            let height = u16::from_be_bytes([sof[1], sof[2]]) as u32;
            let width = u16::from_be_bytes([sof[3], sof[4]]) as u32;
            return checked_pixels(width, height);
        }
        offset = offset
            .checked_add(length)
            .ok_or_else(|| ViewerError::new("JPEG segment offset overflowed"))?;
    }
    Err(ViewerError::new(
        "JPEG dimensions were not found within the header limit",
    ))
}

fn checked_pixels(width: u32, height: u32) -> Result<u64, ViewerError> {
    if width == 0 || height == 0 {
        return Err(ViewerError::new("image dimensions must be non-zero"));
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| ViewerError::new("image pixel count overflowed"))?;
    if pixels > MAX_IMAGE_PIXELS {
        return Err(ViewerError::new(format!(
            "image pixel count {pixels} exceeds the limit of {MAX_IMAGE_PIXELS}"
        )));
    }
    Ok(pixels)
}

fn read_exact_at<S: ByteSource + ?Sized>(
    source: &S,
    offset: u64,
    length: usize,
    label: &str,
) -> Result<Vec<u8>, ViewerError> {
    if offset
        .checked_add(length as u64)
        .is_none_or(|end| end > source.len())
    {
        return Err(ViewerError::new(format!("{label} is truncated")));
    }
    let bytes = read_window(source, offset, length)?;
    if bytes.len() != length {
        return Err(ViewerError::new(format!("{label} is truncated")));
    }
    Ok(bytes)
}
