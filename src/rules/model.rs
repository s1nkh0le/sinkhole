use crate::types::{Mode, RuleDirection, Scope};
use serde::{Serialize, Deserialize};

fn default_enabled() -> bool { true }

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MatcherDef {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub props: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarkerDef {
    pub id: String,
    pub name: Option<String>,
    pub direction: RuleDirection,
    pub scope: Scope,
    pub matcher: MatcherDef,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FilterDef {
    pub id: String,
    pub name: Option<String>,
    pub direction: RuleDirection,
    pub scope: Scope,
    pub matcher: MatcherDef,
    pub mode: Mode,
    #[serde(default)]
    pub needed_markers_all: Vec<String>,
    #[serde(default)]
    pub needed_markers_any: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RuleSetRaw {
    pub comment: Option<String>,
    pub markers: Vec<MarkerDef>,
    pub filters: Vec<FilterDef>,
}
