use serde::{Deserialize, Serialize};

pub type RuleId = String;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
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
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Drop,
}

impl Decision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Drop  => "drop",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MatchStatus { Satisfied, NotSatisfied, Skipped, Error }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode { Blacklist, WhitelistAll, WhitelistAny }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Direction { Inbound, Outbound }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RuleDirection { Inbound, Outbound, Both }

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