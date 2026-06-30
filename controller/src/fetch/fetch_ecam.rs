use crate::fetch::error::CtrlErr;
use crate::fetch::extraction::extract_ok_data;
use reqwest::Client;
use shared::types::ecam_types::EcamEvent;
use shared::types::routing::filter::ecam::FilterReplyEcamAndCursor;
use std::time::Duration;
use tracing::log::trace;

/// GET /ecam
pub async fn fetch_ecam(
    http: &Client,
    base: &str,
    token: &str,
    since: u64,
) -> Result<(Vec<EcamEvent>, u64), CtrlErr> {
    let url = format!("{base}/ecam");

    let fut = http
        .get(&url)
        .bearer_auth(token)
        .query(&[("since", &since.to_string())])
        .send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: serde_json::Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplyEcamAndCursor = extract_ok_data(v)?;

            trace!("[fetch_ecam(), GET {}] got reply {:?}", url, env);
            Ok((env.events, env.next_since))
        }
        401 => Err(CtrlErr::Auth),
        c => Err(CtrlErr::Http(c)),
    }
}
