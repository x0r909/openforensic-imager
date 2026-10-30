use std::collections::BTreeMap;

use hashing::{HashAlgorithm, HashDigest, StreamingHasher};
use proptest::prelude::*;

fn one_shot(input: &[u8]) -> BTreeMap<HashAlgorithm, HashDigest> {
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    hasher.update(input);
    hasher.finish()
}

fn chunked(input: &[u8], splits: &[usize]) -> BTreeMap<HashAlgorithm, HashDigest> {
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    let mut start = 0;
    for split in splits {
        let end = (*split).min(input.len());
        hasher.update(&input[start..end]);
        start = end;
    }
    hasher.update(&input[start..]);
    hasher.finish()
}

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

    #[test]
    fn chunking_does_not_change_sha256(input in proptest::collection::vec(any::<u8>(), 0..4096), splits in proptest::collection::vec(0usize..4096, 0..32)) {
        let mut sorted_splits = splits;
        sorted_splits.sort_unstable();

        prop_assert_eq!(chunked(&input, &sorted_splits), one_shot(&input));
    }
}

