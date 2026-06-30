use crate::api::api_db::{delete_clear_db, delete_prune_db_records, get_db_health, get_db_stats};
use crate::api::api_ecam::get_ecam;
use crate::api::api_health::{get_auth_ping, get_health, get_teapot, get_version};
use crate::api::api_ports::{get_config, get_port_list, put_config};
use crate::api::api_records::get_records;
use crate::api::api_ruleset::{
    get_ruleset, get_ruleset_history, put_ruleset, put_ruleset_validate,
};
use crate::api::api_state::get_state;
use crate::api::api_stats::get_port_stats;
use crate::api::middleware_auth::require_token;
use crate::db::commands::DbHandle;
use crate::{config::Config, registry::PortRegistry};
use axum::routing::{delete, put};
use axum::{Router, middleware, routing::get};
use utoipa::openapi::SecurityRequirement;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

// global fetch
use crate::api::api_health::__path_get_auth_ping;
use crate::api::api_health::__path_get_health;
use crate::api::api_health::__path_get_teapot;
use crate::api::api_health::__path_get_version;

// ecam
use crate::api::api_state::__path_get_state;

// ecam
use crate::api::api_ecam::__path_get_ecam;

// port
use crate::api::api_ports::__path_get_config;
use crate::api::api_ports::__path_get_port_list;
use crate::api::api_ports::__path_put_config;

use crate::api::api_stats::__path_get_port_stats;

// ruleset
use crate::api::api_ruleset::__path_get_ruleset;
use crate::api::api_ruleset::__path_get_ruleset_history;
use crate::api::api_ruleset::__path_put_ruleset;
use crate::api::api_ruleset::__path_put_ruleset_validate;

// records
use crate::api::api_records::__path_get_records;

// db
use crate::api::api_db::__path_delete_clear_db;
use crate::api::api_db::__path_delete_prune_db_records;
use crate::api::api_db::__path_get_db_health;
use crate::api::api_db::__path_get_db_stats;

pub type SharedState = (Config, PortRegistry, DbHandle);

#[derive(OpenApi)]
#[openapi(
    modifiers(&FilterBearerAddon),
    paths(
        // global fetch
        get_health,
        get_auth_ping,
        get_version,
        get_teapot,

        // state
        get_state,

        // ecam
        get_ecam,

        // ports
        get_port_list,
        get_config, put_config,
        get_port_stats,

        // ruleset
        get_ruleset, put_ruleset,
        put_ruleset_validate,
        get_ruleset_history,

        // records
        get_records,

        // db
        get_db_health,
        get_db_stats,
        delete_clear_db,
        delete_prune_db_records,
    ),
    tags(
        (name = "filter", description = "Sinkhole Filter API")
    )
)]
struct ApiDoc;

struct FilterBearerAddon;

impl Modify for FilterBearerAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        const BEARER: &str = "bearerAuth";
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

pub fn build_router(cfg: Config, registry: PortRegistry, db_handle: DbHandle) -> Router {
    let protected = Router::new()
        .route("/auth/ping", get(get_auth_ping))
        .route("/version", get(get_version))
        .route("/teapot", get(get_teapot))
        .route("/state", get(get_state))
        .route("/ecam", get(get_ecam))
        .route("/ports", get(get_port_list))
        .route("/ports/{port}/config", get(get_config).put(put_config))
        .route("/ports/{port}/stats", get(get_port_stats))
        .route("/ports/{port}/ruleset", get(get_ruleset).put(put_ruleset))
        .route("/ports/{port}/ruleset/history", get(get_ruleset_history))
        .route("/ports/{port}/ruleset/validate", put(put_ruleset_validate))
        .route("/ports/{port}/records", get(get_records))
        .route("/db/health", get(get_db_health))
        .route("/db/stats", get(get_db_stats))
        .route("/db/clear", delete(delete_clear_db))
        .route("/db/prune/records", delete(delete_prune_db_records))
        .layer(middleware::from_fn({
            let g = cfg.clone();
            move |req, next| {
                let cfg_for_mw = g.clone();
                async move { require_token(cfg_for_mw, req, next).await }
            }
        }));

    let openapi = SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi());

    // No CORS: the filter control API is server-to-server only. It is reached only
    // by the controller (via `reqwest`, which does not involve CORS); the web UI
    // never talks to a filter directly — it talks to the controller, which relays,
    // and filter liveness is checked controller-side (the `/services/new` health
    // preflight). Emitting no `Access-Control-*` headers means a browser cannot use
    // the filter cross-origin at all. (The filter's own Swagger UI at `/docs` is
    // same-origin and needs no CORS.)
    Router::new()
        .route("/health", get(get_health))
        .merge(protected)
        .merge(openapi)
        .with_state((cfg, registry, db_handle))
}
