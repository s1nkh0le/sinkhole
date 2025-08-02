//! Simple hash‑compare auth.
//! Every control‑API request must carry ?token=<secret>.
//!
//! If `AUTH_HASH` env is **unset** ➜ auth is **disabled** (allows CTF
//! quick‑start).

use crate::config::Config;
use crate::observability::sinks::sink_ecam;
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use anyhow::{Result, anyhow};
use sha2::{Digest, Sha256};
use shared::utils::hashes::sha256_hex;
use tracing::{error, trace, warn};

/// Verifies the token (SHA‑256 hex compare) against `AUTH_HASH`.
/// Returns `Ok(())` when:
///   * `AUTH_HASH` is not set
///      **OR**
///   * token is present **and** hashes to the same value.
pub fn check_token(token_opt: Option<&str>, config: &Config) -> Result<()> {
    let Some(expected_hex) = config.auth_hash.as_ref() else {
        sink_ecam::record(
            Severity::Warn,
            Origin::Security,
            "AUTH_HASH is not set, auth disabled | allowing control action",
            None,
        );
        return Ok(());
    };

    let token = token_opt.ok_or_else(|| anyhow!("missing token"))?;
    let calc_hex = sha256_hex(token.as_ref());

    if &calc_hex == expected_hex {
        trace!("auth ok");
        Ok(())
    } else {
        sink_ecam::record(
            Severity::Error,
            Origin::Security,
            format!(
                "authentication failed: bad token | calculated_hex={} != {}=expected_hex, token={}",
                &calc_hex, expected_hex, token
            ),
            None,
        );

        Err(anyhow!("unauthorized"))
    }
}
