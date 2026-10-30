use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use hashing::{HashAlgorithm, StreamingHasher};
use serde::{Deserialize, Serialize};

const GENERATOR_VERSION: &str = "phase1-v1";
const MAX_BLOCK_SIZE: u32 = 1024 * 1024;
const IO_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RawLayout {
    Empty,
    Dense {
        length: u64,
        block_size: u32,
    },
    Sparse {
        length: u64,
        block_size: u32,
        data_blocks: Vec<u64>,
    },
    Split {
        segment_lengths: Vec<u64>,
        block_size: u32,
    },
}

impl RawLayout {
    fn block_size(&self) -> u32 {
        match self {
            Self::Empty => 512,
            Self::Dense { block_size, .. }
            | Self::Sparse { block_size, .. }
            | Self::Split { block_size, .. } => *block_size,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureSegment {
    pub path: String,
    pub length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureManifest {
    pub generator_version: String,
    pub seed: u64,
    pub layout: RawLayout,
    pub byte_length: u64,
    pub block_size: u32,
    pub segments: Vec<FixtureSegment>,
}

#[derive(Debug)]
pub enum FixtureError {
    Io(std::io::Error),
    InvalidLayout(String),
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "fixture I/O error: {error}"),
            Self::InvalidLayout(message) => write!(formatter, "invalid fixture layout: {message}"),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<std::io::Error> for FixtureError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn generate_raw_fixture(
    output_dir: &Path,
    seed: u64,
    layout: RawLayout,
) -> Result<FixtureManifest, FixtureError> {
    validate(&layout)?;
    std::fs::create_dir_all(output_dir)?;

    let segment_lengths = match &layout {
        RawLayout::Empty => vec![0],
        RawLayout::Dense { length, .. } => vec![*length],
        RawLayout::Sparse { length, .. } => vec![*length],
        RawLayout::Split {
            segment_lengths, ..
        } => segment_lengths.clone(),
    };
    let byte_length = segment_lengths.iter().try_fold(0_u64, |total, length| {
        total.checked_add(*length).ok_or_else(|| {
            FixtureError::InvalidLayout("total fixture length exceeds u64".to_owned())
        })
    })?;
    let block_size = layout.block_size();

    let mut segments = Vec::with_capacity(segment_lengths.len());
    for (index, length) in segment_lengths.into_iter().enumerate() {
        let filename = format!("raw-{seed:016x}-{index:04}.raw");
        let path = output_dir.join(&filename);
        write_segment(&path, seed, index as u64, length, &layout)?;
        segments.push(FixtureSegment {
            path: filename,
            length,
            sha256: sha256_file(&path)?,
        });
    }

    Ok(FixtureManifest {
        generator_version: GENERATOR_VERSION.to_owned(),
        seed,
        layout,
        byte_length,
        block_size,
        segments,
    })
}

fn validate(layout: &RawLayout) -> Result<(), FixtureError> {
    let block_size = layout.block_size();
    if block_size == 0 || block_size > MAX_BLOCK_SIZE {
        return Err(FixtureError::InvalidLayout(format!(
            "block size must be between 1 and {MAX_BLOCK_SIZE}"
        )));
    }
    match layout {
        RawLayout::Sparse {
            length,
            block_size,
            data_blocks,
        } => {
            let block_count = length.div_ceil(u64::from(*block_size));
            for block in data_blocks {
                if *block >= block_count {
                    return Err(FixtureError::InvalidLayout(format!(
                        "sparse block {block} is outside {block_count} blocks"
                    )));
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn write_segment(
    path: &PathBuf,
    seed: u64,
    segment_index: u64,
    length: u64,
    layout: &RawLayout,
) -> Result<(), FixtureError> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    match layout {
        RawLayout::Empty => {}
        RawLayout::Dense { .. } | RawLayout::Split { .. } => {
            let mut remaining = length;
            let mut state = SplitMix64(seed ^ segment_index.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let mut buffer = vec![0_u8; IO_CHUNK_SIZE];
            while remaining > 0 {
                let count = remaining.min(IO_CHUNK_SIZE as u64) as usize;
                fill_pseudorandom(&mut buffer[..count], &mut state);
                file.write_all(&buffer[..count])?;
                remaining -= count as u64;
            }
        }
        RawLayout::Sparse {
            block_size,
            data_blocks,
            ..
        } => {
            file.set_len(length)?;
            let mut state = SplitMix64(seed ^ segment_index.wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let mut buffer = vec![0_u8; *block_size as usize];
            for block in data_blocks {
                let offset = block.checked_mul(u64::from(*block_size)).ok_or_else(|| {
                    FixtureError::InvalidLayout("sparse offset overflow".to_owned())
                })?;
                let count = (length - offset).min(u64::from(*block_size)) as usize;
                fill_pseudorandom(&mut buffer[..count], &mut state);
                file.seek(SeekFrom::Start(offset))?;
                file.write_all(&buffer[..count])?;
            }
        }
    }
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, FixtureError> {
    let mut file = File::open(path)?;
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    let mut buffer = vec![0_u8; IO_CHUNK_SIZE];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finish()[&HashAlgorithm::Sha256].to_hex())
}

fn fill_pseudorandom(buffer: &mut [u8], state: &mut SplitMix64) {
    for chunk in buffer.chunks_mut(8) {
        let bytes = state.next_u64().to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}
