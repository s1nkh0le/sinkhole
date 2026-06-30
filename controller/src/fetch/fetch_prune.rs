use crate::fetch::error::CtrlErr;
use reqwest::Client;
use std::time::Duration;
use tracing::log::trace;

/// DELETE /db/prune/records
///
/// Asks the filter to drop records older than `prune_threshold_ts` (ns). The
/// filter has already had these records pulled into the central DB, so this keeps
/// the filter's local DB bounded. Returns Ok(()) on success.
pub async fn prune_filter_records(
    http: &Client,
    base: &str,
    token: &str,
    prune_threshold_ts: u64,
) -> Result<(), CtrlErr> {
    let url = format!("{base}/db/prune/records");

    let fut = http
        .delete(&url)
        .bearer_auth(token)
        .query(&[("prune_threshold_ts", &prune_threshold_ts.to_string())])
        .send();

    let resp = tokio::time::timeout(Duration::from_secs(10), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            trace!(
                "[prune_filter_records(), DELETE {}] ok (< {})",
                url, prune_threshold_ts
            );
            Ok(())
        }
        401 => Err(CtrlErr::Auth),
        c => Err(CtrlErr::Http(c)),
    }
}
