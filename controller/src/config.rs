use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceCfg {
    pub id:    String,
    pub url:   String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub services: Vec<ServiceCfg>,
    pub pull_interval_secs: u64,
}

pub fn load() -> anyhow::Result<Config> {
    // read `FILTER_0_URL`, `FILTER_0_TOKEN`, `FILTER_1_URL`, …
    let mut services = Vec::new();
    for idx in 0.. {
        let url_key   = format!("FILTER_{idx}_URL");
        let token_key = format!("FILTER_{idx}_TOKEN");
        let Some(url) = std::env::var_os(&url_key) else { break };
        let token = std::env::var(&token_key)
            .unwrap_or_else(|_| panic!("missing {}", token_key));
        services.push(ServiceCfg {
            id: format!("svc{idx}"),
            url: url.into_string().unwrap(),
            token,
        });
    }

    if services.is_empty() {
        anyhow::bail!("no FILTER_n_URL variables found");
    }

    let pull_interval_secs =
        std::env::var("PULL_INTERVAL_SECS").ok()
            .and_then(|v| v.parse().ok()).unwrap_or(2);

    Ok(Config { services, pull_interval_secs })
}
