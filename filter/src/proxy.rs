use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::observability::sinks::sink_ecam::{Origin, Severity};
use crate::observability::sinks::{sink_ecam, sink_telemetry};
use crate::{
    config::{Config, PortBinding},
    engine::dispatcher::{MessageCtx, evaluate},
    http::parse_http,
    registry::{PortRegistry, PortRuntime},
};
use anyhow::{Result, anyhow};
use bytes::BytesMut;
use shared::types::ecam_types::NetworkOrigin;
use shared::types::ecam_types::NetworkOrigin::{ClientToFilter, Filter};
use shared::types::base_types::Decision;
use shared::utils::time::now_ns;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

static CONN_ID: AtomicU64 = AtomicU64::new(1);

// ---------------------------------------------------------------------------
pub async fn run_all_ports(cfg: Config, registry: PortRegistry) -> Result<()> {
    for pb in cfg.ports.clone() {
        let cfg_clone = cfg.clone();
        let reg_clone = registry.clone();
        let pb_clone = pb.clone(); // keep for log after move

        tokio::spawn(async move {
            if let Err(e) = run_proxy_for_port(cfg_clone, reg_clone, pb).await {
                sink_ecam::record(
                    Severity::Error,
                    Origin::Network(Filter),
                    format!("proxy for port {} failed. error={}", pb_clone.public, e),
                    None,
                );
            }
        });
    }
    Ok(())
}

pub(crate) async fn run_proxy_for_port(
    cfg: Config,
    registry: PortRegistry,
    pb: PortBinding,
) -> Result<()> {
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], pb.public));
    let listener = TcpListener::bind(bind_addr).await.map_err(|e| {
        sink_ecam::record(
            Severity::Error,
            Origin::Network(ClientToFilter(pb.public)),
            format!("unable to spawn proxy on address {bind_addr}: '{e}'"),
            None,
        );

        anyhow!("unable to spawn proxy on address {bind_addr}: '{e}'")
    })?;

    tracing::info!(
        "proxy listening on {} → {}:{}",
        bind_addr,
        cfg.upstream_host,
        pb.upstream
    );

    loop {
        match listener.accept().await {
            Ok((client, client_addr)) => {
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
                        sink_ecam::record(
                            Severity::Error,
                            Origin::Network(ClientToFilter(pb.public)),
                            format!(
                                "error in handle_conn(conn_id={:04}, client=..., client_addr={}, upstream_host={}, upstream_port={}, public_port={}), error={}",
                                conn_id, client_addr, upstream_host, upstream_port, public_port, e
                            ),
                            None,
                        );
                    }
                });
            }
            Err(e) => {
                sink_ecam::record(
                    Severity::Error,
                    Origin::Network(ClientToFilter(pb.public)),
                    format!(
                        "[port={}] listener.accept failed on {} | stopping proxy. error={}",
                        pb.public, bind_addr, e
                    ),
                    None,
                );

                // TODO should we just break or restart the proxy?
                break;
            }
        }
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
    let mut server = match TcpStream::connect(&upstream_addr).await {
        Ok(s) => {
            tracing::debug!(
                "[conn_id={:04}] connected to upstream | filter@{} -> service@{}",
                conn_id,
                public_port,
                upstream_addr
            );
            s
        }
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::Network(NetworkOrigin::ServerToFilter(upstream_port)),
                format!(
                    "[conn_id={:04}] failed to connect to upstream at {} | closing client connection as well. error=\"{}\"",
                    conn_id, upstream_addr, e
                ),
                None,
            );
            return Ok(()); // swallow the error, so our spawn logger won’t trigger
        }
    };

    let server_addr = server
        .peer_addr()
        .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

    let (mut cr, mut cw) = client.split();
    let (mut sr, mut sw) = server.split();

    let conn_start_ns = now_ns();

    let in_task = pump(
        conn_id,
        true, /* Inbound, as in, coming from client -> filter -> server */
        &mut cr,
        &mut sw,
        client_addr,
        public_port,
        upstream_port,
        debug,
        port_rt.clone(),
        conn_start_ns,
    );
    let out_task = pump(
        conn_id,
        false, /* Outbound, as in, going to client <- filter <- server */
        &mut sr,
        &mut cw,
        server_addr,
        public_port,
        upstream_port,
        debug,
        port_rt,
        conn_start_ns,
    );

    let _ = tokio::try_join!(in_task, out_task)?;
    Ok(())
}

