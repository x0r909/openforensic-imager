use std::collections::BTreeMap;

use digest::Digest;
use md5::Md5;
use sha1::Sha1;
use sha2::{Sha256, Sha512};

use crate::{HashAlgorithm, HashDigest};

enum HasherState {
    Md5(Md5),
    Sha1(Sha1),
    Sha256(Sha256),
    Sha512(Sha512),
}

impl HasherState {
    fn new(algorithm: HashAlgorithm) -> Self {
        match algorithm {
            HashAlgorithm::Md5 => Self::Md5(Md5::new()),
            HashAlgorithm::Sha1 => Self::Sha1(Sha1::new()),
            HashAlgorithm::Sha256 => Self::Sha256(Sha256::new()),
            HashAlgorithm::Sha512 => Self::Sha512(Sha512::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::Md5(hasher) => hasher.update(bytes),
            Self::Sha1(hasher) => hasher.update(bytes),
            Self::Sha256(hasher) => hasher.update(bytes),
            Self::Sha512(hasher) => hasher.update(bytes),
        }
    }

    fn finish(self, algorithm: HashAlgorithm) -> (HashAlgorithm, HashDigest) {
        let digest = match self {
            Self::Md5(hasher) => hasher.finalize().to_vec(),
            Self::Sha1(hasher) => hasher.finalize().to_vec(),
            Self::Sha256(hasher) => hasher.finalize().to_vec(),
            Self::Sha512(hasher) => hasher.finalize().to_vec(),
        };
        (algorithm, HashDigest::new(digest))
    }
}

pub struct StreamingHasher {
    states: Vec<(HashAlgorithm, HasherState)>,
}

impl StreamingHasher {
    pub fn new(algorithms: &[HashAlgorithm]) -> Self {
        let mut algorithms = algorithms.to_vec();
        algorithms.sort_unstable();
        algorithms.dedup();
        let states = algorithms
            .into_iter()
            .map(|algorithm| (algorithm, HasherState::new(algorithm)))
            .collect();
        Self { states }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for (_, state) in &mut self.states {
            state.update(bytes);
        }
    }

    pub fn finish(self) -> BTreeMap<HashAlgorithm, HashDigest> {
        self.states
            .into_iter()
            .map(|(algorithm, state)| state.finish(algorithm))
            .collect()
    }
}

