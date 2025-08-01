use crate::{config::Config, state::ControllerState};
use reqwest::Client;

pub fn spawn_all(cfg: &Config, state: ControllerState) {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    for svc in &cfg.services {
        let svc_cfg = svc.clone();
        let state   = state.clone();
        let client  = client.clone();
        let interval = cfg.pull_interval_secs;

        tokio::spawn(async move {
            loop {
                if let Err(e) = pull_one_service(&client, &svc_cfg, &state).await {
                    tracing::warn!(service = %svc_cfg.id, error = %e, "pull failed");
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
            }
        });
    }
}

async fn pull_one_service(
    client: &Client,
    svc: &crate::config::ServiceCfg,
    state: &ControllerState,
) -> anyhow::Result<()> {
    use anyhow::Context;

    // Pull ECAM -----------------------------------------------------------
    let ecam_url = format!("{}/ecam?token={}", svc.url, svc.token);
    let ecam_resp: serde_json::Value = client.get(&ecam_url)
        .send().await?.error_for_status()?
        .json().await.context("decoding /ecam")?;

    // convert to Vec<EcamEvent>, push into state … (omitted)

    // Pull ports list -----------------------------------------------------
    let ports_url = format!("{}/ports?token={}", svc.url, svc.token);
    let ports: serde_json::Value = client.get(&ports_url)
        .send().await?.error_for_status()?
        .json().await?;

    // For every port, pull logs if needed … (omitted)

    Ok(())
}
