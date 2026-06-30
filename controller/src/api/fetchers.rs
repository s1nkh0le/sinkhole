use crate::fetch::error::CtrlErr;
use crate::fetch::extraction::extract_ok_data;
use reqwest::Client;
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::types::routing::filter::ports::{
    FilterReplySuccessPortAndVersion, FilterRequestPutBodyConfigUpdate,
};
use shared::types::routing::filter::ruleset::{
    FilterReplyRulesetHistory, FilterReplySuccessRuleset, FilterReplySuccessValidationRuleset,
    RulesetHistoryEntry,
};
use shared::types::ruleset_types::{PortInfo, RuleSetRaw};
use std::time::Duration;

/// Turn a non-2xx filter response into a `CtrlErr`, preserving the filter's own
/// structured `ErrorEnvelope` when present so the controller can propagate the
/// real status and detail (instead of flattening everything to `502`).
async fn err_from_filter_response(resp: reqwest::Response) -> CtrlErr {
    let code = resp.status().as_u16();
    if code == 401 {
        return CtrlErr::Auth;
    }
    // Prefer the filter's structured error body; fall back to a bare status code
    // if it isn't a parseable `ErrorEnvelope`.
    match resp.json::<ErrorEnvelope>().await {
        Ok(env) => CtrlErr::CustomEnvelope(env),
        Err(e) => {
            tracing::trace!("could not deserialize filter ErrorEnvelope (HTTP {code}): {e}");
            CtrlErr::Http(code)
        }
    }
}

pub async fn put_port_config_http(
    http: &Client,
    base: &str,
    token: &str,
    port: u16,
    body: &FilterRequestPutBodyConfigUpdate,
) -> Result<(PortInfo, String), CtrlErr> {
    let url = format!("{base}/ports/{port}/config");

    let fut = http.put(&url).bearer_auth(token).json(body).send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplySuccessPortAndVersion = extract_ok_data(v)?;
            Ok((env.info, env.ver_info))
        }
        _ => Err(err_from_filter_response(resp).await),
    }
}

pub async fn put_ruleset_filter(
    http: &Client,
    base: &str,
    token: &str,
    port: u16,
    body: &RuleSetRaw,
) -> Result<(RuleSetRaw, String), CtrlErr> {
    let url = format!("{base}/ports/{port}/ruleset");

    let fut = http.put(&url).bearer_auth(token).json(body).send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplySuccessRuleset = extract_ok_data(v)?;
            Ok((env.ruleset, env.ver_ruleset))
        }
        _ => Err(err_from_filter_response(resp).await),
    }
}

pub async fn get_ruleset_history_filter(
    http: &Client,
    base: &str,
    token: &str,
    port: u16,
) -> Result<Vec<RulesetHistoryEntry>, CtrlErr> {
    let url = format!("{base}/ports/{port}/ruleset/history");

    let fut = http.get(&url).bearer_auth(token).send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplyRulesetHistory = extract_ok_data(v)?;
            Ok(env.entries)
        }
        _ => Err(err_from_filter_response(resp).await),
    }
}

pub async fn validate_ruleset_filter(
    http: &Client,
    base: &str,
    token: &str,
    port: u16,
    body: &RuleSetRaw,
) -> Result<(bool, RuleSetRaw, Option<String>), CtrlErr> {
    let url = format!("{base}/ports/{port}/ruleset/validate");

    let fut = http.put(&url).bearer_auth(token).json(body).send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplySuccessValidationRuleset = extract_ok_data(v)?;
            Ok((env.validated, env.ruleset, env.validation_errors))
        }
        _ => Err(err_from_filter_response(resp).await),
    }
}
