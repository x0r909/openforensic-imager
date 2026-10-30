use std::str::FromStr;

use forensic_core::{ByteRange, CaseId, CoreError, ErrorCode};
use uuid::Uuid;

#[test]
fn ids_round_trip_as_uuid_text() {
    let uuid = Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888);
    let id = CaseId::from(uuid);

    assert_eq!(CaseId::from_str(&id.to_string()).unwrap(), id);
    assert_eq!(Uuid::from(id), uuid);
}

#[test]
fn zero_length_range_at_end_is_valid() {
    let range = ByteRange::new(16, 0, 16).unwrap();

    assert_eq!(range.offset(), 16);
    assert_eq!(range.length(), 0);
    assert_eq!(range.end(), 16);
}

#[test]
fn full_subject_range_is_valid() {
    let range = ByteRange::new(0, 16, 16).unwrap();

    assert_eq!(range.end(), 16);
}

#[test]
fn range_past_subject_is_rejected() {
    let error = ByteRange::new(15, 2, 16).unwrap_err();

    assert_eq!(error.code(), ErrorCode::RangeOutOfBounds);
    assert!(!error.retryable());
}

#[test]
fn overflowing_range_is_rejected() {
    let error = ByteRange::new(u64::MAX, 1, u64::MAX).unwrap_err();

    assert_eq!(error.code(), ErrorCode::RangeOverflow);
    assert!(!error.retryable());
}

#[test]
fn internal_errors_are_retryable() {
    let error = CoreError::internal("worker stopped unexpectedly");

    assert_eq!(error.code(), ErrorCode::Internal);
    assert!(error.retryable());
}
