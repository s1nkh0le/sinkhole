use crate::api::api_ecam::{get_aggregated_ecam, get_service_ecam};
use crate::api::api_health::{get_auth_ping, get_health, get_list_services};
use crate::api::api_ports::{get_list_ports, get_port_config, put_port_config};
use crate::api::api_records::{get_aggregated_records, get_service_port_records};
use crate::api::api_records_grouped::get_grouped_records;
use crate::api::api_ruleset::{
    get_ruleset, get_ruleset_history, put_ruleset, put_validate_ruleset,
};
use crate::api::api_service::{delete_service_supervisor, put_service_supervisor_new};
use crate::api::api_service_meta::get_service_meta;
use crate::api::api_service_meta::put_service_meta;
use crate::api::api_state::get_state;
use crate::api::middleware_auth::require_ctrl_token;

use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, put};
use axum::{Router, middleware, routing::get};
use rust_embed::RustEmbed;
use utoipa::openapi::SecurityRequirement;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

/// global
use crate::api::api_health::__path_get_auth_ping;
use crate::api::api_health::__path_get_health;
use crate::api::api_health::__path_get_list_services;
use crate::api::api_state::__path_get_state;

/// ecam
use crate::api::api_ecam::__path_get_aggregated_ecam;
use crate::api::api_ecam::__path_get_service_ecam;

use crate::api::api_records::__path_get_aggregated_records;
use crate::api::api_records::__path_get_service_port_records;
/// records
use crate::api::api_records_grouped::__path_get_grouped_records;

use crate::api::api_service::__path_delete_service_supervisor;
/// service
use crate::api::api_service::__path_put_service_supervisor_new;
use crate::api::api_service_meta::__path_get_service_meta;
use crate::api::api_service_meta::__path_put_service_meta;

use crate::api::api_ports::__path_get_list_ports;
/// ports config
use crate::api::api_ports::__path_get_port_config;
use crate::api::api_ports::__path_put_port_config;

/// rulesets
use crate::api::api_ruleset::__path_get_ruleset;
use crate::api::api_ruleset::__path_get_ruleset_history;
use crate::api::api_ruleset::__path_put_ruleset;
use crate::api::api_ruleset::__path_put_validate_ruleset;
use crate::supervisor::app_state::AppState;

#[derive(OpenApi)]
#[openapi(
    modifiers(&BearerAddon),
    paths(
        // global
        get_health,
        get_auth_ping,
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
        get_ruleset_history,
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

/// The built web UI (`frontend/dist`), embedded into the binary at compile time in
/// both debug and release (`debug-embed` feature), so behaviour is identical across
/// profiles. If the folder is absent at build time nothing is embedded (serving then
/// no-ops).
#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
struct FrontendAssets;

/// Serve one embedded UI asset, falling back to `index.html` so client-side (SPA)
/// routes resolve on deep-link / refresh.
fn serve_ui_asset(path: &str) -> Response {
    let path = if path.is_empty() { "index.html" } else { path };
    if let Some(file) = FrontendAssets::get(path) {
        let mime = file.metadata.mimetype().to_string();
        return ([(header::CONTENT_TYPE, mime)], file.data.into_owned()).into_response();
    }
    match FrontendAssets::get("index.html") {
        Some(idx) => ([(header::CONTENT_TYPE, "text/html")], idx.data.into_owned()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn ui_index() -> Response {
    serve_ui_asset("index.html")
}
async fn ui_path(Path(path): Path<String>) -> Response {
    serve_ui_asset(&path)
}

pub fn build_router(state: AppState) -> Router {
    let cfg_for_mw = state.ctrl.cfg();

    // CORS policy (shared builder):
    //   * an explicit origin list (SINKHOLE_CONTROLLER_CORS_ORIGIN) => strict
    //     allow-list, honored regardless of UI mode.
    //   * no origin + debug => permissive (`Any`), dev convenience.
    //   * no origin + non-debug => same-origin only (cross-origin denied). Config
    //     load guarantees this branch only when serving the bundled (same-origin)
    //     UI; otherwise it refuses to start.
    let cors = match cfg_for_mw.cors_origin.as_deref() {
        Some(origins) => shared::api::cors::build_cors_layer(Some(origins)),
        None if cfg_for_mw.debug => shared::api::cors::build_cors_layer(None),
        None => shared::api::cors::build_cors_layer_same_origin_only(),
    };

    let protected = Router::new()
        // global
        .route("/auth/ping", get(get_auth_ping))
        .route("/services", get(get_list_services))
        .route("/state", get(get_state))
        // ecam
        .route("/ecam", get(get_aggregated_ecam))
        .route("/services/{svc}/ecam", get(get_service_ecam))
        // records
        .route("/records", get(get_aggregated_records))
        .route(
            "/services/{svc}/ports/{port}/records",
            get(get_service_port_records),
        )
        .route("/records/grouped", get(get_grouped_records))
        // service
        .route("/services/new", put(put_service_supervisor_new))
        .route("/services/{svc}/delete", delete(delete_service_supervisor))
        .route(
            "/services/{svc}/meta",
            get(get_service_meta).put(put_service_meta),
        )
        // ports config
        .route("/services/{svc}/ports", get(get_list_ports))
        .route(
            "/services/{svc}/ports/{port}/config",
            get(get_port_config).put(put_port_config),
        )
        // rulesets
        .route(
            "/services/{svc}/ports/{port}/ruleset",
            get(get_ruleset).put(put_ruleset),
        )
        .route(
            "/services/{svc}/ports/{port}/ruleset/history",
            get(get_ruleset_history),
        )
        .route(
            "/services/{svc}/ports/{port}/ruleset/validate",
            put(put_validate_ruleset),
        )
        // auth
        .layer(middleware::from_fn({
            let cfg_for_mw = cfg_for_mw.clone();
            move |req, next| {
                let cfg_for_mw = cfg_for_mw.clone();
                async move { require_ctrl_token(cfg_for_mw, req, next).await }
            }
        }));

    let openapi = SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi());

    let mut app = Router::new()
        .route("/health", get(get_health))
        .merge(protected)
        .merge(openapi)
        .layer(cors)
        .with_state(state);

    // Optionally serve the embedded web UI under `/ui` (same origin as the API, so
    // no CORS), with an index.html fallback for client-side routing. Mounted under a
    // prefix because the SPA's routes (e.g. /services) would otherwise collide with
    // the API routes at the root. Opt-in via SERVE_FRONTEND; a no-op (with a warning)
    // if the boolean is set but no UI was embedded at build time.
    if cfg_for_mw.serve_frontend {
        if FrontendAssets::get("index.html").is_some() {
            app = app
                .route("/", get(|| async { Redirect::permanent("/ui/") }))
                .route("/ui", get(|| async { Redirect::permanent("/ui/") }))
                .route("/ui/", get(ui_index))
                .route("/ui/{*path}", get(ui_path));
            tracing::info!("serving embedded web UI at /ui");
        } else {
            tracing::warn!("SERVE_FRONTEND is enabled but no web UI was embedded at build time");
        }
    }

    app
}
