CREATE TABLE IF NOT EXISTS ecam (
ts_ns       INTEGER      NOT NULL,
service_id  TEXT         NOT NULL,
json        TEXT         NOT NULL,
PRIMARY KEY (ts_ns, service_id)
);

CREATE INDEX IF NOT EXISTS idx_ecam_service_ts ON ecam (service_id, ts_ns);

CREATE TABLE IF NOT EXISTS records (
timestamp_ns INTEGER      NOT NULL,
service_id   TEXT         NOT NULL,
port         INTEGER      NOT NULL,
json         TEXT         NOT NULL,
PRIMARY KEY (timestamp_ns, service_id, port)
);

CREATE INDEX IF NOT EXISTS idx_records_service_port_ts ON records (service_id, port, timestamp_ns);