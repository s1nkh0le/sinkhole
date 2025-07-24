use crate::utils::sha256_hex;
use anyhow::{anyhow, Result};

/// Pass the plaintext token (usually extracted from request JSON) and the optional AUTH_HASH.
/// If AUTH_HASH is None we accept everything (dev mode).
pub fn check_token(token_opt: Option<&str>, auth_hash: &Option<String>) -> Result<()> {
    if let Some(expected) = auth_hash {
        let token = token_opt.ok_or_else(|| anyhow!("missing token"))?;
        let hashed = sha256_hex(token.as_bytes());
        if &hashed != expected {
            return Err(anyhow!("unauthorized"));
        }
    }
    Ok(())
}
