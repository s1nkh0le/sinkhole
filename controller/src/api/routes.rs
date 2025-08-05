use crate::api::api_cached::{get_cached_port_ruleset};
use crate::api::api_ecam::{get_aggregated_ecam, get_service_ecam};
use crate::api::api_ports::{get_port_config, get_port_config_cached, get_list_ports, put_port_config};
use crate::api::api_records::{get_aggregated_records, get_service_port_logs};
use crate::api::api_ruleset::{get_ruleset, get_ruleset_cached, put_ruleset, put_validate_ruleset};
use crate::api::api_state::get_state;
use crate::api::middleware_auth::require_ctrl_token;
use crate::api::api_health::{get_health, get_list_services};
use crate::state::ControllerState;
use axum::{middleware, routing::get, Router};
use axum::routing::put;
use utoipa::{OpenApi};
use utoipa_swagger_ui::SwaggerUi;

// paths generated methods
use crate::api::api_health::__path_get_health;
use crate::api::api_health::__path_get_list_services;
use crate::api::api_state::__path_get_state;

use crate::api::api_ecam::__path_get_aggregated_ecam;
use crate::api::api_ecam::__path_get_service_ecam;
use crate::api::api_records::__path_get_aggregated_records;
use crate::api::api_records::__path_get_service_port_logs;


#[derive(OpenApi)]
#[openapi(
    paths(
        get_health,
        get_list_services,
        get_state,

        get_aggregated_ecam,
        get_service_ecam,
        get_aggregated_records,
        get_service_port_logs
    ),
    components(
        schemas(
            shared::sink_model::EcamEvent,
            shared::eval_record::EvaluationRecord,
        )
    ),
    tags(
        (name = "controller", description = "Sinkhole Controller API")
    )
)]
struct ApiDoc;

pub fn build_router(state: ControllerState) -> Router {
    let cfg_for_mw = state.cfg();

    let protected = Router::new()
        // state
        .route("/auth/ping", get(get_health))
        .route("/services", get(get_list_services))
        .route("/state", get(get_state))

        // ecam
        .route("/ecam", get(get_aggregated_ecam))
        .route("/services/{svc}/ecam", get(get_service_ecam))
        
        // records
        .route("/records", get(get_aggregated_records))
        .route("/services/{svc}/ports/{port}/records", get(get_service_port_logs))
        

        // svc
        .route("/services/{svc}/ports", get(get_list_ports))
        .route("/services/{svc}/ports/{port}/ruleset", get(get_ruleset).put(put_ruleset))
        .route("/services/{svc}/ports/{port}/ruleset/validate", put(put_validate_ruleset))
        .route("/services/{svc}/ports/{port}/ruleset/cached", get(get_cached_port_ruleset))
        .route("/services/{svc}/ports/{port}/config", get(get_port_config).put(put_port_config))
        .route("/services/{svc}/ports/{port}/config/cached", get(get_ruleset_cached))
        .layer(middleware::from_fn({
            let cfg_for_mw = cfg_for_mw.clone();
            move |req, next| {
                let cfg_for_mw = cfg_for_mw.clone();
                async move { require_ctrl_token(cfg_for_mw, req, next).await }
            }
        }));

    Router::new()
        .route("/health", get(get_health))
        .merge(protected)
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .with_state(state)
}
