use serde::{Deserialize, Serialize};

pub type RuleId = String;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    Inbound,
    Outbound,
}

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

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum MatchStatus {
    Satisfied,
    NotSatisfied,
    Skipped,
    Error,
}
