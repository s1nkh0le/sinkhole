// controller/src/fetchers.rs (append)

use std::time::Duration;
use reqwest::Client;
use tracing::log::trace;
use shared::types::{
    ecam_types::EcamEvent,
    record_types::EvaluationRecord,
};
use shared::types::routing::filter::ecam::FilterReplyEcamAndCursor;
use shared::types::routing::filter::records::FilterReplyRecordsAndCursor;
use crate::fetch::extraction::extract_ok_data;
use crate::fetch::error::CtrlErr;

const REC_LIMIT: usize = 5000;

/// GET /ports/{port}/records
pub async fn fetch_records(
    http:   &Client,
    base:   &str,
    port:   u16,
    token:  &str,
    since:  u64,
) -> Result<(Vec<EvaluationRecord>, u64), CtrlErr> {
    let url = format!("{base}/ports/{port}/records");

    let fut = http
        .get(&url)
        .bearer_auth(token)
        .query(&[
            ("since", &since.to_string()),
            ("limit", &REC_LIMIT.to_string()),
            ("order", &"asc".to_string()),
        ])
        .send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: serde_json::Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplyRecordsAndCursor = extract_ok_data(v)?;
            
            trace!("[fetch_records(), GET {}] got reply {:?}", url, env);
            Ok((env.records, env.next_since))
        }
        401 => Err(CtrlErr::Auth),
        c   => Err(CtrlErr::Http(c)),
    }
}
