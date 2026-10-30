use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use hashing::{HashAlgorithm, HashDigest, StreamingHasher};

fn hashes(input: &[u8]) -> BTreeMap<HashAlgorithm, HashDigest> {
    let mut hasher = StreamingHasher::new(&[
        HashAlgorithm::Md5,
        HashAlgorithm::Sha1,
        HashAlgorithm::Sha256,
        HashAlgorithm::Sha512,
    ]);
    hasher.update(input);
    hasher.finish()
}

#[test]
fn hashes_empty_input_with_known_vectors() {
    let result = hashes(b"");

    assert_eq!(
        result[&HashAlgorithm::Md5].to_hex(),
        "d41d8cd98f00b204e9800998ecf8427e"
    );
    assert_eq!(
        result[&HashAlgorithm::Sha1].to_hex(),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
    assert_eq!(
        result[&HashAlgorithm::Sha256].to_hex(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        result[&HashAlgorithm::Sha512].to_hex(),
        concat!(
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce",
            "47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        )
    );
}

#[test]
fn hashes_abc_with_known_vectors() {
    let result = hashes(b"abc");

    assert_eq!(
        result[&HashAlgorithm::Md5].to_hex(),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        result[&HashAlgorithm::Sha1].to_hex(),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        result[&HashAlgorithm::Sha256].to_hex(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        result[&HashAlgorithm::Sha512].to_hex(),
        concat!(
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a",
            "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        )
    );
}

#[test]
fn sha256_matches_coreutils_on_repository_readme() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../README.md");
    let bytes = std::fs::read(&path).unwrap();
    let mut hasher = StreamingHasher::new(&[HashAlgorithm::Sha256]);
    hasher.update(&bytes);
    let expected = hasher.finish()[&HashAlgorithm::Sha256].to_hex();

    let output = Command::new("sha256sum").arg(&path).output().unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();

    assert_eq!(actual, expected);
}

