CREATE TABLE IF NOT EXISTS ecam (
    ts_ns       INTEGER      NOT NULL,
    service_id  TEXT         NOT NULL,
    json        TEXT         NOT NULL,
PRIMARY KEY (ts_ns, service_id)
);

CREATE INDEX IF NOT EXISTS idx_ecam_service_ts ON ecam (service_id, ts_ns);

CREATE TABLE IF NOT EXISTS records (
    timestamp_ns INTEGER NOT NULL,
    service_id   TEXT    NOT NULL,
    port         INTEGER NOT NULL,

    inbound      INTEGER NOT NULL,        -- 0/1
    decision     TEXT    NOT NULL,        -- 'allow' | 'drop'
    conn_id      INTEGER NOT NULL,

    http_method  TEXT,                    -- e.g. GET
    http_path    TEXT,                    -- e.g. /api/v1/...
    http_status  INTEGER,                 -- e.g. 200
    http_length  INTEGER,                 -- body_len

    json         TEXT    NOT NULL,        -- full record
PRIMARY KEY (timestamp_ns, service_id, port));

CREATE INDEX IF NOT EXISTS idx_records_service_port_ts
    ON records(service_id, port, timestamp_ns);

CREATE INDEX IF NOT EXISTS idx_records_service_port_inbound_ts
    ON records(service_id, port, inbound, timestamp_ns);

CREATE INDEX IF NOT EXISTS idx_records_service_port_decision_ts
    ON records(service_id, port, decision, timestamp_ns);

CREATE INDEX IF NOT EXISTS idx_records_service_port_status_ts
    ON records(service_id, port, http_status, timestamp_ns);

CREATE INDEX IF NOT EXISTS idx_records_service_port_method_ts
    ON records(service_id, port, http_method, timestamp_ns);


CREATE TABLE IF NOT EXISTS drop_reason_items (
    timestamp_ns INTEGER NOT NULL,
    service_id   TEXT    NOT NULL,
    port         INTEGER NOT NULL,

    reason_kind  TEXT    NOT NULL,  -- 'blacklist' | 'whitelist_all_miss' | 'whitelist_any_miss'
    item_type    TEXT    NOT NULL,  -- 'filter_id' | 'missing_id' | 'hit_id'
    value        TEXT    NOT NULL   -- rule id

    -- (no FK; we use (ts,svc,port) from the parent row)
);

CREATE INDEX IF NOT EXISTS idx_dri_svc_port_kind_value_ts
    ON drop_reason_items(service_id, port, reason_kind, value, timestamp_ns);