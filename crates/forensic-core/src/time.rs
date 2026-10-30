use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UtcTimestamp(OffsetDateTime);

impl UtcTimestamp {
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    pub fn from_unix_timestamp_nanos(nanoseconds: i128) -> Option<Self> {
        OffsetDateTime::from_unix_timestamp_nanos(nanoseconds)
            .ok()
            .map(Self)
    }

    pub fn unix_timestamp_nanos(&self) -> i128 {
        self.0.unix_timestamp_nanos()
    }

    pub fn to_rfc3339(&self) -> String {
        self.0
            .format(&Rfc3339)
            .expect("UTC timestamps are representable as RFC 3339")
    }
}

impl FromStr for UtcTimestamp {
    type Err = time::error::Parse;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        OffsetDateTime::parse(value, &Rfc3339).map(Self)
    }
}

impl fmt::Display for UtcTimestamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_rfc3339())
    }
}

impl Serialize for UtcTimestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_rfc3339())
    }
}

impl<'de> Deserialize<'de> for UtcTimestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

