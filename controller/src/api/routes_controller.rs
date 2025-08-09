use crate::api::api_ecam::{get_aggregated_ecam, get_service_ecam};
use crate::api::api_ports::{get_port_config, get_list_ports, put_port_config};
use crate::api::api_records::{get_aggregated_records, get_service_port_records};
use crate::api::api_ruleset::{get_ruleset, put_ruleset, put_validate_ruleset};
use crate::api::api_state::get_state;
use crate::api::api_service::get_service_meta;
use crate::api::middleware_auth::require_ctrl_token;
use crate::api::api_health::{get_health, get_list_services};
use crate::state::ctrl::ControllerState;
use axum::{middleware, routing::get, Router};
use axum::routing::put;
use utoipa::{OpenApi};
use utoipa_swagger_ui::SwaggerUi;

// paths generated methods
/// global fetch
use crate::api::api_health::__path_get_health;
use crate::api::api_health::__path_get_list_services;
use crate::api::api_state::__path_get_state;

/// ecam
use crate::api::api_ecam::__path_get_aggregated_ecam;
use crate::api::api_ecam::__path_get_service_ecam;

/// records
use crate::api::api_records::__path_get_aggregated_records;
use crate::api::api_records::__path_get_service_port_records;

/// service
use crate::api::api_service::__path_get_service_meta;
use crate::api::api_service::__path_put_service_meta;

/// ports config
use crate::api::api_ports::__path_get_port_config;
use crate::api::api_ports::__path_put_port_config;
use crate::api::api_ports::__path_get_list_ports;

/// rulesets
use crate::api::api_ruleset::__path_get_ruleset;
use crate::api::api_ruleset::__path_put_ruleset;
use crate::api::api_ruleset::__path_put_validate_ruleset;
use crate::api::api_service::put_service_meta;

#[derive(OpenApi)]
#[openapi(
    paths(
        // global fetch
        get_health,
        get_list_services,
        get_state,

        // ecam
        get_aggregated_ecam,
        get_service_ecam,

        // records
        get_aggregated_records,
        get_service_port_records,

        // records
        get_service_meta,
        put_service_meta,

        // ports config
        get_list_ports,
        get_port_config, put_port_config,

        // rulesets
        get_ruleset, put_ruleset,
        put_validate_ruleset,
    ),
    tags(
        (name = "controller", description = "Sinkhole Controller API"),
        (name = "ecam", description = "ECAM events from filter"),
        (name = "records", description = "Evaluation records from filter"),
        (name = "ports", description = "Port management for controller and filters"),
        (name = "ruleset", description = "Ruleset management for controller and filters"),
        (name = "unauthenticated", description = "API endpoints exposed to the public withouth requiring a token"),
    )
)]
struct ApiDoc;

pub fn build_router(state: ControllerState) -> Router {
    let cfg_for_mw = state.cfg();

    let protected = Router::new()
        // global fetch
        .route("/auth/ping", get(get_health))
        .route("/services", get(get_list_services))
        .route("/state", get(get_state))

        // ecam
        .route("/ecam", get(get_aggregated_ecam))
        .route("/services/{svc}/ecam", get(get_service_ecam))

        // records
        .route("/records", get(get_aggregated_records))
        .route("/services/{svc}/ports/{port}/records", get(get_service_port_records))

        // service
        .route("/services/{svc}/meta", get(get_service_meta).put(put_service_meta))
        
        // ports config
        .route("/services/{svc}/ports", get(get_list_ports))
        .route("/services/{svc}/ports/{port}/config", get(get_port_config).put(put_port_config))

        // rulesets
        .route("/services/{svc}/ports/{port}/ruleset", get(get_ruleset).put(put_ruleset))
        .route("/services/{svc}/ports/{port}/ruleset/validate", put(put_validate_ruleset))

        // auth
        .layer(middleware::from_fn({
            let cfg_for_mw = cfg_for_mw.clone();
            move |req, next| {
                let cfg_for_mw = cfg_for_mw.clone();
                async move { require_ctrl_token(cfg_for_mw, req, next).await }
            }
        }));

    let openapi = SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi());

    Router::new()
        .route("/health", get(get_health))
        .merge(protected)
        .merge(openapi)
        .with_state(state)
}
