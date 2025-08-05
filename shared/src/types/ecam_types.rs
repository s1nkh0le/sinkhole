use serde::{Deserialize, Serialize};
use strum_macros::Display;
use utoipa::ToSchema;

/// Severity level of an ecam event
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Display, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warn,
    Error,
}

/// Describing how traffic reached the filter
#[derive(Clone, Debug, Serialize, Deserialize, Display, ToSchema)]
pub enum NetworkOrigin {
    Filter,
    ClientToFilter(u16),
    ServerToFilter(u16),
}

/// Origin of an event within the system
#[derive(Debug, Clone, Serialize, Deserialize, Display, ToSchema)]
pub enum Origin {
    Global,
    Configuration,
    Network(NetworkOrigin),
    Port(u16),
    Matcher { port: u16, filter_id: String },
    DB,
    API,
    Telemetry,
    Security,
}

/// A single ECAM system event
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EcamEvent {
    /// Timestamp of the event
    pub ts_ns: i64,
    pub severity: Severity,
    pub origin: Origin,
    /// Human-readable message describing what happened
    pub msg: String,
    /// Structured JSON data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
