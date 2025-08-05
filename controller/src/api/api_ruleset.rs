use crate::state::ControllerState;
use axum::{
    extract::{OriginalUri, Path, State},
    http::StatusCode,
    Json,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;
use shared::{rules_model::RuleSetRaw, };
use utoipa::{ToSchema};
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{ok, OkEnvelope};



/// Successful ruleset reply
#[derive(Serialize, ToSchema)]
pub struct RulesetOkData {
    pub ruleset: RuleSetRaw,
    version: String,
}
impl shared::api::ok::sealed::Sealed for RulesetOkData {}
impl shared::api::ok::OkData for RulesetOkData {}




/// Envelope to parse filter’s response for /ruleset.
#[derive(Deserialize)]
struct FilterResponseRulesetEnvelope {
    status: String,
    data: FilterResponseRulesetData,
}
#[derive(Deserialize)]
struct FilterResponseRulesetData {
    ruleset: RuleSetRaw,
    version: String,
}



#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/ruleset",
    params(
        ("ctrl_token" = String, description = "Authentication token"),
        ("service_id" = String, Path, description = "Service identifier"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<RulesetOkData>),
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
    // Validate service & port in controller state
    {
        let g = state.read();
        let svc = match g.services.get(&service_id) {
            Some(s) => s,
            None => {
                return ErrorEnvelope::not_found("sanitization", uri.path())
                    .with_title("Unknown service")
                    .with_detail(format!("service_id '{service_id}' is not known"))
                    .into_result()
            }
        };
        if !svc.ports.contains_key(&port) {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown port")
                .with_detail(format!("port {port} is not known for service '{service_id}'"))
                .into_result();
        }
    }

    if let Some((ruleset, version)) = state.read()
        .services
        .get(&service_id)
        .and_then(|s| s.ports.get(&port))
        .and_then(|p| p.ruleset.clone().zip(p.ruleset_version.clone()))
    {
        return ok(RulesetOkData { ruleset, version });
    }

    warn!("[get_ruleset(service_id={}, port={}), GET {}] cache miss, fetching ruleset from filter", service_id, port, uri.path());

    // Fetch from filter
    let (base, token) = {
        let g = state.read();
        let svc = g.services.get(&service_id).expect("validated above");
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/ruleset");
    let resp = Client::new()
        .get(&url)
        .query(&[("token", token.as_str())])
        .send()
        .await;

    // Network error to filter
    let Ok(resp) = resp else {
        return ErrorEnvelope::bad_gateway("filter_unreachable", uri.path())
            .with_title("Filter unreachable")
            .with_detail("controller could not reach the filter")
            .into_result();
    };

    if !resp.status().is_success() {
        return ErrorEnvelope::bad_gateway("filter_http_error", uri.path())
            .with_title("Filter returned error")
            .with_detail(format!("filter responded with {}", resp.status()))
            .into_result();
    }

    let env = match resp.json::<FilterResponseRulesetEnvelope>().await {
        Ok(v) => v,
        Err(e) => {
            return ErrorEnvelope::bad_gateway("bad_filter_payload", uri.path())
                .with_title("Invalid filter JSON")
                .with_detail(e.to_string())
                .into_result()
        }
    };

    if env.status != "ok" {
        return ErrorEnvelope::bad_gateway("filter_non_ok", uri.path())
            .with_title("Filter returned non-ok")
            .with_detail("ruleset endpoint signaled failure")
            .into_result();
    }

    // Cache & return
    // let rs = env.data.ruleset;
    // state.write()
    //     .services
    //     .get_mut(&service_id)
    //     .and_then(|s| s.ports.get_mut(&port))
    //     .map(|p| p.ruleset = Some(rs.clone()));
    // ok(RulesetOkData { ruleset: rs, version: env.data.version });



    let (rs, version) = (env.data.ruleset.clone(), env.data.version.clone());

    {
        let mut guard = state.write();
        if let Some(p) = guard
            .services
            .get_mut(&service_id)
            .and_then(|s| s.ports.get_mut(&port))
        {
            p.ruleset = Some(rs.clone());
            p.ruleset_version = Some(version.clone()); // todo maybe ??
        }
    }

    ok(RulesetOkData { ruleset: rs, version })
}





#[utoipa::path(
    get,
    path = "/services/{service_id}/ports/{port}/ruleset/cached",
    params(
        ("ctrl_token" = String, description = "Authentication token"),
        ("service_id" = String, Path, description = "Service identifier"),
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<RulesetOkData>),
        (status = 400, description = "Invalid request", body = ErrorEnvelope),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown service or port", body = ErrorEnvelope),
        (status = 500, description = "Server error", body = ErrorEnvelope),
        (status = 503, description = "Service Unavailable", body = ErrorEnvelope),
    ),
    tags = ["controller", "ruleset"]
)]
pub async fn get_ruleset_cached(
    OriginalUri(uri): OriginalUri,
    Path((service_id, port)): Path<(String, u16)>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    {
        let g = state.read();
        let svc = match g.services.get(&service_id) {
            Some(s) => s,
            None => {
                return ErrorEnvelope::not_found("sanitization", uri.path())
                    .with_title("Unknown service")
                    .with_detail(format!("service_id '{service_id}' is not known"))
                    .into_result()
            }
        };
        if !svc.ports.contains_key(&port) {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown port")
                .with_detail(format!("port {port} is not known for service '{service_id}'"))
                .into_result();
        }
    }

    if let Some((ruleset, version)) = state
        .read()
        .services
        .get(&service_id)
        .and_then(|s| s.ports.get(&port))
        .and_then(|p| p.ruleset.clone().zip(p.ruleset_version.clone()))
    {
        return ok(RulesetOkData { ruleset, version });
    }

    // Cache miss in cached version results in an error
    ErrorEnvelope::not_found("cache_miss", uri.path())
        .with_title("Ruleset not cached")
        .with_detail(format!(
            "Ruleset for service_id={service_id} and port={port} is not available in controller cache.\
             Call /services/{service_id}/ports/{port}/ruleset to fetch manually from the filter, or wait for the sync daemon to pull it from the filter.\
             If you keep getting this error, the filter's syncer may be inoperative."
        ))
        .into_result()
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
        (status = 200, description = "OK", body = OkEnvelope<RulesetOkData>),
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
    {
        let g = state.read();
        let svc = match g.services.get(&service_id) {
            Some(s) => s,
            None => {
                return ErrorEnvelope::not_found("sanitization", uri.path())
                    .with_title("Unknown service")
                    .with_detail(format!("service_id '{service_id}' is not known"))
                    .into_result()
            }
        };
        if !svc.ports.contains_key(&port) {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown port")
                .with_detail(format!("port {port} is not known for service '{service_id}'"))
                .into_result();
        }
    }

    let (base, token) = {
        let g = state.read();
        let svc = g.services.get(&service_id).expect("validated above");
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/ruleset");
    let resp = Client::new()
        .put(&url)
        .query(&[("token", token.as_str())])
        .json(&body)
        .send()
        .await;

    // Network error to filter
    let Ok(resp) = resp else {
        return ErrorEnvelope::bad_gateway("filter_unreachable", uri.path())
            .with_title("Filter unreachable")
            .with_detail("controller could not reach the filter")
            .into_result();
    };

    // Parse filter JSON whatever the HTTP status is, because the filter may
    // return structured errors with non-2xx and we want to surface details.
    let payload: FilterResponseRulesetEnvelope = match resp.json::<FilterResponseRulesetEnvelope>().await {
        Ok(v) => v,
        Err(e) => {
            return ErrorEnvelope::bad_gateway("bad_filter_payload", uri.path())
                .with_title("Invalid filter JSON")
                .with_detail(e.to_string())
                .into_result()
        }
    };

    // If filter says non-ok, return 400 with details from the filter payload.
    // if payload.get("status") != Some(&Value::String("ok".to_string())) {
    //     // Surface filter payload as detail to help debugging.
    //     return ErrorEnvelope::bad_request("filter_rejected_ruleset", uri.path())
    //         .with_title("Filter rejected ruleset")
    //         .with_detail(payload.to_string())
    //         .into_result();
    // }

    // On success, cache and normalize RulesetOkData.
    //state.write()
    //    .services
    //    .get_mut(&service_id)
    //    .and_then(|s| s.ports.get_mut(&port))
    //    .map(|p| p.ruleset = Some(body.clone()));
    //ok(RulesetOkData { ruleset: body })



    let (rs, version) = (payload.data.ruleset, payload.data.version);

    // 2. Lock & update in one go
    {
        let mut guard = state.write();
        if let Some(p) = guard
            .services
            .get_mut(&service_id)
            .and_then(|s| s.ports.get_mut(&port))
        {
            p.ruleset = Some(rs.clone());
            p.ruleset_version = Some(version.clone());
        }
    } // write‐lock drops here

    // 3. Return exactly what you just cached
    ok(RulesetOkData { ruleset: rs, version })
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
        (status = 200, description = "OK", body = OkEnvelope<RulesetOkData>),
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
    {
        let g = state.read();
        let svc = match g.services.get(&service_id) {
            Some(s) => s,
            None => {
                return ErrorEnvelope::not_found("sanitization", uri.path())
                    .with_title("Unknown service")
                    .with_detail(format!("service_id '{service_id}' is not known"))
                    .into_result()
            }
        };
        if !svc.ports.contains_key(&port) {
            return ErrorEnvelope::not_found("sanitization", uri.path())
                .with_title("Unknown port")
                .with_detail(format!("port {port} is not known for service '{service_id}'"))
                .into_result();
        }
    }

    let (base, token) = {
        let g = state.read();
        let svc = g.services.get(&service_id).expect("validated above");
        (svc.cfg.base_url.clone(), svc.cfg.token.clone())
    };

    let url = format!("{base}/ports/{port}/ruleset/validate");

    let resp = Client::new()
        .put(&url)
        .query(&[("token", token.as_str())])
        .json(&body)
        .send()
        .await;

    let Ok(resp) = resp else {
        return ErrorEnvelope::bad_gateway("filter_unreachable", uri.path())
            .with_title("Filter unreachable")
            .with_detail("controller could not reach the filter")
            .into_result();
    };

    let payload: RulesetValidationOkData = match resp.json::<RulesetValidationOkData>().await {
        Ok(v) => v,
        Err(e) => {
            return ErrorEnvelope::bad_gateway("bad_filter_payload", uri.path())
                .with_title("Invalid filter JSON")
                .with_detail(e.to_string())
                .into_result()
        }
    };

    ok(payload)
}