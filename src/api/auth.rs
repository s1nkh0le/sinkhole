//! Simple hash‑compare auth.
//! Every control‑API request must carry ?token=<secret>.
//!
//! If `AUTH_HASH` env is **unset** ➜ auth is **disabled** (allows CTF
//! quick‑start).

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use tracing::{debug, error, trace, warn};
use crate::config::Config;

static AUTH_HASH: Lazy<Option<String>> = Lazy::new(|| {
    std::env::var("TFILTER_AUTH_HASH")
        .ok()
        .map(|s| s.to_ascii_lowercase())
});

/// Verifies the token (SHA‑256 hex compare) against `AUTH_HASH`.
/// Returns `Ok(())` when:
///   * `AUTH_HASH` is not set
///      **OR**
///   * token is present **and** hashes to the same value.
pub fn check_token(token_opt: Option<&str>, config: &Config) -> Result<()> {
    let Some(expected_hex) = config.auth_hash.as_ref() else {
        // auth disabled
        warn!("AUTH_HASH is not set, auth disabled, allowing control action");
        return Ok(());
    };

    let token = token_opt.ok_or_else(|| anyhow!("missing token"))?;
    let calc_hex = {
        let mut h = Sha256::new();
        h.update(token.as_bytes());
        hex::encode(h.finalize())
    };

    if &calc_hex == expected_hex {
        trace!("auth ok");
        Ok(())
    } else {
        error!("auth failed: bad token, calculated_hex={} != {}=expected_hex, token={}", &calc_hex, expected_hex, token);
        Err(anyhow!("unauthorized"))
    }
}
