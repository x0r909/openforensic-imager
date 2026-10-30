use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tsk_bindings::{TskError, TskImage, TskVolumeSystem};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartitionScheme {
    Mbr,
    Gpt,
    Other(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Partition {
    pub address: u64,
    pub table_number: i8,
    pub slot_number: i8,
    pub start_sector: u64,
    pub sector_count: u64,
    pub offset_bytes: u64,
    pub length_bytes: u64,
    pub description: String,
    pub flags: u32,
    pub allocated: bool,
    pub metadata: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartitionAnalysis {
    pub scheme: PartitionScheme,
    pub block_size: u32,
    pub partitions: Vec<Partition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionError {
    message: String,
}

impl PartitionError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PartitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PartitionError {}

impl From<TskError> for PartitionError {
    fn from(error: TskError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

pub fn analyze_partitions(path: &Path) -> Result<PartitionAnalysis, PartitionError> {
    let image = TskImage::open(path)?;
    let volume_system = TskVolumeSystem::open(&image)?;
    let scheme = match volume_system.type_id() {
        0x0001 => PartitionScheme::Mbr,
        0x0010 => PartitionScheme::Gpt,
        other => PartitionScheme::Other(other),
    };
    let block_size = volume_system.block_size();
    let mut partitions = Vec::with_capacity(volume_system.partition_count() as usize);
    for index in 0..volume_system.partition_count() {
        let partition = volume_system.partition(index)?;
        let length_bytes = partition
            .sector_count
            .checked_mul(u64::from(block_size))
            .ok_or_else(|| PartitionError {
                message: "partition length overflowed".to_owned(),
            })?;
        let offset_bytes = partition
            .start_sector
            .checked_mul(u64::from(block_size))
            .ok_or_else(|| PartitionError {
                message: "partition offset overflowed".to_owned(),
            })?;
        partitions.push(Partition {
            address: partition.address,
            table_number: partition.table_number,
            slot_number: partition.slot_number,
            start_sector: partition.start_sector,
            sector_count: partition.sector_count,
            offset_bytes,
            length_bytes,
            description: partition.description(),
            flags: partition.flags,
            allocated: partition.is_allocated(),
            metadata: partition.is_metadata(),
        });
    }
    Ok(PartitionAnalysis {
        scheme,
        block_size,
        partitions,
    })
}
