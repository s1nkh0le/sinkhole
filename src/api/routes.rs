use axum::{routing::get, Router, };
use crate::api::config::{get_config, list_ports, put_config};
use crate::api::db::{db_health_ep, db_stats_ep};
use crate::api::health::{health, version};
use crate::api::logs::get_logs;
use crate::api::ruleset::{get_ruleset, put_ruleset};
use crate::api::stats::get_stats;
use crate::{
    config::Config,
    registry::PortRegistry
};
use crate::db::actor::DbHandle;

pub type SharedState = (Config, PortRegistry, DbHandle);

pub fn build_router(cfg: Config, registry: PortRegistry, db_handle: DbHandle) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/ports", get(list_ports))
        .route("/ports/{port}/config", get(get_config).put(put_config))
        .route("/ports/{port}/stats", get(get_stats))
        .route("/ports/{port}/ruleset", get(get_ruleset).put(put_ruleset))
        .route("/ports/{port}/logs", get(get_logs))
        .route("/db/health", get(db_health_ep))
        .route("/db/stats",  get(db_stats_ep))
        .with_state((cfg, registry, db_handle))
}
