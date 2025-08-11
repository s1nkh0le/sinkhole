use crate::fetch::extraction::extract_ok_data;
use crate::fetch::error::CtrlErr;
use reqwest::Client;
use shared::types::routing::filter::state::FilterStateOkData;
use shared::types::snapshot_types::PortSnapshot;
use std::time::Duration;
use tracing::log::trace;

pub async fn fetch_state(
    http: &Client,
    base: &str,
    token: &str,
    timeout: Duration,
) -> Result<Vec<PortSnapshot>, CtrlErr> {
    let url = format!("{base}/state");
    let fut = http
        .get(&url)
        .bearer_auth(token)
        .send();

    let resp = tokio::time::timeout(timeout, fut).await.map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: serde_json::Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterStateOkData = extract_ok_data(v)?;

            trace!("[fetch_state(), GET {}] got reply {:?}", url, env);
            Ok(env.ports)
        }
        401 => Err(CtrlErr::Auth),
        code => Err(CtrlErr::Http(code)),
    }
}
