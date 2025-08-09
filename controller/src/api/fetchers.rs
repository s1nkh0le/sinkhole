use std::time::Duration;
use reqwest::Client;
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::types::routing::filter::ports::{FilterCtrlRequestPutBodyConfigUpdate, FilterReplySuccessPortAndVersion};
use shared::types::routing::filter::ruleset::{FilterReplySuccessRuleset, FilterReplySuccessValidationRuleset};
use shared::types::ruleset_types::{PortInfo, RuleSetRaw};
use crate::fetch::error::CtrlErr;
use crate::fetch::extraction::extract_ok_data;




pub async fn put_port_config_http(
    http:   &Client,
    base:   &str,
    token:  &str,
    port:   u16,
    body:   &FilterCtrlRequestPutBodyConfigUpdate,
) -> Result<(PortInfo, String), CtrlErr> {
    let url = format!("{base}/ports/{port}/config");

    let fut = http
        .put(&url)
        .query(&[("token", token)])
        .json(body)
        .send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplySuccessPortAndVersion = extract_ok_data(v)?;
            Ok((env.info, env.ver_info))
        }
        401 => Err(CtrlErr::Auth),
        c   => Err(CtrlErr::Http(c)),
    }
}



pub async fn put_ruleset_filter(
    http:   &reqwest::Client,
    base:   &str,
    token:  &str,
    port:   u16,
    body:   &RuleSetRaw,
) -> Result<(RuleSetRaw, String), CtrlErr> {
    let url = format!("{base}/ports/{port}/ruleset");

    let fut = http
        .put(&url)
        .query(&[("token", token)])
        .json(body)
        .send();

    let resp = tokio::time::timeout(Duration::from_secs(5555), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: Value = resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplySuccessRuleset = extract_ok_data(v)?;
            Ok((env.ruleset, env.ver_ruleset))
        }
        400 => {
            let err: ErrorEnvelope = resp
                .json()
                .await
                .map_err(|e| {
                    tracing::trace!("Could not deserialize error=: {}", e);

                    CtrlErr::Json(e.into())
                })?;

            Err(CtrlErr::CustomEnvelope(err))
        }
        401 => Err(CtrlErr::Auth),
        c   => Err(CtrlErr::Http(c)),
    }
}


pub async fn validate_ruleset_filter(
    http:   &reqwest::Client,
    base:   &str,
    token:  &str,
    port:   u16,
    body:   &RuleSetRaw,
) -> Result<(bool, RuleSetRaw, Option<String>), CtrlErr> {
    let url = format!("{base}/ports/{port}/ruleset/validate");

    let fut = http
        .put(&url)
        .query(&[("token", token)])
        .json(body)
        .send();

    let resp = tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .map_err(|_| CtrlErr::Timeout)??;

    match resp.status().as_u16() {
        200 => {
            let v: serde_json::Value =
                resp.json().await.map_err(|e| CtrlErr::Json(e.into()))?;
            let env: FilterReplySuccessValidationRuleset = extract_ok_data(v)?;
            Ok((env.validated, env.ruleset, env.validation_errors))
        }
        400 => {
            let err: ErrorEnvelope = resp
                .json()
                .await
                .map_err(|e| {
                    tracing::trace!("Could not deserialize error=: {}", e);

                    CtrlErr::Json(e.into())
                })?;

            Err(CtrlErr::CustomEnvelope(err))
        }
        401 => Err(CtrlErr::Auth),
        c   => Err(CtrlErr::Http(c)),
    }
}