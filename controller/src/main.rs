use tokio::{net::TcpListener};
use tracing_subscriber::{fmt, EnvFilter};
use crate::api::routes;

mod api;
mod config;
mod db;
mod puller;
mod state;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();

    let cfg = config::load()?;
    let db = db::init(&cfg.controller.db_path)?;
    let state = state::ControllerState::new(cfg.clone(), db.clone())?;

    puller::spawn_all_pull_tasks(state.clone(), reqwest::Client::new(), db.clone());

    let app = routes::build_router(state.clone());

    let bind = std::env::var("CONTROLLER_BIND").unwrap_or_else(|_| "0.0.0.0:7000".into());
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!("controller API listening on {bind}");

    let server = axum::serve(listener, app);

    tokio::select! {
        res = server => {
            res?;
        }
        // _ = signal::ctrl_c() => {
        //     tracing::info!("CTRL-C received – shutting down");
        // }
    }

    let _ = db.tx.send(db::Cmd::Shutdown).await;


    Ok(())
}
