//! Simple hash‑compare auth.
//! Every control‑API request must carry ?token=<secret>.
//!
//! If `AUTH_HASH` env is **unset** ➜ auth is **disabled** (allows CTF
//! quick‑start).

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

static AUTH_HASH: Lazy<Option<String>> = Lazy::new(|| {
    std::env::var("AUTH_HASH")
        .ok()
        .map(|s| s.to_ascii_lowercase())
});

/// Verifies the token (SHA‑256 hex compare) against `AUTH_HASH`.
/// Returns `Ok(())` when:
///   * `AUTH_HASH` is not set
///      **OR**
///   * token is present **and** hashes to the same value.
pub fn check_token(token_opt: Option<&str>) -> Result<()> {
    let Some(expected_hex) = AUTH_HASH.as_ref() else {
        // auth disabled
        debug!("AUTH_HASH not set -> auth disabled");
        return Ok(());
    };

    let token = token_opt.ok_or_else(|| anyhow!("missing token"))?;
    let calc_hex = {
        let mut h = Sha256::new();
        h.update(token.as_bytes());
        hex::encode(h.finalize())
    };

    if &calc_hex == expected_hex {
        debug!("auth ok");
        Ok(())
    } else {
        warn!("auth failed: bad token");
        Err(anyhow!("unauthorized"))
    }
}
