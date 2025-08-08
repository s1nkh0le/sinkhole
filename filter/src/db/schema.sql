PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS records
(
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp_ns INTEGER NOT NULL,
    port         INTEGER NOT NULL,                      /* external */
    inbound      INTEGER NOT NULL,
    decision     TEXT    NOT NULL,
    json         TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_records_port_ts ON records (port, timestamp_ns);
CREATE INDEX IF NOT EXISTS idx_records_port_id ON records (port, id);

CREATE TABLE IF NOT EXISTS port_rulesets
(
    port          INTEGER PRIMARY KEY,
    version       TEXT    NOT NULL,
    json          TEXT    NOT NULL,
    updated_at_ns INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS port_configs
(
    external_port       INTEGER PRIMARY KEY,
    upstream_port       INTEGER NOT NULL,
    name                TEXT,
    comment             TEXT,
    hex_color           TEXT,
    enabled_scopes_json TEXT    NOT NULL,
    updated_at_ns       INTEGER NOT NULL
);