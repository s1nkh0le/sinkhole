use crate::api::fetchers::{put_ruleset_filter, validate_ruleset_filter};
use crate::api::util::{ensure_port_exists, ensure_service_exists, ensure_snapshots};
use crate::state::ControllerState;
use axum::{
    extract::{OriginalUri, Path, State},
    http::StatusCode,
    Json,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkEnvelope};
use shared::types::ruleset_types::RuleSetRaw;
use shared::utils::version::calc_version_combined_copyover;
use utoipa::ToSchema;

/// Successful ruleset reply
#[derive(Serialize, ToSchema)]
pub struct CtrlReplyRulesetOkData {
    pub ruleset: RuleSetRaw,
    pub version: String,
}
impl shared::api::ok::sealed::Sealed for CtrlReplyRulesetOkData {}
impl shared::api::ok::OkData for CtrlReplyRulesetOkData {}


#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/ruleset",
    params(
        ("ctrl_token" = String, description = "Authentication token"),
        ("service_id" = String, Path, description = "Service identifier"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<CtrlReplyRulesetOkData>),
        (status = 400, description = "Invalid request", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service or port", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned error", body = ErrorEnvelope)
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn get_ruleset(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service id ──────────────────────────────────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // ── 2) Ensure snapshots cached (one-shot fetch if needed) ───────────
    if let Err(e) = ensure_snapshots(&state, &service_id, &Client::new()).await {
        return ErrorEnvelope::service_unavailable("state_unavailable", uri.path())
            .with_title("Ruleset not cached")
            .with_detail(e.to_string())
            .into_result();
    }

    // ── 3) Validate port existence after caching attempt ────────────────
    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 4) Extract ruleset + version from cached snapshot ───────────────
    let (ruleset, version) = {
        let g = state.read();
        let snap = g.services
            .get(&service_id)
            .and_then(|svc| svc.ports.get(&port))
            .and_then(|p| p.snap.as_ref())
            .expect("port validated");

        (snap.ruleset.clone(), snap.ver_ruleset.clone())
    };

    ok(CtrlReplyRulesetOkData { ruleset, version })
}





#[derive(Serialize, Deserialize, ToSchema)]
pub struct RulesetValidationOkData {
    pub validated: bool,
    pub validated_version: Option<String>
}
impl shared::api::ok::sealed::Sealed for RulesetValidationOkData {}
impl shared::api::ok::OkData for RulesetValidationOkData {}


#[utoipa::path(
    put,
    path = "/services/{service_id}/ports/{port}/ruleset",
    params(
        ("ctrl_token" = String, description = "Authentication token"),
        ("service_id" = String, Path, description = "Service identifier"),
        ("port" = u16, Path, description = "Public port"),
    ),
    request_body = RuleSetRaw,
    responses(
        (status = 200, description = "OK", body = OkEnvelope<CtrlReplyRulesetOkData>),
        (status = 400, description = "Filter rejected ruleset", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service or port", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned error", body = ErrorEnvelope)
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn put_ruleset(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<RuleSetRaw>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service & snapshots ─────────────────────────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    if let Err(e) = ensure_snapshots(&state, &service_id, &Client::new()).await {
        return ErrorEnvelope::service_unavailable("state_unavailable", uri.path())
            .with_title("Ruleset not cached")
            .with_detail(e.to_string())
            .into_result();
    }

    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 2) Snapshot creds ------------------------------------------------
    let (base_url, token) = {
        let g = state.read();
        let svc = g.services.get(&service_id).expect("validated");
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    // ── 3) Relay PUT to filter ------------------------------------------
    let (ruleset, ver_ruleset) = match put_ruleset_filter(&Client::new(), &base_url, &token, port, &body, ).await {
        Ok(t) => t,
        Err(err) => {
            return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                .with_title("Filter unreachable")
                .with_detail(err.to_string())
                .into_result();
        }
    };

    // ── 4) Update cached snapshot ---------------------------------------
    {
        let mut w = state.write();
        if let Some(p) = w.services
            .get_mut(&service_id)
            .and_then(|svc| svc.ports.get_mut(&port))
        {
            if let Some(mut snap) = p.snap.take() {
                snap.ruleset      = ruleset.clone();
                snap.ver_ruleset  = ver_ruleset.clone();
                snap.ver_combined = calc_version_combined_copyover(&snap.ver_info, &ver_ruleset);
                p.snap = Some(snap);
            }
        }
    }

    // ── 5) Reply to client ----------------------------------------------
    ok(CtrlReplyRulesetOkData {
        ruleset,
        version: ver_ruleset,
    })
}


#[utoipa::path(
    put,
    path = "/services/{service_id}/ports/{port}/ruleset/validate",
    params(
        ("ctrl_token" = String, description = "Authentication token"),
        ("service_id" = String, Path, description = "Service identifier"),
        ("port" = u16, Path, description = "Public port"),
    ),
    request_body = RuleSetRaw,
    responses(
        (status = 200, description = "OK", body = OkEnvelope<CtrlReplyRulesetOkData>),
        (status = 400, description = "Filter rejected ruleset", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service or port", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
        (status = 502, description = "Filter unreachable or returned error", body = ErrorEnvelope)
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn put_validate_ruleset(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
    Json(body): Json<RuleSetRaw>,
) -> (StatusCode, Json<Value>) {
    // ── 1) Validate service/port (snapshots optional) ───────────────────
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }
    if !ensure_port_exists(&state, &service_id, port) {
        return ErrorEnvelope::not_found("port_not_found", uri.path())
            .with_title("Unknown port")
            .with_detail(format!(
                "port '{port}' is not known for service '{service_id}'"
            ))
            .into_result();
    }

    // ── 2) Snapshot creds ----------------------------------------------
    let (base_url, token) = {
        let g = state.read();
        let svc = g.services.get(&service_id).expect("validated");
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    // ── 3) Relay validate call to filter --------------------------------
    let (validated, _ruleset, maybe_err) = match validate_ruleset_filter(&Client::new(), &base_url, &token, port, &body, ).await {
        Ok(t) => t,
        Err(err) => {
            return ErrorEnvelope::bad_gateway("filter_response_error", uri.path())
                .with_title("Filter unreachable")
                .with_detail(err.to_string())
                .into_result();
        }
    };

    if !validated {
        return ErrorEnvelope::bad_request("ruleset_validation_failed", uri.path())
            .with_title("Filter rejected ruleset")
            .with_detail(maybe_err.unwrap_or_else(|| "validation failed".into()))
            .into_result();
    }

    // ── 4) Success: echo back what we received (ruleset + version hash) ─
    // Version hash is meaningful only after install; we return placeholder.
    ok(CtrlReplyRulesetOkData {
        ruleset: body,
        version: "<not-installed>".into(),
    })
}