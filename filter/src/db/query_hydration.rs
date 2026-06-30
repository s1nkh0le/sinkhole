use crate::db::schema::init_schema;
use crate::rules::RuleSetRaw;
use rusqlite::{Connection, params};
use shared::types::base_types::Scope;
use shared::types::routing::filter::ruleset::RulesetHistoryEntry;
use std::sync::Arc;

pub struct HydratedPortData {
    pub ruleset: Option<(RuleSetRaw, String)>,
    pub config: Option<HydratedConfig>,
}

pub struct HydratedConfig {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub hex_color: Option<String>,
    pub enabled_scopes: Vec<Scope>,
}

pub fn hydrate_registry(
    db_path: &str,
    registry: &crate::registry::PortRegistry,
) -> anyhow::Result<()> {
    let ports: Vec<u16> = registry.iter().map(|e| e.binding.public).collect();
    let data = hydrate_for_ports(db_path, &ports)?;

    for (port, h) in data {
        if let Some(pr) = registry.get(&port) {
            if let Some(cfgp) = h.config {
                if let Some(n) = cfgp.name {
                    pr.name.store(Some(Arc::new(n)));
                }
                if let Some(c) = cfgp.comment {
                    pr.comment.store(Some(Arc::new(c)));
                }
                if let Some(hc) = cfgp.hex_color {
                    pr.hex_color.store(Some(Arc::new(hc)));
                }
                let mut g = pr.enabled_scopes.write();
                g.clear();
                g.extend(cfgp.enabled_scopes);
            }
            if let Some((raw, _ver)) = h.ruleset {
                let compiled = crate::rules::compile::compile_ruleset(&raw)?;
                pr.raw_ruleset.store(Arc::new(raw));
                pr.compiled.store(Arc::new(compiled));
            }
        }
    }

    Ok(())
}

pub fn hydrate_for_ports(
    path: &str,
    ports: &[u16],
) -> anyhow::Result<Vec<(u16, HydratedPortData)>> {
    let mut conn = Connection::open(path)?;
    // ensure tables exist (idempotent)
    init_schema(&conn)?;

    let mut out = Vec::new();
    for p in ports {
        let ruleset = load_ruleset(&mut conn, *p)?;
        let config = load_config(&mut conn, *p)?;
        out.push((*p, HydratedPortData { ruleset, config }));
    }
    Ok(out)
}

/// Load the full publish history for a port, newest first. Each row's `json` is
/// a serialized `RuleSetRaw`; the commit message rides inside it. Unparseable
/// rows are skipped rather than failing the whole listing.
pub fn load_ruleset_history(
    conn: &Connection,
    port: u16,
) -> anyhow::Result<Vec<RulesetHistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT version, json, updated_at_ns FROM port_rulesets
         WHERE external_port = ? ORDER BY id DESC",
    )?;
    let rows = stmt.query_map(params![port as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;

    let mut out = Vec::new();
    for r in rows {
        let (version, json, updated_at_ns) = r?;
        match serde_json::from_str::<RuleSetRaw>(&json) {
            Ok(ruleset) => out.push(RulesetHistoryEntry {
                version,
                updated_at_ns,
                ruleset,
            }),
            Err(e) => tracing::warn!(
                port,
                version,
                "skipping unparseable ruleset history row: {e}"
            ),
        }
    }
    Ok(out)
}

fn load_ruleset(conn: &mut Connection, port: u16) -> anyhow::Result<Option<(RuleSetRaw, String)>> {
    let mut stmt = conn.prepare(
        "SELECT version, json FROM port_rulesets WHERE external_port = ? ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![port as i64])?;
    if let Some(row) = rows.next()? {
        let version: String = row.get(0)?;
        let json_str: String = row.get(1)?;
        let raw: RuleSetRaw = serde_json::from_str(&json_str)?;
        Ok(Some((raw, version)))
    } else {
        Ok(None)
    }
}

fn load_config(conn: &mut Connection, port: u16) -> anyhow::Result<Option<HydratedConfig>> {
    let mut stmt = conn.prepare(
        "SELECT name, comment, hex_color, enabled_scopes_json
         FROM port_configs WHERE external_port = ? ORDER BY id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query(params![port as i64])?;
    if let Some(row) = rows.next()? {
        let name: Option<String> = row.get(0)?;
        let comment: Option<String> = row.get(1)?;
        let hex_color: Option<String> = row.get(2)?;
        let scopes_json: String = row.get(3)?;
        let scopes: Vec<Scope> = serde_json::from_str(&scopes_json).unwrap_or_default();

        Ok(Some(HydratedConfig {
            name,
            comment,
            hex_color,
            enabled_scopes: scopes,
        }))
    } else {
        Ok(None)
    }
}
