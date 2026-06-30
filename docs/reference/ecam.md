# ECAM events

ECAM is sinkhole's structured operational event log: the things worth an operator's attention, such as configuration errors, network failures, security warnings, and database problems. It is separate from two other streams:

- evaluation records, the per-message allow and drop decisions (see [Evaluation semantics](../filtering/evaluation.md));
- ordinary log lines, the free-form developer output on stdout.

## Lifecycle

A filter writes events into a small in-memory buffer (capped, with the oldest dropped when it fills). The controller pulls each filter's events on the `SYNC_ECAM_INTERVAL_MS` cadence, using a per-filter cursor, and stores them centrally so the UI can show one event feed for the whole fleet. See [Data and persistence](../architecture/persistence.md#centralization-controller-pull).

## Event shape

```json
{
  "ts_ns":    1754004005155452000,
  "severity": "warn",
  "origin":   { "Network": { "ClientToFilter": 8080 } },
  "msg":      "TLS handshake failed on port 8080",
  "data":     null
}
```

| Field | Type | Meaning |
|---|---|---|
| `ts_ns` | u64 | Event time, in nanoseconds. |
| `severity` | `"warn"` or `"error"` | How serious it is. |
| `origin` | enum | Which part of the system raised it (see below). |
| `msg` | string | Human-readable description. |
| `data` | JSON or absent | Optional structured payload; omitted when there is none. |

## Severity

There are two levels, because ECAM is for the noteworthy, not for routine debugging:

- `warn`: degraded but recoverable, such as an upstream that was briefly unreachable or a dropped TLS handshake.
- `error`: a real fault, such as a misconfiguration, a port that failed to bind, a database write failure, or a security problem.

## Origins

`origin` says which part of the system raised the event.

| Origin | Raised by |
|---|---|
| `Global` | process-wide events |
| `Configuration` | environment and config validation problems |
| `Network` | accept, connect, and TLS issues (carries a per-port detail) |
| `Port` | a specific public port |
| `Matcher` | a specific rule on a port (carries the port and rule id) |
| `DB` | local database problems |
| `API` | control-API handling |
| `Telemetry` | the record pipeline |
| `Security` | authentication and authorization events |

## Reading events

- From one filter directly: `GET /ecam`, paging with the `since` cursor.
- Aggregated through the controller: `GET /ecam`, or `GET /services/{service}/ecam` for a single filter.

Events are ordered oldest first, and each response carries a `next_since` cursor to page forward. See the [API reference](../pages/reference/swagger/index.md).
