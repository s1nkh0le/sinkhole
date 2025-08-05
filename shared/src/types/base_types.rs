use serde::{Deserialize, Serialize};
use strum_macros::Display;
use utoipa::ToSchema;

/// The set of scopes in which matchers and filters can operate
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Ord, Display, PartialOrd, ToSchema)]
pub enum Scope {
    /// General metadata about the request/response; todo maybe less useful now
    #[serde(rename = "meta")]
    Meta,

    /// Raw byte stream of the request/response
    #[serde(rename = "stream")]
    Stream,

    /// HTTP header (names and values)
    #[serde(rename = "http.header")]
    HttpHeader,

    /// Request path (URL)
    #[serde(rename = "http.path")]
    HttpPath,

    /// Request or response body
    #[serde(rename = "http.body")]
    HttpBody,

    /// HTTP method
    #[serde(rename = "http.method")]
    HttpMethod,

    /// HTTP response status code
    #[serde(rename = "http.status")]
    HttpStatus,

    /// Timing information
    #[serde(rename = "timing")]
    Timing,
}

/// Decision that a filter or rule can make on matching traffic
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Drop,
}

impl Decision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Drop => "drop",
        }
    }
}

/// The result of evaluating a matcher
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MatchStatus {
    Satisfied,
    NotSatisfied,
    Skipped,
    Error,
}

/// Mode of operation for a filter rule
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Blacklist,
    WhitelistAll,
    WhitelistAny,
}

/// Network flow direction
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum Direction {
    Inbound,
    Outbound,
}

/// Declarative direction enumeration for filter and marker rules
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RuleDirection {
    Inbound,
    Outbound,
    Both,
}

impl RuleDirection {
    pub fn matches(&self, rt: Direction) -> bool {
        matches!(
            (self, rt),
            (RuleDirection::Both, _)
                | (RuleDirection::Inbound, Direction::Inbound)
                | (RuleDirection::Outbound, Direction::Outbound)
        )
    }
}
