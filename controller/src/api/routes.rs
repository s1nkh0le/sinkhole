use crate::api::api_cached::{get_cached_port_config, get_cached_port_ruleset};
use crate::api::api_ecam::get_ecam;
use crate::api::api_ports::{get_port_config, get_port_info, list_ports, put_port_config};
use crate::api::api_records::get_records;
use crate::api::api_ruleset::{get_ruleset, put_ruleset};
use crate::api::api_state::get_state;
use crate::api::auth::require_ctrl_token;
use crate::api::global::{get_ecam_global, get_records_global};
use crate::api::health::{health, list_services};
use crate::state::ControllerState;
use axum::{middleware, routing::get, Router};

pub fn build_router(state: ControllerState) -> Router {
    let cfg_for_mw = state.cfg(); // Arc<ControllerCfg>

    let protected = Router::new()
        // state
        // .route("/health", get(health))
        .route("/services", get(list_services))
        .route("/state", get(get_state))
        // aggregates
        .route("/ecam", get(get_ecam_global))
        .route("/records", get(get_records_global))
        // svc
        .route("/services/{svc}/ports", get(list_ports))
        .route("/services/{svc}/ports/{port}/info", get(get_port_info))
        .route(
            "/services/{svc}/ports/{port}/ruleset",
            get(get_ruleset).put(put_ruleset),
        )
        .route("/services/{svc}/ecam", get(get_ecam))
        .route("/services/{svc}/ports/{port}/logs", get(get_records))
        .route(
            "/services/{svc}/ports/{port}/config",
            get(get_port_config).put(put_port_config),
        )
        .route(
            "/services/{svc}/ports/{port}/config/cached",
            get(get_cached_port_config),
        )
        .route(
            "/services/{svc}/ports/{port}/ruleset/cached",
            get(get_cached_port_ruleset),
        )
        .layer(middleware::from_fn({
            let cfg_for_mw = cfg_for_mw.clone();
            move |req, next| {
                let cfg_for_mw = cfg_for_mw.clone();
                async move { require_ctrl_token(cfg_for_mw, req, next).await }
            }
        }));

    Router::new()
        .route("/health", get(health))
        .merge(protected)
        .with_state(state)
}
