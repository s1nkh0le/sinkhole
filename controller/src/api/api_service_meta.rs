use crate::api::util::ensure_service_exists;
use crate::state::ctrl::{ControllerState, ServiceMetadata};
use axum::Json;
use axum::extract::{OriginalUri, Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use shared::api::err::ErrorEnvelope;
use shared::api::ok::{OkEnvelope, ok};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CtrlServiceEditRequest {
    /// If present, set name; empty string clears it.
    pub name: Option<String>,
    /// If present, set description; empty string clears it.
    pub description: Option<String>,
    /// If present, set color; empty or invalid clears it.
    pub hex_color: Option<String>,
    /// If present, set icon; empty string clears it.
    pub icon: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CtrlServiceEditOkData {
    pub service_id: String,
    pub meta: ServiceMetadata,
}
impl shared::api::ok::sealed::Sealed for CtrlServiceEditOkData {}
impl shared::api::ok::OkData for CtrlServiceEditOkData {}

/// /services/{service_id}/meta
///
/// Read service metadata
#[utoipa::path(
    get,
    path = "/services/{service_id}/meta",
    params(
        ("service_id" = String, Path,                                       description = "Service id"),
    ),
    responses(
        (status = 200, description = "OK",                                  body = OkEnvelope<CtrlServiceEditOkData>),
        (status = 401, description = "Unauthorized",                        body = ErrorEnvelope),
        (status = 404, description = "Unknown service",                     body = ErrorEnvelope),
    ),
    tags = ["controller", "services"]
)]
pub async fn get_service_meta(
    OriginalUri(uri): OriginalUri,
    Path(service_id): Path<String>,
    State(state): State<ControllerState>,
) -> (StatusCode, Json<Value>) {
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    let meta = {
        let g = state.read();
        // Re-check under the lock: a concurrent delete could have removed the
        // service between the existence check above and here.
        let Some(svc) = g.services.get(&service_id) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        svc.metadata.clone()
    };

    ok(CtrlServiceEditOkData { service_id, meta })
}

/// /services/{service_id}/meta
///
/// Edit service metadata
#[utoipa::path(
    put,
    path = "/services/{service_id}/meta",
    params(
        ("service_id" = String, Path,                                       description = "Service id"),
    ),
    responses(
        (status = 200, description = "OK",                                  body = OkEnvelope<CtrlServiceEditOkData>),
        (status = 401, description = "Unauthorized",                        body = ErrorEnvelope),
        (status = 404, description = "Unknown service",                     body = ErrorEnvelope),
        (status = 400, description = "Invalid metadata",                    body = ErrorEnvelope),
    ),
    tags = ["controller", "services"]
)]
pub async fn put_service_meta(
    OriginalUri(uri): OriginalUri,
    Path(service_id): Path<String>,
    State(state): State<ControllerState>,
    Json(req): Json<CtrlServiceEditRequest>,
) -> (StatusCode, Json<Value>) {
    // Must exist
    if !ensure_service_exists(&state, &service_id) {
        return ErrorEnvelope::not_found("service_not_found", uri.path())
            .with_title("Unknown service")
            .with_detail(format!("service_id '{service_id}' is not known"))
            .into_result();
    }

    // Normalize intent: None=unchanged; Some("")=clear; Some("x")=set
    let name_norm = normalize_opt_string(req.name);
    let desc_norm = normalize_opt_string(req.description);
    let hex_color_norm = normalize_opt_string(req.hex_color);
    let icon_norm = normalize_opt_string(req.icon);

    // Validate hex color if set
    if let Some(Some(ref c)) = hex_color_norm
        && !validate_hex_color(c)
    {
        return ErrorEnvelope::bad_request("invalid_hex_color", uri.path())
            .with_title("Invalid hex color")
            .with_detail("expected #RRGGBB or #RRGGBBAA")
            .into_result();
    }

    // Apply patch
    let meta = {
        let mut w = state.write();
        // Re-check under the lock: a concurrent delete could have removed the
        // service between the existence check above and here.
        let Some(svc) = w.services.get_mut(&service_id) else {
            return ErrorEnvelope::not_found("service_not_found", uri.path())
                .with_title("Unknown service")
                .with_detail(format!("service_id '{service_id}' is not known"))
                .into_result();
        };
        let m = &mut svc.metadata;

        if let Some(v) = name_norm.clone() {
            m.name = v;
        }
        if let Some(v) = desc_norm.clone() {
            m.description = v;
        }
        if let Some(v) = hex_color_norm.clone() {
            m.hex_color = v;
        }
        if let Some(v) = icon_norm.clone() {
            m.icon = v;
        }

        m.clone()
    };

    // Persist so it survives controller restarts (only when the fleet is
    // hydrated from the DB; otherwise the services table is dormant).
    if state.cfg().db_hydrate {
        crate::db::insertion::persist_service_meta(&state.db(), &service_id, &meta).await;
    }

    ok(CtrlServiceEditOkData { service_id, meta })
}

fn normalize_opt_string(v: Option<String>) -> Option<Option<String>> {
    // None => leave unchanged; Some("") => clear; Some("x") => set
    v.map(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

fn validate_hex_color(s: &str) -> bool {
    let bytes = s.as_bytes();
    if !(bytes.len() == 7 || bytes.len() == 9) {
        return false;
    }
    if bytes[0] != b'#' {
        return false;
    }
    bytes[1..].iter().all(|b| (*b as char).is_ascii_hexdigit())
}
