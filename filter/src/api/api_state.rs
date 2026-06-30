use crate::api::routes_filter::SharedState;
use axum::Json;
use axum::extract::{OriginalUri, State};
use axum::http::StatusCode;
use shared::api::ok::{OkEnvelope, ok};
use shared::types::routing::filter::state::FilterStateOkData;

/// /fetch
///
/// Returns a full fetch dump of the filter, including all port info, rulesets and versions
#[utoipa::path(
    get,
    path = "/fetch",
    params(
        ("port" = u16, Path, description = "Public port"),
    ),
    responses(
        (status = 200, description = "OK", body = OkEnvelope<FilterStateOkData>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Unknown port"),
    ),
    tag = "filter"
)]
pub async fn get_state(
    OriginalUri(_uri): OriginalUri,
    State((_cfg, registry, _db)): State<SharedState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut snapshots = Vec::new();

    for entry in registry.iter() {
        let _port = *entry.key();
        let pr = entry.value();

        let port_snapshot = pr.to_snapshot();
        snapshots.push(port_snapshot);
    }

    // sort ports for nicer diff-ability
    snapshots.sort_by_key(|p| p.info.public);

    let payload = FilterStateOkData { ports: snapshots };
    ok(payload)
}
