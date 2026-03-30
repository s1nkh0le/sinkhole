//! Bidirectional TCP proxy with rule evaluation.
//!
//! When `TLS_ENABLED=true` the proxy terminates TLS from clients, runs
//! the rule engine on the decrypted traffic, and optionally re-encrypts
//! to the upstream challenge.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::observability::sinks::sink_ecam::{Origin, Severity};
use crate::observability::sinks::{sink_ecam, sink_telemetry};
use crate::{
    config::{Config, PortBinding},
    engine::dispatcher::{MessageCtx, evaluate as eval_rules},
    http::parse_http,
    registry::{PortRegistry, PortRuntime},
};
use anyhow::{Result, anyhow};
use bytes::BytesMut;
use shared::sink_model::NetworkOrigin;
use shared::sink_model::NetworkOrigin::{ClientToFilter, Filter};
use shared::types::Decision;
use shared::utils::time::now_ns;
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::TlsAcceptor;
use tokio_rustls::TlsConnector;

static CONN_ID: AtomicU64 = AtomicU64::new(1);

// ---------------------------------------------------------------------------
// TLS state carried through the proxy when `TLS_ENABLED=true`.
// ---------------------------------------------------------------------------

pub struct TlsState {
    pub acceptor: TlsAcceptor,
    /// If `Some`, traffic is re-encrypted to upstream over TLS.
    pub upstream_connector: Option<TlsConnector>,
    /// Upstream hostname used for the SNI extension.
    pub upstream_host: String,
}

// ---------------------------------------------------------------------------
pub async fn run_all_ports(
    cfg: Config,
    registry: PortRegistry,
    tls: Option<Arc<TlsState>>,
) -> Result<()> {
    for pb in cfg.ports.clone() {
        let cfg_clone = cfg.clone();
        let reg_clone = registry.clone();
        let pb_clone = pb.clone();
        let tls_clone = tls.clone();

        tokio::spawn(async move {
            if let Err(e) = run_proxy_for_port(cfg_clone, reg_clone, pb, tls_clone).await {
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
    tls: Option<Arc<TlsState>>,
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
                let tls_clone = tls.clone();

                tokio::spawn(async move {
                    let result = if let Some(ref tls_st) = tls_clone {
                        handle_conn_tls(
                            conn_id,
                            client,
                            client_addr,
                            &upstream_host,
                            upstream_port,
                            public_port,
                            debug,
                            port_rt,
                            tls_st,
                        )
                        .await
                    } else {
                        handle_conn_plain(
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
                    };
                    if let Err(e) = result {
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
// Plain-text connection handler (TLS disabled)
// ---------------------------------------------------------------------------
async fn handle_conn_plain(
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

// ---------------------------------------------------------------------------
// TLS connection handler (TLS_ENABLED=true)
// ---------------------------------------------------------------------------
async fn handle_conn_tls(
    conn_id: u64,
    client_tcp: TcpStream,
    client_addr: SocketAddr,
    upstream_host: &str,
    upstream_port: u16,
    public_port: u16,
    debug: bool,
    port_rt: Arc<PortRuntime>,
    tls_st: &TlsState,
) -> Result<()> {
    // 1. Accept TLS from client ------------------------------------------
    let client_tls = match tls_st.acceptor.accept(client_tcp).await {
        Ok(s) => s,
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::Network(ClientToFilter(public_port)),
                format!(
                    "[conn_id={:04}] TLS handshake with client failed: {}",
                    conn_id, e
                ),
                None,
            );
            return Ok(());
        }
    };

    // 2. Connect to upstream (plain or TLS) ------------------------------
    let upstream_addr = format!("{upstream_host}:{upstream_port}");
    let upstream_tcp = match TcpStream::connect(&upstream_addr).await {
        Ok(s) => {
            tracing::debug!(
                "[conn_id={:04}] connected to upstream (TLS) | filter@{} -> service@{}",
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
                    "[conn_id={:04}] failed to connect to upstream at {} | error=\"{}\"",
                    conn_id, upstream_addr, e
                ),
                None,
            );
            return Ok(());
        }
    };
    let server_addr = upstream_tcp
        .peer_addr()
        .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

    let conn_start_ns = now_ns();

    // Use tokio::io::split because TLS streams don't expose split() by ref.
    let (mut cr, mut cw) = io::split(client_tls);

    if let Some(ref connector) = tls_st.upstream_connector {
        // ---- upstream re-encryption path --------------------------------
        let sni = crate::tls::server_name(&tls_st.upstream_host)?;
        let server_tls = connector.connect(sni, upstream_tcp).await.map_err(|e| {
            anyhow!(
                "[conn_id={:04}] upstream TLS handshake failed: {}",
                conn_id,
                e
            )
        })?;

        let (mut sr, mut sw) = io::split(server_tls);

        let in_task = pump(
            conn_id, true, &mut cr, &mut sw, client_addr, public_port,
            upstream_port, debug, port_rt.clone(), conn_start_ns,
        );
        let out_task = pump(
            conn_id, false, &mut sr, &mut cw, server_addr, public_port,
            upstream_port, debug, port_rt, conn_start_ns,
        );
        let _ = tokio::try_join!(in_task, out_task)?;
    } else {
        // ---- upstream plain-text path -----------------------------------
        let (mut sr, mut sw) = io::split(upstream_tcp);

        let in_task = pump(
            conn_id, true, &mut cr, &mut sw, client_addr, public_port,
            upstream_port, debug, port_rt.clone(), conn_start_ns,
        );
        let out_task = pump(
            conn_id, false, &mut sr, &mut cw, server_addr, public_port,
            upstream_port, debug, port_rt, conn_start_ns,
        );
        let _ = tokio::try_join!(in_task, out_task)?;
    }

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
    conn_start_ns: u128,
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
                conn_prev_msg_ns: last_msg_ns,
            };

            last_msg_ns = conn_now_ns;

            let compiled = port_rt.compiled.load();
            let (decision, rec) = eval_rules(&compiled, &port_rt, &msg_ctx, debug);

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
