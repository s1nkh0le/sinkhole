use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::{net::SocketAddr, str::FromStr};

#[derive(Clone, Debug, Deserialize)]
pub struct RawConfig {
    pub upstream_host: String,
    #[serde(default)]
    pub listen_map: String, // "8080:80,8443:8000"
    #[serde(default = "default_ctrl_port")]
    pub ctrl_port: u16,
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub auth_hash: Option<String>,
    #[serde(default)]
    pub body_limit_bytes: Option<usize>,
}

fn default_ctrl_port() -> u16 { 7000 }

#[derive(Clone, Debug)]
pub struct PortBinding {
    pub public: u16,
    pub upstream: u16,
}


#[derive(Clone, Debug)]
pub struct Config {
    pub upstream_host: String,
    pub ports: Vec<PortBinding>,
    pub ctrl_port: u16,
    pub debug: bool,
    pub auth_hash: Option<String>,
    pub body_limit_bytes: usize,
}

pub fn load() -> Result<Config> {
    let raw: RawConfig = envy::from_env()?;

    let ports = parse_listen_map(&raw.listen_map)?;
    let body_limit_bytes = raw.body_limit_bytes.unwrap_or(1_048_576);

    Ok(Config {
        upstream_host: raw.upstream_host,
        ports,
        ctrl_port: raw.ctrl_port,
        debug: raw.debug,
        auth_hash: raw.auth_hash,
        body_limit_bytes,
    })
}

fn parse_listen_map(s: &str) -> Result<Vec<PortBinding>> {
    if s.trim().is_empty() {
        return Err(anyhow!("LISTEN_MAP is empty"));
    }
    let mut out = Vec::new();
    for pair in s.split(',') {
        let (a, b) = pair
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid pair '{}'", pair))?;
        let public = u16::from_str(a.trim())?;
        let upstream = u16::from_str(b.trim())?;
        out.push(PortBinding { public, upstream });
    }
    Ok(out)
}

impl Config {
    pub fn ctrl_bind_addr(&self) -> SocketAddr {
        // listen on all interfaces
        SocketAddr::from(([0, 0, 0, 0], self.ctrl_port))
    }
}
