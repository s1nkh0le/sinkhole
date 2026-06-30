use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::db::commands::{DbHandle, FilterDbCmd};
use crate::observability::sinks::sink_ecam::{Origin, Severity};
use crate::observability::sinks::{sink_ecam, sink_telemetry};
use crate::{
    config::{Config, PortBinding},
    engine::dispatcher::{MessageCtx, evaluate},
    http::{H2_PREFACE, HttpParse, classify_raw, http_protocol, parse_http},
    registry::{PortRegistry, PortRuntime},
};
use shared::types::base_types::Protocol;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// Classify a `listener.accept()` error as transient (the listener can keep
/// running) or fatal (something about the socket itself is wrong). Resource
/// exhaustion — per-process fd limit (`EMFILE`), system-wide fd limit (`ENFILE`),
/// kernel buffers/memory (`ENOBUFS`/`ENOMEM`) — and per-connection failures the
/// peer caused (`ECONNABORTED`, `EINTR`, `EPROTO`) are transient: dropping the
/// whole listener would take a guarded host offline over a momentary spike.
/// Anything else (e.g. `EBADF`/`ENOTSOCK`/`EINVAL`) means the listening socket is
/// broken and the loop should stop.
fn accept_error_is_transient(e: &io::Error) -> bool {
    use io::ErrorKind::*;
    if matches!(e.kind(), ConnectionAborted | Interrupted | OutOfMemory) {
        return true;
    }
    // EMFILE(24)/ENFILE(23)/ENOBUFS(105)/EPROTO(71) have no stable `ErrorKind`
    // on this toolchain, so match the raw errno (Linux values).
    matches!(e.raw_os_error(), Some(24 | 23 | 105 | 71))
}

/// Protocol tag for an unframed (raw) chunk: `http/2` once the connection's h2
/// preface has been seen, otherwise an ascii/binary classification of the bytes.
#[inline]
fn raw_protocol(is_h2: bool, raw: &[u8]) -> Protocol {
    if is_h2 {
        Protocol::Http2
    } else {
        classify_raw(raw)
    }
}
use anyhow::{Result, anyhow};
use bytes::BytesMut;
use shared::types::base_types::Decision;
use shared::types::ecam_types::NetworkOrigin;
use shared::types::ecam_types::NetworkOrigin::{ClientToFilter, Filter};
use shared::utils::time::now_ns;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Mutex, Semaphore, oneshot};
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Headroom above the body limit for the raw-flush threshold, so a message whose
/// body is right at `body_limit` plus its (small) HTTP headers can still be fully
/// buffered and parsed as HTTP rather than tipping into a raw-stream flush.
const RAW_FLUSH_HEADROOM: usize = 4096;

/// Monotonic connection-id allocator (hi/lo). Hands out ids from an in-memory
/// block reserved from the filter DB's `meta` counter; reserves a fresh block
/// (via the DB actor) when the current one is exhausted. Ids are unique and
/// monotonic across restarts (never reused), continuous within a run, with gaps
/// only where a run ends mid-block. See `reserve_conn_id_block`.
pub struct ConnIdAllocator {
    db: DbHandle,
    /// `(next, ceiling)`: hand out `next` while `next < ceiling`. `(0, 0)` ⇒
    /// reserve on first use.
    state: Mutex<(u64, u64)>,
}

impl ConnIdAllocator {
    pub fn new(db: DbHandle) -> Self {
        Self {
            db,
            state: Mutex::new((0, 0)),
        }
    }

    /// Allocate the next connection id. Cheap (a lock + increment); only the rare
    /// block-boundary crossing awaits the DB actor.
    pub async fn next(&self) -> u64 {
        let mut g = self.state.lock().await;
        if g.0 >= g.1 {
            *g = self.reserve().await;
        }
        let id = g.0;
        g.0 += 1;
        id
    }

    async fn reserve(&self) -> (u64, u64) {
        let (tx, rx) = oneshot::channel();
        if self
            .db
            .tx
            .send(FilterDbCmd::ReserveConnIds { reply: tx })
            .await
            .is_err()
        {
            // Actor gone (shutdown): degrade to a large in-process range so we
            // never block; ids stay unique within this run.
            return (1, u64::MAX);
        }
        rx.await.unwrap_or((1, u64::MAX))
    }
}

/// The filter→upstream connection. Plaintext for a plain service; a TLS client
/// stream for a TLS service (the re-encrypt leg). Both variants are plain
/// `AsyncRead + AsyncWrite`, so everything downstream (`pump`, the engine,
/// records) is identical regardless of transport — the crypto is unwrapped here
/// at the edge and never seen again.
enum UpstreamStream {
    Plain(TcpStream),
    Tls(tokio_rustls::client::TlsStream<TcpStream>),
}

