use serde::{Deserialize, Serialize};

pub type RuleId = String;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum Scope {
    Meta,
    Stream,
    HttpHeader,
    HttpPath,
    HttpBody,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Decision {
    Allow,
    Drop,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum MatchStatus {
    Satisfied,
    NotSatisfied,
    Skipped,
    Error,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Mode { Blacklist, WhitelistAll, WhitelistAny }

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Direction { Inbound, Outbound }

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
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