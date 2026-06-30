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

/* Small key/value store for filter-local counters. Never pruned. Currently holds
   `conn_id_ceiling`: the high-water mark for the monotonic connection-id allocator
   (hi/lo block reservation), so conn_ids never repeat across filter restarts even
   though the records table is pruned aggressively. */
CREATE TABLE IF NOT EXISTS meta
(
    key   TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);

/* Append-only history: one row per deployed ruleset/config. The latest row
   per port (highest id) is the active one; older rows are kept as an archive.
   Hydration reads the latest; `DELETE /db/clear` wipes the whole history. */
CREATE TABLE IF NOT EXISTS port_rulesets
(
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    external_port INTEGER NOT NULL,
    version       TEXT    NOT NULL,
    json          TEXT    NOT NULL,
    updated_at_ns INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_port_rulesets_port_id ON port_rulesets (external_port, id);

CREATE TABLE IF NOT EXISTS port_configs
(
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    external_port       INTEGER NOT NULL,
    upstream_port       INTEGER NOT NULL,
    name                TEXT,
    comment             TEXT,
    hex_color           TEXT,
    enabled_scopes_json TEXT    NOT NULL,
    updated_at_ns       INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_port_configs_port_id ON port_configs (external_port, id);