impl AsyncRead for UpstreamStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            UpstreamStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for UpstreamStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            UpstreamStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_flush(cx),
            UpstreamStream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            UpstreamStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            UpstreamStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

// ---------------------------------------------------------------------------
/// Bind every public port up front, failing fast if any is unavailable.
///
/// Done before serving so a port clash (e.g. address already in use) surfaces as
/// one clean error the caller can exit on, rather than each listener dying in its
/// own task and leaving a half-up filter that silently proxies nothing.
pub async fn bind_all_ports(cfg: &Config) -> Result<Vec<(PortBinding, TcpListener)>> {
    let mut bound = Vec::with_capacity(cfg.ports.len());
    for pb in cfg.ports.clone() {
        let bind_addr = SocketAddr::from(([0, 0, 0, 0], pb.public));
        let listener = TcpListener::bind(bind_addr).await.map_err(|e| match e.kind() {
            // Fatal: the caller exits, so a single clean error beats also emitting a
            // raw ECAM record that wouldn't be centralized before shutdown anyway.
            std::io::ErrorKind::AddrInUse => anyhow!(
                "public port {bind_addr} is already in use; is another filter or service bound to it?"
            ),
            _ => anyhow!("unable to bind proxy on address {bind_addr}: {e}"),
        })?;
        bound.push((pb, listener));
    }
    Ok(bound)
}

pub async fn run_all_ports(
    cfg: Config,
    registry: PortRegistry,
    cancel: CancellationToken,
    conn_alloc: Arc<ConnIdAllocator>,
    bound: Vec<(PortBinding, TcpListener)>,
) -> Result<()> {
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    // Global cap on simultaneous connections across all ports (fd/memory is a
    // process-wide resource). `None` when disabled (MAX_CONNECTIONS=0).
    let conn_sem = (cfg.max_connections > 0).then(|| Arc::new(Semaphore::new(cfg.max_connections)));

    for (pb, listener) in bound {
        let cfg_clone = cfg.clone();
        let reg_clone = registry.clone();
        let pb_clone = pb.clone(); // keep for log after move
        let cancel_clone = cancel.clone();
        let alloc = conn_alloc.clone();
        let sem = conn_sem.clone();

        handles.push(tokio::spawn(async move {
            if let Err(e) =
                run_proxy_for_port(cfg_clone, reg_clone, pb, listener, cancel_clone, alloc, sem)
                    .await
            {
                sink_ecam::record(
                    Severity::Error,
                    Origin::Network(Filter),
                    format!("proxy for port {} failed. error={}", pb_clone.public, e),
                    None,
                );
            }
        }));
    }

    // Wait for all listeners to stop (they exit when `cancel` is triggered).
    for h in handles {
        let _ = h.await;
    }
    Ok(())
}

