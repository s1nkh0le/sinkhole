use serde::{Deserialize, Serialize};
use strum_macros::Display;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Hash, Ord, Display, PartialOrd, ToSchema)]
pub enum Scope {
    #[serde(rename = "meta")]
    Meta,
    #[serde(rename = "stream")]
    Stream,
    #[serde(rename = "http.header")]
    HttpHeader,
    #[serde(rename = "http.path")]
    HttpPath,
    #[serde(rename = "http.body")]
    HttpBody,
    #[serde(rename = "http.method")]
    HttpMethod,
    #[serde(rename = "http.status")]
    HttpStatus,
    #[serde(rename = "timing")]
    Timing,
}

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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum MatchStatus {
    Satisfied,
    NotSatisfied,
    Skipped,
    Error,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Blacklist,
    WhitelistAll,
    WhitelistAny,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub enum Direction {
    Inbound,
    Outbound,
}

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
