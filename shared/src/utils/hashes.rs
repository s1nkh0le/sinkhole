use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

pub fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex::encode(hasher.finalize())
}

/// Verify a plaintext token against an expected lowercase hex SHA-256 digest in
/// constant time.
///
/// - Hashes `token` and compares the 32 raw digest bytes against `expected_hex`
///   using a constant-time comparison, so a mismatch leaks no timing signal
///   about how many leading bytes matched.
/// - Zeroizes the intermediate digest copy before returning.
/// - Returns `false` if `expected_hex` is not valid 32-byte hex.
pub fn verify_sha256_hex(token: &str, expected_hex: &str) -> bool {
    let mut expected = match hex::decode(expected_hex.trim()) {
        Ok(b) if b.len() == 32 => b,
        _ => return false,
    };

    let mut digest = [0u8; 32];
    digest.copy_from_slice(Sha256::digest(token.as_bytes()).as_slice());

    let equal = digest.ct_eq(expected.as_slice());

    digest.zeroize();
    expected.zeroize();

    equal.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_accepts_matching_token() {
        let token = "s3cr3t-random-token";
        let hash = sha256_hex(token.as_bytes());
        assert!(verify_sha256_hex(token, &hash));
    }

    #[test]
    fn verify_rejects_wrong_token() {
        let hash = sha256_hex(b"correct");
        assert!(!verify_sha256_hex("wrong", &hash));
    }

    #[test]
    fn verify_is_trim_and_case_tolerant_on_expected() {
        let token = "abc";
        let hash = sha256_hex(token.as_bytes());
        assert!(verify_sha256_hex(token, &format!("  {hash}  ")));
        assert!(verify_sha256_hex(token, &hash.to_uppercase()));
    }

    #[test]
    fn verify_rejects_malformed_expected() {
        assert!(!verify_sha256_hex("abc", "not-hex"));
        assert!(!verify_sha256_hex("abc", "dead")); // too short
    }
}
