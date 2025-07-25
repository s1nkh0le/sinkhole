//! Bidirectional TCP proxy with rule evaluation.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use bytes::BytesMut;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

use crate::{
    config::{Config, PortBinding},
    engine::dispatcher::{evaluate as eval_rules, MessageCtx},
    http::parse_http,
    observability::{eval_record::EvaluationRecord, telemetry::emit_record},
    registry::{PortRegistry, PortRuntime},
    types::{Decision, Direction},
    utils::now_ns,
};

static CONN_ID: AtomicU64 = AtomicU64::new(1);

// ---------------------------------------------------------------------------
pub async fn run_all_ports(cfg: Config, registry: PortRegistry) -> Result<()> {
    for pb in cfg.ports.clone() {
        let cfg_clone = cfg.clone();
        let reg_clone = registry.clone();
        let pb_clone = pb.clone(); // keep for log after move

        tokio::spawn(async move {
            if let Err(e) =
                run_proxy_for_port(cfg_clone, reg_clone, pb).await
            {
                tracing::error!(
                    error=?e,
                    "proxy for port {} failed",
                    pb_clone.public
                );
            }
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
pub(crate) async fn run_proxy_for_port(
    cfg: Config,
    registry: PortRegistry,
    pb: PortBinding,
) -> Result<()> {
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], pb.public));
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(
        "proxy listening on {} → {}:{}",
        bind_addr,
        cfg.upstream_host,
        pb.upstream
    );

    while let Ok((client, client_addr)) = listener.accept().await {
        let conn_id = CONN_ID.fetch_add(1, Ordering::Relaxed);
        let upstream_host = cfg.upstream_host.clone();
        let upstream_port = pb.upstream;
        let public_port = pb.public;
        let debug = cfg.debug;
        let port_rt = registry
            .get(&public_port)
            .expect("PortRuntime missing")
            .clone();

        tokio::spawn(async move {
            if let Err(e) = handle_conn(
                conn_id,
                client,
                client_addr,
                &upstream_host,
                upstream_port,
                public_port,
                debug,
                port_rt,
            )
                .await
            {
                tracing::warn!(conn_id, error=?e, "connection errored");
            }
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
async fn handle_conn(
    conn_id: u64,
    mut client: TcpStream,
    client_addr: SocketAddr,
    upstream_host: &str,
    upstream_port: u16,
    public_port: u16,
    debug: bool,
    port_rt: Arc<PortRuntime>,
) -> Result<()> {
    let upstream_addr = format!("{upstream_host}:{upstream_port}");
    let mut server = TcpStream::connect(&upstream_addr).await?;

    let server_addr = server.peer_addr().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

    let (mut cr, mut cw) = client.split();
    let (mut sr, mut sw) = server.split();

    let in_task = pump(
        conn_id,
        true,
        &mut cr,
        &mut sw,
        client_addr,
        public_port,
        upstream_port,
        debug,
        port_rt.clone(),
    );
    let out_task = pump(
        conn_id,
        false,
        &mut sr,
        &mut cw,
        server_addr,
        public_port,
        upstream_port,
        debug,
        port_rt,
    );

    let _ = tokio::try_join!(in_task, out_task)?;
    Ok(())
}

// ---------------------------------------------------------------------------
async fn pump<R, W>(
    conn_id: u64,
    inbound: bool,
    reader: &mut R,
    writer: &mut W,
    src_addr: SocketAddr,
    public_port: u16,
    upstream_port: u16,
    debug: bool,
    port_rt: Arc<PortRuntime>,
) -> Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut seq = 0u64;
    let mut buf = BytesMut::with_capacity(8192);

    loop {
        let read_n = reader.read_buf(&mut buf).await?;
        if read_n == 0 {
            break;
        }

        // try parse HTTP
        let eval = |raw: &[u8], http_opt: Option<&crate::http::HttpMessage>| -> Decision {
            let msg_ctx = MessageCtx {
                conn_id,
                seq,
                inbound,
                public_port,
                upstream_port,
                raw,
                http: http_opt,
                src_ip: &src_addr.ip().to_string(),
                src_port: src_addr.port(),
            };
            let compiled = port_rt.compiled.load();
            let (decision, rec) = eval_rules(&compiled, &port_rt, &msg_ctx, debug);
            emit_record(&rec);
            decision
        };

        if let Ok(Some((parsed, used))) = parse_http(&buf) {
            let raw = buf.split_to(used).freeze();
            let decision = eval(&raw, Some(&parsed.msg));
            seq += 1;

            if decision == Decision::Allow {
                writer.write_all(&raw).await?;
            } else if !debug {
                break;
            }
        } else if buf.len() > 8192 {
            let raw = buf.split().freeze();
            let decision = eval(&raw, None);
            seq += 1;

            if decision == Decision::Allow {
                writer.write_all(&raw).await?;
            } else if !debug {
                break;
            }
        }
    }

    // after the loop
    if !buf.is_empty() {
        // treat remaining bytes as a raw stream message
        let raw = buf.split().freeze();
        let decision = {
            let msg_ctx = MessageCtx {
                conn_id,
                seq,
                inbound,
                public_port,
                upstream_port,
                raw: &raw,
                http: None,
                src_ip: &src_addr.ip().to_string(),
                src_port: src_addr.port(),
            };
            let compiled = port_rt.compiled.load();
            let (d, rec) = eval_rules(&compiled, &port_rt, &msg_ctx, debug);
            emit_record(&rec);
            d
        };

        if decision == Decision::Allow {
            writer.write_all(&raw).await?;
        }
    }

    writer.flush().await?;
    Ok(())
}