///
/// # Arguments
///
/// * `conn_id`: TCP connection id
/// * `inbound`: Whether the direction of traffic is from client to service (inbound), or from service to client (outbound)
/// * `reader`: Client/Service reading buffer
/// * `writer`: Service/Client writing buffer
/// * `src_addr`: Address of entity sending data
/// * `public_port`: External, public port of the filter proxy for clients
/// * `upstream_port`: Internal, private port of the service, just used for forwarding
/// * `debug`: Whether debugging is enabled
/// * `port_rt`: Port runtime
/// * `conn_start_ns`: Time (ns) at which this TCP connection has started
///
/// returns: Result<(), Error>
///
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
    conn_start_ns: u64,
) -> Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut seq = 0u64;
    let mut buf = BytesMut::with_capacity(8192);

    // helper closure --------------------------------------------------------
    let mut last_msg_ns = conn_start_ns;

    let mut process_chunk =
        async |raw: &[u8], http_opt: Option<&crate::http::HttpMessage>| -> Result<bool> {
            let conn_now_ns = now_ns();

            // ---- build ctx & evaluate ----------------------------------------
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
                conn_start_ns: conn_start_ns,
                conn_prev_msg_ns: conn_now_ns,
            };

            last_msg_ns = conn_now_ns;

            let compiled = port_rt.compiled.load();
            let early_stop = false; // debug
            let (decision, rec) = evaluate(&compiled, &port_rt, &msg_ctx, early_stop);

            sink_telemetry::record(&rec);

            // ---- log & maybe forward -----------------------------------------
            if inbound {
                tracing::trace!(
                    bytes = raw.len(),
                    ?decision,
                    "processed inbound  chunk: conn_id={:04}/seq={}, client@{}:{} -> filter:{} ",
                    conn_id,
                    seq,
                    src_addr.ip().to_string(),
                    src_addr.port().to_string(),
                    public_port
                );
            } else {
                tracing::trace!(
                    bytes = raw.len(),
                    ?decision,
                    "processed outbound chunk: conn_id={:04}/seq={}, filter <- service@{}:{} ",
                    conn_id,
                    seq,
                    src_addr.ip().to_string(),
                    src_addr.port().to_string()
                );
            }

            seq += 1;
            if decision == Decision::Allow {
                writer.write_all(raw).await?;
                Ok(true)
            } else {
                Ok(false) // signal to stop
            }
        };

    // main read loop --------------------------------------------------------
    loop {
        let read_n = reader.read_buf(&mut buf).await?;
        if read_n == 0 {
            if inbound {
                tracing::trace!("[conn_id={:04}] connection closed by client", conn_id);
            } else {
                tracing::trace!("[conn_id={:04}] connection closed by service", conn_id);
            }
            break; // EOF
        }

        if inbound {
            tracing::trace!("[conn_id={:04}] read {} bytes from client", conn_id, read_n);
        } else {
            tracing::trace!(
                "[conn_id={:04}] read {} bytes from service",
                conn_id,
                read_n
            );
        }

        // complete HTTP message?
        if let Ok(Some((parsed, used))) = parse_http(&buf, inbound) {
            let raw = buf.split_to(used).freeze();
            if !process_chunk(&raw, Some(&parsed.msg)).await? {
                return Ok(());
            }
            continue;
        }

        // buffer too large? evaluate as raw stream
        if buf.len() > 8192 {
            let raw = buf.split().freeze();
            if !process_chunk(&raw, None).await? {
                return Ok(());
            }
        }
    }

    // leftover bytes after EOF
    if !buf.is_empty() {
        let raw = buf.split().freeze();
        process_chunk(&raw, None).await?;
    }

    writer.flush().await?;
    Ok(())
}
