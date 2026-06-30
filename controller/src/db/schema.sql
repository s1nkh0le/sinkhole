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
    message_seq  INTEGER NOT NULL,

    http_method  TEXT,                    -- e.g. GET
    http_path    TEXT,                    -- e.g. /api/v1/...
    http_status  INTEGER,                 -- e.g. 200
    http_length  INTEGER,                 -- body_len
    protocol     TEXT,                    -- 'http/1.1' | 'http/2' | 'raw-ascii' | 'raw-binary'

    json         TEXT    NOT NULL,        -- full record
-- Identity is the record's origin key. `conn_id` is monotonic per filter (see
-- the filter's conn-id allocator), so it never repeats within a service — re-pulls
-- of the same record collide here and are ignored (idempotent), while genuinely
-- distinct records never collide.
PRIMARY KEY (service_id, port, conn_id, inbound, message_seq));

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

CREATE INDEX IF NOT EXISTS idx_records_service_port_protocol_ts
    ON records(service_id, port, protocol, timestamp_ns);




-- Each (record, marker_id) pair once
CREATE TABLE IF NOT EXISTS record_markers (
timestamp_ns INTEGER NOT NULL,
service_id   TEXT    NOT NULL,
port         INTEGER NOT NULL,
conn_id      INTEGER NOT NULL,
inbound      INTEGER NOT NULL,
message_seq  INTEGER NOT NULL,
marker_id    TEXT    NOT NULL,
PRIMARY KEY (service_id, port, conn_id, inbound, message_seq, marker_id)
);

CREATE INDEX IF NOT EXISTS idx_record_markers_marker_sid_port_ts
    ON record_markers (marker_id, service_id, port, timestamp_ns);

CREATE INDEX IF NOT EXISTS idx_record_markers_sid_port_ts
    ON record_markers (service_id, port, timestamp_ns);




-- Flattened, indexed view of each dropped record's drop reasons — one row per
-- (record, reason_kind, item_type, value) — so the traffic analyzer can answer
-- "everything dropped by filter X / by blacklist" with an index instead of
-- scanning JSON. The full record still lives in `records.json`; this is purely a
-- query accelerator for high-volume analytics.
CREATE TABLE IF NOT EXISTS record_drop_reasons (
    timestamp_ns INTEGER NOT NULL,
    service_id   TEXT    NOT NULL,
    port         INTEGER NOT NULL,
    conn_id      INTEGER NOT NULL,
    inbound      INTEGER NOT NULL,
    message_seq  INTEGER NOT NULL,

    reason_kind  TEXT    NOT NULL,  -- 'blacklist' | 'whitelist_all_miss' | 'whitelist_any_miss'
    item_type    TEXT    NOT NULL,  -- 'filter_id' | 'missing_id' | 'hit_id'
    value        TEXT    NOT NULL,  -- rule id

    PRIMARY KEY (service_id, port, conn_id, inbound, message_seq, reason_kind, item_type, value)
);

CREATE INDEX IF NOT EXISTS idx_rdr_svc_port_kind_value_ts
    ON record_drop_reasons(service_id, port, reason_kind, value, timestamp_ns);



-- The tracked fleet: connection config + display metadata in one self-standing
-- table, so the controller can rehydrate its services (and everything set on
-- them) on restart. Gated by SINKHOLE_CONTROLLER_DB_HYDRATE. Retires the old
-- `service_metadata` table.
DROP TABLE IF EXISTS service_metadata;

CREATE TABLE IF NOT EXISTS services (
    id          TEXT PRIMARY KEY,
    base_url    TEXT    NOT NULL,
    token       TEXT    NOT NULL,
    name        TEXT,
    description TEXT,
    hex_color   TEXT,
    icon        TEXT,
    added_at_ns INTEGER NOT NULL
);