pub(crate) async fn run_proxy_for_port(
    cfg: Config,
    registry: PortRegistry,
    pb: PortBinding,
    listener: TcpListener,
    cancel: CancellationToken,
    conn_alloc: Arc<ConnIdAllocator>,
    conn_sem: Option<Arc<Semaphore>>,
) -> Result<()> {
    // The listener is already bound (see bind_all_ports); this is just for logging.
    let bind_addr = SocketAddr::from(([0, 0, 0, 0], pb.public));

    // Effective upstream host for this port: the per-port override if present,
    // else the global UPSTREAM_HOST. Lets one filter guard several services.
    let port_upstream_host = pb
        .upstream_host
        .clone()
        .unwrap_or_else(|| cfg.upstream_host.clone());

    tracing::info!(
        "proxy listening on {} → {}:{}",
        bind_addr,
        port_upstream_host,
        pb.upstream
    );

    // Build the TLS acceptor (inbound termination) and connector (upstream
    // re-encrypt) once per port if a certificate is configured. A TLS port is
    // TLS end-to-end: both are built together or neither is. A bad cert/key
    // fails the port immediately rather than per connection.
    let (tls_acceptor, tls_connector): (Option<TlsAcceptor>, Option<TlsConnector>) = match &pb.tls {
        Some(files) => {
            let acceptor = crate::tls::build_acceptor(&files.cert_path, &files.key_path)
                .map_err(|e| anyhow!("TLS setup failed for port {}: {e}", pb.public))?;
            let connector = crate::tls::build_connector()
                .map_err(|e| anyhow!("upstream TLS setup failed for port {}: {e}", pb.public))?;
            tracing::info!(
                "[port={}] TLS enabled: terminate inbound + re-encrypt to upstream",
                pb.public
            );
            (Some(acceptor), Some(connector))
        }
        None => (None, None),
    };

    // Throttle the "cap reached" ECAM so a connection flood can't drown the log.
    let mut last_cap_warn_ns: u64 = 0;

    loop {
        let accepted = tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("[port={}] shutdown requested, stopping listener", pb.public);
                break;
            }
            res = listener.accept() => res,
        };

        match accepted {
            Ok((client, client_addr)) => {
                // Enforce the simultaneous-connection cap: take a permit (held for
                // the connection's lifetime). At capacity, drop the freshly
                // accepted socket rather than queue it, which would be its own backlog.
                let permit = match &conn_sem {
                    Some(sem) => match sem.clone().try_acquire_owned() {
                        Ok(p) => Some(p),
                        Err(_) => {
                            let now = now_ns();
                            if now.saturating_sub(last_cap_warn_ns) > 5_000_000_000 {
                                last_cap_warn_ns = now;
                                sink_ecam::record(
                                    Severity::Warn,
                                    Origin::Network(ClientToFilter(pb.public)),
                                    format!(
                                        "connection cap ({}) reached on port {}; rejecting new connections",
                                        cfg.max_connections, pb.public
                                    ),
                                    None,
                                );
                            }
                            drop(client);
                            continue;
                        }
                    },
                    None => None,
                };
                let conn_id = conn_alloc.next().await;
                let upstream_host = port_upstream_host.clone();
                let upstream_port = pb.upstream;
                let public_port = pb.public;
                let body_limit = cfg.body_limit_bytes;
                let idle_flush_ms = cfg.idle_flush_ms;
                let conn_idle_timeout_secs = cfg.conn_idle_timeout_secs;
                let conn_max_lifetime_secs = cfg.conn_max_lifetime_secs;
                let port_rt = match registry.get(&public_port) {
                    Some(rt) => rt.clone(),
                    None => {
                        sink_ecam::record(
                            Severity::Error,
                            Origin::Network(ClientToFilter(public_port)),
                            format!(
                                "no PortRuntime registered for public port {public_port}; dropping connection"
                            ),
                            None,
                        );
                        continue;
                    }
                };
                let conn_cancel = cancel.clone();
                let tls = tls_acceptor.clone();
                let upstream_connector = tls_connector.clone();

                tokio::spawn(async move {
                    // Released when this connection ends, freeing a cap slot.
                    let _permit = permit;
                    let result = match tls {
                        Some(acceptor) => match acceptor.accept(client).await {
                            Ok(tls_stream) => {
                                handle_conn(
                                    conn_id,
                                    tls_stream,
                                    client_addr,
                                    &upstream_host,
                                    upstream_port,
                                    public_port,
                                    body_limit,
                                    idle_flush_ms,
                                    conn_idle_timeout_secs,
                                    conn_max_lifetime_secs,
                                    port_rt,
                                    upstream_connector,
                                    conn_cancel,
                                )
                                .await
                            }
                            Err(e) => {
                                sink_ecam::record(
                                    Severity::Warn,
                                    Origin::Network(ClientToFilter(public_port)),
                                    format!(
                                        "[conn_id={conn_id:04}] TLS handshake failed on port {public_port}: {e}"
                                    ),
                                    None,
                                );
                                return;
                            }
                        },
                        None => {
                            handle_conn(
                                conn_id,
                                client,
                                client_addr,
                                &upstream_host,
                                upstream_port,
                                public_port,
                                body_limit,
                                idle_flush_ms,
                                conn_idle_timeout_secs,
                                conn_max_lifetime_secs,
                                port_rt,
                                upstream_connector,
                                conn_cancel,
                            )
                            .await
                        }
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
                if accept_error_is_transient(&e) {
                    // Resource exhaustion / peer-caused failure. Keep the listener
                    // alive — tearing it down would take the guarded host offline
                    // over a momentary spike. Back off briefly so we don't spin at
                    // 100% CPU while the condition (e.g. fd table full) clears.
                    sink_ecam::record(
                        Severity::Warn,
                        Origin::Network(ClientToFilter(pb.public)),
                        format!(
                            "[port={}] transient accept() error on {}, backing off 100ms. error={}",
                            pb.public, bind_addr, e
                        ),
                        None,
                    );
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }

                sink_ecam::record(
                    Severity::Error,
                    Origin::Network(ClientToFilter(pb.public)),
                    format!(
                        "[port={}] fatal accept() error on {} | stopping listener. error={}",
                        pb.public, bind_addr, e
                    ),
                    None,
                );
                break;
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
async fn handle_conn<C>(
    conn_id: u64,
    client: C,
    client_addr: SocketAddr,
    upstream_host: &str,
    upstream_port: u16,
    public_port: u16,
    body_limit: usize,
    idle_flush_ms: u64,
    idle_timeout_secs: u64,
    lifetime_secs: u64,
    port_rt: Arc<PortRuntime>,
    upstream_connector: Option<TlsConnector>,
    cancel: CancellationToken,
) -> Result<()>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    let upstream_addr = format!("{upstream_host}:{upstream_port}");
    let tcp = match TcpStream::connect(&upstream_addr).await {
        Ok(s) => {
            tracing::debug!(
                "[conn_id={:04}] connected to upstream | filter@{} -> service@{} (tls={})",
                conn_id,
                public_port,
                upstream_addr,
                upstream_connector.is_some()
            );
            s
        }
        Err(e) => {
            sink_ecam::record(
                Severity::Warn,
                Origin::Network(NetworkOrigin::ServerToFilter(upstream_port)),
                format!(
                    "[conn_id={:04}] upstream connect failed: public port {} → service {} is unreachable; \
                     dropping client {}. error kind=\"{}\", detail=\"{}\"",
                    conn_id,
                    public_port,
                    upstream_addr,
                    client_addr,
                    e.kind(),
                    e
                ),
                None,
            );
            return Ok(()); // swallow the error, so our spawn logger won’t trigger
        }
    };

    // Capture the peer address from the raw socket before any TLS wrapping.
    let server_addr = tcp
        .peer_addr()
        .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());

    // A TLS port re-encrypts to the upstream: open a fresh TLS client connection
    // over the TCP socket. Identity is not verified (trusted hop — see
    // `tls::build_connector`). A plaintext port forwards the raw socket.
    let server: UpstreamStream = match upstream_connector {
        Some(connector) => {
            let sni = match ServerName::try_from(upstream_host.to_string()) {
                Ok(name) => name,
                Err(e) => {
                    sink_ecam::record(
                        Severity::Warn,
                        Origin::Network(NetworkOrigin::ServerToFilter(upstream_port)),
                        format!(
                            "[conn_id={conn_id:04}] invalid upstream SNI '{upstream_host}': {e}; dropping client {client_addr}"
                        ),
                        None,
                    );
                    return Ok(());
                }
            };
            match connector.connect(sni, tcp).await {
                Ok(tls) => UpstreamStream::Tls(tls),
                Err(e) => {
                    sink_ecam::record(
                        Severity::Warn,
                        Origin::Network(NetworkOrigin::ServerToFilter(upstream_port)),
                        format!(
                            "[conn_id={:04}] upstream TLS handshake failed: public port {} → service {}; \
                             dropping client {}. error kind=\"{}\", detail=\"{}\"",
                            conn_id,
                            public_port,
                            upstream_addr,
                            client_addr,
                            e.kind(),
                            e
                        ),
                        None,
                    );
                    return Ok(());
                }
            }
        }
        None => UpstreamStream::Plain(tcp),
    };

    let (mut cr, mut cw) = tokio::io::split(client);
    let (mut sr, mut sw) = tokio::io::split(server);

    let conn_start_ns = now_ns();

    // Shared per-connection HTTP/2 flag: the inbound pump sets it when it sees the
    // h2 client preface, and both pumps read it to tag (and pass through) h2 frames.
    let conn_is_h2 = Arc::new(AtomicBool::new(false));

    // Per-connection token: when either direction ends, it cancels this token so
    // the sibling pump tears its half down too (half-close → full close). It is a
    // child of the listener token, so a global shutdown still propagates to both.
    let conn_token = cancel.child_token();

    // Per-connection activity clock. The pumps bump it on every read; the optional
    // idle/lifetime guards below cancel `conn_token` (which both pumps already
    // honour) when a limit is crossed, tearing the connection down.
    let last_activity = Arc::new(AtomicU64::new(now_ns()));

    if lifetime_secs > 0 {
        let tok = conn_token.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = tok.cancelled() => {}
                _ = tokio::time::sleep(Duration::from_secs(lifetime_secs)) => {
                    sink_ecam::record(
                        Severity::Warn,
                        Origin::Network(ClientToFilter(public_port)),
                        format!("[conn_id={conn_id:04}] closing connection: exceeded max lifetime ({lifetime_secs}s)"),
                        None,
                    );
                    tok.cancel();
                }
            }
        });
    }

    if idle_timeout_secs > 0 {
        let tok = conn_token.clone();
        let last = last_activity.clone();
        let idle_ns = idle_timeout_secs.saturating_mul(1_000_000_000);
        // Poll a few times per window (between 1s and 30s) so a crossed deadline is
        // acted on promptly without a busy loop.
        let step = Duration::from_secs(idle_timeout_secs.clamp(1, 30));
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tok.cancelled() => break,
                    _ = tokio::time::sleep(step) => {
                        if now_ns().saturating_sub(last.load(Ordering::Relaxed)) >= idle_ns {
                            sink_ecam::record(
                                Severity::Warn,
                                Origin::Network(ClientToFilter(public_port)),
                                format!("[conn_id={conn_id:04}] closing connection: idle for {idle_timeout_secs}s"),
                                None,
                            );
                            tok.cancel();
                            break;
                        }
                    }
                }
            }
        });
    }

    let in_task = pump(
        conn_id,
        true, /* Inbound, as in, coming from client -> filter -> server */
        &mut cr,
        &mut sw,
        client_addr, /* src: client */
        server_addr, /* dst: upstream service */
        public_port,
        upstream_port,
        body_limit,
        idle_flush_ms,
        port_rt.clone(),
        conn_start_ns,
        conn_is_h2.clone(),
        last_activity.clone(),
        conn_token.clone(),
    );
    let out_task = pump(
        conn_id,
        false, /* Outbound, as in, going to client <- filter <- server */
        &mut sr,
        &mut cw,
        server_addr, /* src: upstream service */
        client_addr, /* dst: client */
        public_port,
        upstream_port,
        body_limit,
        idle_flush_ms,
        port_rt,
        conn_start_ns,
        conn_is_h2,
        last_activity,
        conn_token.clone(),
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
/// * `dst_addr`: Address of entity receiving data (the upstream service on inbound, the client on outbound)
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
    dst_addr: SocketAddr,
    public_port: u16,
    upstream_port: u16,
    body_limit: usize,
    idle_flush_ms: u64,
    port_rt: Arc<PortRuntime>,
    conn_start_ns: u64,
    conn_is_h2: Arc<AtomicBool>,
    last_activity: Arc<AtomicU64>,
    cancel: CancellationToken,
) -> Result<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut seq = 0u64;
    let mut buf = BytesMut::with_capacity(8192);

    // Threshold at which a non-completing / non-HTTP buffer is flushed as a raw
    // stream chunk. The body limit governs how much *body* we capture, but a full
    // HTTP message is headers + body — so the flush threshold sits a small headroom
    // ABOVE the body limit (and never below 8 KiB), letting a message whose body is
    // right at the limit still complete and parse as HTTP instead of tipping into a
    // raw flush. NOTE on chunked Transfer-Encoding: chunked bodies are not
    // reassembled into an HttpMessage — they pass through as raw-stream chunks once
    // this threshold is reached, so HTTP-scoped rules do not apply.
    let raw_flush_threshold = body_limit.max(8192) + RAW_FLUSH_HEADROOM;

    // helper closure --------------------------------------------------------
    let mut last_msg_ns = conn_start_ns;

    let mut process_chunk = async |raw: &[u8],
                                   http_opt: Option<&crate::http::HttpMessage>,
                                   protocol: shared::types::base_types::Protocol|
           -> Result<bool> {
        let conn_now_ns = now_ns();

        // ---- build ctx & evaluate ----------------------------------------
        let src_ip = src_addr.ip().to_string();
        let dst_ip = dst_addr.ip().to_string();
        let msg_ctx = MessageCtx {
            conn_id,
            seq,
            inbound,
            public_port,
            upstream_port,
            raw,
            raw_capture_limit: body_limit,
            http: http_opt,
            protocol,
            src_ip: &src_ip,
            src_port: src_addr.port(),
            dst_ip: &dst_ip,
            dst_port: dst_addr.port(),
            conn_start_ns,
            conn_prev_msg_ns: last_msg_ns,
            msg_now_ns: conn_now_ns,
        };

        last_msg_ns = conn_now_ns;

        let compiled = port_rt.compiled.load();
        let (decision, rec) = evaluate(&compiled, &msg_ctx);

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
            // A write error means the receiving side is gone; treat it as a
            // normal end-of-connection (stop), not a fatal error, so we can
            // still tear the other half down gracefully.
            if let Err(e) = writer.write_all(raw).await {
                tracing::trace!(
                    "[conn_id={conn_id:04}] forward write failed ({}); stopping pump",
                    e.kind()
                );
                return Ok(false);
            }
            Ok(true)
        } else {
            Ok(false) // signal to stop
        }
    };

    // Read-idle flush window for unframed traffic. `None` (idle_flush_ms == 0)
    // restores the legacy "buffer until a complete HTTP message / body limit / EOF"
    // behavior. See `SINKHOLE_FILTER_IDLE_FLUSH_MS`.
    let idle = (idle_flush_ms > 0).then(|| Duration::from_millis(idle_flush_ms));

    // True once we've parsed a committed HTTP message head whose body is still
    // arriving. While set, we must NOT idle-flush (it would split a real HTTP
    // message); only the body limit can force a raw flush.
    let mut awaiting_http_body = false;

    // main read loop --------------------------------------------------------
    loop {
        // Arm the idle window only when we hold un-forwarded bytes that aren't a
        // committed HTTP body: if the sender then pauses (waiting for a reply stuck
        // in our buffer), flush what we have as a raw chunk instead of deadlocking
        // until EOF/body-limit. Outer = Result<read, Elapsed>.
        let arm_idle = idle.is_some() && !buf.is_empty() && !awaiting_http_body;
        let read_res = match idle {
            Some(d) if arm_idle => tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::trace!("[conn_id={:04}] shutdown/teardown requested, closing connection", conn_id);
                    break;
                }
                r = tokio::time::timeout(d, reader.read_buf(&mut buf)) => r,
            },
            _ => tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::trace!("[conn_id={:04}] shutdown/teardown requested, closing connection", conn_id);
                    break;
                }
                r = reader.read_buf(&mut buf) => Ok(r),
            },
        };

        let read_n = match read_res {
            // Idle window elapsed with bytes pending → flush them as a raw chunk.
            Err(_elapsed) => {
                let raw = buf.split().freeze();
                let proto = raw_protocol(conn_is_h2.load(Ordering::Relaxed), &raw);
                if !process_chunk(&raw, None, proto).await? {
                    break;
                }
                continue;
            }
            Ok(Ok(n)) => n,
            // A peer reset/abort (ECONNRESET, broken pipe, unexpected TLS EOF) is a
            // normal way for a connection to end here — fall through to graceful
            // teardown instead of surfacing it as a hard error.
            Ok(Err(e)) => {
                tracing::trace!(
                    "[conn_id={:04}] read ended ({}); tearing down",
                    conn_id,
                    e.kind()
                );
                break;
            }
        };
        if read_n == 0 {
            if inbound {
                tracing::trace!("[conn_id={:04}] connection closed by client", conn_id);
            } else {
                tracing::trace!("[conn_id={:04}] connection closed by service", conn_id);
            }
            break; // EOF
        }

        // Record activity for the per-connection idle guard (cheap, lock-free).
        last_activity.store(now_ns(), Ordering::Relaxed);

        if inbound {
            tracing::trace!("[conn_id={:04}] read {} bytes from client", conn_id, read_n);
        } else {
            tracing::trace!(
                "[conn_id={:04}] read {} bytes from service",
                conn_id,
                read_n
            );
        }

        // HTTP/2: detected by the client connection preface. We don't parse h2
        // frames (HPACK is stateful — out of scope), so a recognized h2 connection
        // is tagged and passed through as raw chunks in both directions.
        if inbound && !conn_is_h2.load(Ordering::Relaxed) && buf.starts_with(H2_PREFACE) {
            conn_is_h2.store(true, Ordering::Relaxed);
        }
        if conn_is_h2.load(Ordering::Relaxed) {
            awaiting_http_body = false;
            let raw = buf.split().freeze();
            if !process_chunk(&raw, None, Protocol::Http2).await? {
                break;
            }
            continue;
        }

        match parse_http(&buf, inbound, body_limit) {
            // Complete HTTP/1.x message → forward framed, tagged by its version.
            HttpParse::Complete(parsed, used) => {
                awaiting_http_body = false;
                let proto = http_protocol(parsed.msg.version);
                let raw = buf.split_to(used).freeze();
                if !process_chunk(&raw, Some(&parsed.msg), proto).await? {
                    break;
                }
                continue;
            }
            // Committed HTTP message, body still arriving → keep buffering (suppress
            // idle-flush next iteration). Only the body limit forces a raw flush.
            HttpParse::AwaitingBody => {
                awaiting_http_body = true;
                if buf.len() > raw_flush_threshold {
                    awaiting_http_body = false;
                    let raw = buf.split().freeze();
                    let proto = raw_protocol(conn_is_h2.load(Ordering::Relaxed), &raw);
                    if !process_chunk(&raw, None, proto).await? {
                        break;
                    }
                }
                continue;
            }
            // Not (yet) HTTP: a partial head, or unframed/raw traffic. Don't flush
            // eagerly — an unframed "message" is often several back-to-back writes
            // (a banner line, then a menu block), so we let the idle window coalesce
            // them into one chunk instead of forwarding each read as its own
            // message. The body limit still caps the buffer; with the idle window
            // disabled (`IDLE_FLUSH_MS=0`) this is the legacy buffer-until-cap/EOF
            // path.
            HttpParse::Incomplete => {
                awaiting_http_body = false;
                if buf.len() > raw_flush_threshold {
                    let raw = buf.split().freeze();
                    let proto = raw_protocol(conn_is_h2.load(Ordering::Relaxed), &raw);
                    if !process_chunk(&raw, None, proto).await? {
                        break;
                    }
                }
            }
        }
    }

    // leftover bytes after EOF (best effort — the peer may already be gone)
    if !buf.is_empty() {
        let raw = buf.split().freeze();
        let proto = raw_protocol(conn_is_h2.load(Ordering::Relaxed), &raw);
        let _ = process_chunk(&raw, None, proto).await;
    }

    // Gracefully close the write half: for a TLS stream this sends `close_notify`,
    // so strict clients (browsers, the game bot) see a clean end rather than a
    // truncated stream. Errors are ignored — the peer may have closed already.
    let _ = writer.shutdown().await;

    // Signal the sibling pump (other direction) to tear its half down too, so the
    // whole connection closes promptly instead of half-open.
    cancel.cancel();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PortBinding;
    use crate::registry::PortRuntime;
    use crate::rules::compile::compile_ruleset;
    use arc_swap::{ArcSwap, ArcSwapOption};
    use parking_lot::RwLock;
    use shared::types::base_types::Scope;
    use shared::types::ruleset_types::RuleSetRaw;
    use std::collections::HashSet;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    #[test]
    fn accept_errno_classification() {
        // Resource-exhaustion / peer-caused → transient (keep the listener up).
        for errno in [
            24,  /*EMFILE*/
            23,  /*ENFILE*/
            105, /*ENOBUFS*/
            71,  /*EPROTO*/
        ] {
            assert!(
                accept_error_is_transient(&io::Error::from_raw_os_error(errno)),
                "errno {errno} should be transient"
            );
        }
        // ErrorKind-mapped transients.
        for kind in [
            io::ErrorKind::ConnectionAborted,
            io::ErrorKind::Interrupted,
            io::ErrorKind::OutOfMemory,
        ] {
            assert!(accept_error_is_transient(&io::Error::from(kind)));
        }
        // A broken listening socket → fatal (stop the loop).
        assert!(!accept_error_is_transient(&io::Error::from_raw_os_error(
            9 /*EBADF*/
        )));
        assert!(!accept_error_is_transient(&io::Error::from(
            io::ErrorKind::InvalidInput
        )));
    }

    // An empty ruleset compiles to "allow everything", so the pump's forwarding
    // behavior (not the engine) is what's under test.
    fn allow_all_runtime() -> Arc<PortRuntime> {
        let raw = Arc::new(RuleSetRaw {
            commit_message: None,
            markers: vec![],
            filters: vec![],
        });
        let compiled = Arc::new(compile_ruleset(&raw).unwrap());
        let scopes: HashSet<Scope> = [Scope::Stream].into_iter().collect();
        Arc::new(PortRuntime {
            binding: PortBinding {
                public: 1234,
                upstream: 80,
                upstream_host: None,
                tls: None,
            },
            enabled_scopes: RwLock::new(scopes),
            raw_ruleset: ArcSwap::from(raw),
            compiled: ArcSwap::from(compiled),
            name: ArcSwapOption::from(None),
            comment: ArcSwapOption::from(None),
            hex_color: ArcSwapOption::from(None),
        })
    }

    /// Spawn a pump reading from `pump_rx` and writing to `pump_tx`, with a 50ms
    /// idle window. Returns the join handle.
    fn spawn_pump(
        pump_rx: tokio::io::DuplexStream,
        pump_tx: tokio::io::DuplexStream,
    ) -> tokio::task::JoinHandle<()> {
        let rt = allow_all_runtime();
        let cancel = CancellationToken::new();
        let addr: SocketAddr = "127.0.0.1:1111".parse().unwrap();
        let mut pump_rx = pump_rx;
        let mut pump_tx = pump_tx;
        let is_h2 = Arc::new(AtomicBool::new(false));
        let last_activity = Arc::new(AtomicU64::new(now_ns()));
        tokio::spawn(async move {
            let _ = pump(
                1,
                true,
                &mut pump_rx,
                &mut pump_tx,
                addr,
                addr,
                1234,
                80,
                1_048_576,
                50,
                rt,
                now_ns(),
                is_h2,
                last_activity,
                cancel,
            )
            .await;
        })
    }

    // Regression: a small non-HTTP message must be forwarded promptly — not held
    // until EOF or the body limit (the interactive-deadlock bug). It rides the idle
    // window (~50ms here), well within the test's read timeout.
    #[tokio::test]
    async fn forwards_small_non_http_without_eof() {
        let (mut client_tx, pump_rx) = duplex(64 * 1024);
        let (pump_tx, mut out_rx) = duplex(64 * 1024);
        let h = spawn_pump(pump_rx, pump_tx);

        client_tx.write_all(b"ping\n").await.unwrap();
        let mut got = [0u8; 5];
        tokio::time::timeout(Duration::from_secs(2), out_rx.read_exact(&mut got))
            .await
            .expect("forwarded before EOF")
            .unwrap();
        assert_eq!(&got, b"ping\n");

        drop(client_tx);
        let _ = h.await;
    }

    // An uppercase-first non-HTTP message looks like a partial HTTP request, so it
    // takes the idle-flush escape rather than the fast path — still forwarded
    // (within the idle window), never deadlocked.
    #[tokio::test]
    async fn idle_flush_forwards_uppercase_non_http() {
        let (mut client_tx, pump_rx) = duplex(64 * 1024);
        let (pump_tx, mut out_rx) = duplex(64 * 1024);
        let h = spawn_pump(pump_rx, pump_tx);

        client_tx.write_all(b"COMMAND foo\n").await.unwrap();
        let mut got = [0u8; 12];
        tokio::time::timeout(Duration::from_secs(2), out_rx.read_exact(&mut got))
            .await
            .expect("idle-flushed before EOF")
            .unwrap();
        assert_eq!(&got, b"COMMAND foo\n");

        drop(client_tx);
        let _ = h.await;
    }

    // A complete HTTP request is still forwarded byte-exact (framed path unchanged).
    #[tokio::test]
    async fn forwards_complete_http_request() {
        let (mut client_tx, pump_rx) = duplex(64 * 1024);
        let (pump_tx, mut out_rx) = duplex(64 * 1024);
        let h = spawn_pump(pump_rx, pump_tx);

        let req = b"GET /hello HTTP/1.1\r\nHost: x\r\n\r\n";
        client_tx.write_all(req).await.unwrap();
        let mut got = vec![0u8; req.len()];
        tokio::time::timeout(Duration::from_secs(2), out_rx.read_exact(&mut got))
            .await
            .expect("http forwarded")
            .unwrap();
        assert_eq!(&got, req);

        drop(client_tx);
        let _ = h.await;
    }

    // A silent upstream + silent client: nothing speaks, so only the connection
    // guards can end it. Used by the idle/lifetime tests below.
    async fn silent_upstream_port() -> u16 {
        let up = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = up.local_addr().unwrap().port();
        tokio::spawn(async move {
            if let Ok((s, _)) = up.accept().await {
                tokio::time::sleep(Duration::from_secs(30)).await;
                drop(s);
            }
        });
        port
    }

    // A connection idle past CONN_IDLE_TIMEOUT_SECS is torn down.
    #[tokio::test]
    async fn idle_timeout_closes_connection() {
        let up_port = silent_upstream_port().await;
        // Client stays connected but sends nothing; `_hold` keeps the other end
        // open so the pump blocks on read rather than seeing EOF.
        let (client, _hold) = duplex(1024);
        let caddr: SocketAddr = "127.0.0.1:2222".parse().unwrap();

        let start = std::time::Instant::now();
        let res = tokio::time::timeout(
            Duration::from_secs(6),
            handle_conn(
                1,
                client,
                caddr,
                "127.0.0.1",
                up_port,
                1234,
                1_048_576,
                50,
                1, /* idle_timeout_secs */
                0, /* lifetime_secs */
                allow_all_runtime(),
                None,
                CancellationToken::new(),
            ),
        )
        .await;
        assert!(
            res.is_ok(),
            "handle_conn should return once the idle guard fires"
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "idle teardown too slow"
        );
    }

    // A connection open past CONN_MAX_LIFETIME_SECS is torn down.
    #[tokio::test]
    async fn lifetime_closes_connection() {
        let up_port = silent_upstream_port().await;
        let (client, _hold) = duplex(1024);
        let caddr: SocketAddr = "127.0.0.1:3333".parse().unwrap();

        let start = std::time::Instant::now();
        let res = tokio::time::timeout(
            Duration::from_secs(6),
            handle_conn(
                1,
                client,
                caddr,
                "127.0.0.1",
                up_port,
                1234,
                1_048_576,
                50,
                0, /* idle_timeout_secs (off) */
                1, /* lifetime_secs */
                allow_all_runtime(),
                None,
                CancellationToken::new(),
            ),
        )
        .await;
        assert!(
            res.is_ok(),
            "handle_conn should return once the lifetime guard fires"
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "lifetime teardown too slow"
        );
    }
}
