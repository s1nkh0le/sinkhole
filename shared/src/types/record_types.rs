use crate::types::base_types::{Decision, MatchStatus, Protocol, Scope};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct HttpInfo {
    pub method: Option<String>,
    pub path: Option<String>,
    pub status: Option<u16>,
    pub reason: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body_len: usize,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind")]
pub enum DropReason {
    #[serde(rename = "blacklist")]
    Blacklist { filter_ids: Vec<String> },
    #[serde(rename = "whitelist_all_miss")]
    WhitelistAllMiss {
        required: usize,
        hit: usize,
        missing_ids: Vec<String>,
    },
    #[serde(rename = "whitelist_any_miss")]
    WhitelistAnyMiss {
        group_size: usize,
        hit_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct MatchOutput {
    pub scope: Scope,
    pub target: String,
    pub condition: String,
    pub input: String,
    pub matched: Option<String>,
    pub numeric: Option<f64>,
    pub status: MatchStatus,
    pub comment: Option<String>,
    pub comparator: String,
    pub flags: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct EvaluationDetail {
    pub kind: String, // "marker" | "filter"
    pub id: String,
    pub name: Option<String>,
    pub mode: Option<String>,
    pub gated: Option<bool>,
    pub decision_effect: Option<String>,
    pub match_output: MatchOutput,
}

// per‑message
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct StatsSnapshot {
    pub blacklist_hits: u64,
    pub whitelist_all_total: u64,
    pub whitelist_all_hit: u64,
    pub whitelist_any_total: u64,
    pub whitelist_any_hit: u64,
}

/// Complete record of an evaluation decision for a single message
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct EvaluationRecord {
    pub timestamp_ns: u64,
    pub conn_id: u64,
    pub message_seq: u64,
    pub inbound: bool,
    pub public_port: u16,
    pub upstream_port: u16,
    /// Wire protocol detected for this message (HTTP/1.x parsed, HTTP/2 tagged
    /// but not parsed, raw ascii/binary for unframed traffic).
    pub protocol: Protocol,
    /// IDs of the markers that were satisfied during evaluation.
    pub markers: Vec<String>,
    /// Final decision for this message: `Allow` or `Drop`.
    pub decision: Decision,
    pub drop_reasons: Vec<DropReason>,
    pub http: Option<HttpInfo>,
    // Captured frame, base64-encoded for JSON transport.
    pub raw_b64: String,
    pub details: Vec<EvaluationDetail>,
    // meta
    pub src_ip: String,
    pub src_port: u16,
    pub dst_ip: String,
    pub dst_port: u16,
}
