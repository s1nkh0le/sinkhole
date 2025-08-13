use crate::api::api_ecam::{get_aggregated_ecam, get_service_ecam};
use crate::api::api_health::{get_health, get_list_services};
use crate::api::api_ports::{get_list_ports, get_port_config, put_port_config};
use crate::api::api_records::{get_aggregated_records, get_service_port_records};
use crate::api::api_ruleset::{get_ruleset, put_ruleset, put_validate_ruleset};
use crate::api::api_service::{delete_service_supervisor, put_service_supervisor_new};
use crate::api::api_service_meta::get_service_meta;
use crate::api::api_service_meta::put_service_meta;
use crate::api::api_state::get_state;
use crate::api::middleware_auth::require_ctrl_token;
use crate::api::api_records_grouped::get_grouped_records;
use axum::routing::{delete, put};
use axum::{middleware, routing::get, Router};
use utoipa::{Modify, OpenApi};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::SecurityRequirement;
use utoipa_swagger_ui::SwaggerUi;


/// global
use crate::api::api_health::__path_get_health;
use crate::api::api_health::__path_get_list_services;
use crate::api::api_state::__path_get_state;


/// ecam
use crate::api::api_ecam::__path_get_aggregated_ecam;
use crate::api::api_ecam::__path_get_service_ecam;


/// records
use crate::api::api_records_grouped::__path_get_grouped_records;
use crate::api::api_records::__path_get_aggregated_records;
use crate::api::api_records::__path_get_service_port_records;


/// service
use crate::api::api_service::__path_put_service_supervisor_new;
use crate::api::api_service::__path_delete_service_supervisor;
use crate::api::api_service_meta::__path_get_service_meta;
use crate::api::api_service_meta::__path_put_service_meta;


/// ports config
use crate::api::api_ports::__path_get_port_config;
use crate::api::api_ports::__path_get_list_ports;
use crate::api::api_ports::__path_put_port_config;

/// rulesets
use crate::api::api_ruleset::__path_get_ruleset;
use crate::api::api_ruleset::__path_put_ruleset;
use crate::api::api_ruleset::__path_put_validate_ruleset;
use crate::supervisor::app_state::AppState;


#[derive(OpenApi)]
#[openapi(
    modifiers(&BearerAddon),
    paths(
        // global
        get_health,
        get_list_services,
        get_state,

        // ecam
        get_aggregated_ecam,
        get_service_ecam,

        // records
        get_aggregated_records,
        get_service_port_records,
        get_grouped_records,

        // service
        put_service_supervisor_new, delete_service_supervisor,
        get_service_meta, put_service_meta,

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
struct BearerAddon;

impl Modify for BearerAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        const BEARER: &'static str = "bearerAuth";
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            BEARER,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("Token")
                    .build(),
            ),
        );

        openapi
            .security
            .get_or_insert_with(Vec::new)
            .push(SecurityRequirement::new(BEARER, Vec::<String>::new()));
    }
}


pub fn build_router(state: AppState) -> Router {
    let cfg_for_mw = state.ctrl.cfg();

    let protected = Router::new()
        // global
        .route("/auth/ping", get(get_health))
        .route("/services", get(get_list_services))
        .route("/state", get(get_state))

        // ecam
        .route("/ecam", get(get_aggregated_ecam))
        .route("/services/{svc}/ecam", get(get_service_ecam))

        // records
        .route("/records", get(get_aggregated_records))
        .route("/services/{svc}/ports/{port}/records", get(get_service_port_records))
        .route("/records/grouped", get(get_grouped_records))

        // service
        .route("/services/new", put(put_service_supervisor_new))
        .route("/services/{svc}/delete", delete(delete_service_supervisor))
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
