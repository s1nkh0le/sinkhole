# Filter

A filter is a transparent TCP proxy that guards a single service. Clients connect to the filter instead of to the service. The filter opens its own connection to the service behind it, passes traffic through in both directions, and runs that service's rules on every message in between. Only allowed messages are passed on. A dropped message ends the connection.

## How traffic flows

A filter listens on one or more public ports, the ports the service used to expose directly. For each client that connects, it opens a matching connection to the service and then moves bytes both ways: client to service (inbound) and service to client (outbound).

In each direction it watches the stream, breaks it into messages, and checks each message against the port's rules before letting it continue. An allowed message is forwarded exactly as it arrived, byte for byte. A dropped message is not forwarded at all: the filter stops and closes the connection.

## What counts as a message

When the traffic is HTTP and the filter can read a complete request or response, that request or response is the message. When it cannot read HTTP, it works with raw chunks of bytes instead.

Raw traffic has no message boundaries, so the filter has to decide on its own when a chunk is complete. It treats a chunk as done when the sender pauses for a short, configurable window (`IDLE_FLUSH_MS`), when the buffered data grows past the body limit, or when the connection closes.

!!! note "Interactive and line-based traffic"
    Many non-HTTP services speak first or work line by line, for example a menu prompt or a netcat-style session. Waiting for the sender to go quiet lets the filter forward a prompt right away instead of waiting for the connection to close, and it groups the pieces of one logical message (a banner line followed by a menu) into a single record instead of many. `IDLE_FLUSH_MS` sets that quiet window, and `0` turns the behavior off. While an HTTP body is still arriving, the filter keeps waiting for the rest of it rather than cutting it into raw chunks.

## Protocol tagging

Every message is tagged with the protocol it appeared to use. HTTP/1.0 and HTTP/1.1 are tagged from the parsed message. HTTP/2 is recognized from the start of the connection but not parsed, so its frames are seen as raw bytes. Everything else is tagged raw, split into ascii and binary by sampling the start of the chunk and checking whether most of it is printable text. The controller stores the tag, so you can filter records by protocol later.

!!! warning "Chunked Transfer-Encoding"
    A chunked HTTP body is not reassembled into a complete message. It passes through as raw chunks, which means rules scoped to HTTP parts (the path, body, or headers) do not see it. Only stream-scoped rules do. This is a known limitation.

## TLS

A port can be marked as a TLS service. The filter completes the TLS handshake with the client using the service's own certificate and key, inspects the traffic in the clear, then re-encrypts it onto a fresh TLS connection to the service. Traffic stays encrypted on both sides; there is no mode that sends plaintext on to the service. See [TLS termination](../configuration/tls.md).

## What it captures versus what it forwards

Forwarding is always exact: the filter never changes what reaches the service or the client. Capture is separate. For inspection and for the record it writes, the filter keeps up to a configurable amount of each body (`BODY_LIMIT_BYTES`) and marks anything larger as truncated. This keeps a single large upload from bloating a record. The limit only affects what is captured and stored, never what is forwarded.

## The rule engine

Rules are evaluated in two phases.

1. **Markers** look at the message and record which conditions hold, without deciding anything.
2. **Filters** make the decision. A filter can require certain markers before it applies. A blacklist filter that matches drops the message immediately. Whitelist filters must be satisfied, either all of them or at least one depending on the mode, or the message is dropped. If nothing drops it, it is allowed.

Every message produces a record whatever the outcome, with per-rule detail and, for a drop, a structured reason. The full rules and their exact semantics are in [Filtering](../filtering/rules.md).

Each rule tests one part of a message with a matcher: a plain "always", a regular expression, a byte or hex signature, a size check, or a session-timing check. If the part a rule needs is not present, the rule is skipped rather than counted as a match.

## Live reload

Each port holds its current ruleset in memory in a way that lets the control API swap in a new one without pausing traffic or dropping connections. Snapshots are versioned by a content hash, so the controller can tell cheaply whether a port's rules or config have changed.

## What it records and reports

Two streams leave a filter.

- **Evaluation records**, one per message, are written to the local database. They are also printed to stdout, but only in debug mode, since one line per message is far too much for normal operation.
- **ECAM events** are the filter's operational log (configuration problems, network failures, security warnings). They are kept in a capped in-memory buffer and pulled by the controller. See [ECAM events](../reference/ecam.md).

## Storage

Records and each port's ruleset and config are written to a local SQLite database through a single writer. On boot the filter can restore its last ruleset and config from that database, so it comes back already enforcing its rules. The details, including batching and pruning, are in [Data and persistence](persistence.md).

## Control API

The filter exposes a small bearer-authenticated API (by default on `CONTROL_BIND=0.0.0.0:7001`) that the controller uses to read state, records, and events, and to push rulesets and config. It is reached only by the controller, never by a browser, so it has no CORS layer. The web UI talks only to the controller. See [Authentication](../configuration/auth.md). The endpoint list is in the [Filter API reference](../pages/reference/swagger/openapi-filter.md).
