use crate::types::{Mode, RuleDirection, Scope};
use serde::{Serialize, Deserialize};

fn default_enabled() -> bool { true }

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum MatcherDef {
    #[serde(rename = "stream_regex")]
    StreamRegex {
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    #[serde(rename = "path_regex")]
    PathRegex {
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    #[serde(rename = "body_regex")]
    BodyRegex {
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    #[serde(rename = "header_regex")]
    HeaderRegex {
        header_name: String,
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    #[serde(rename = "method_regex")]
    MethodRegex {
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    #[serde(rename = "status_regex")]
    StatusRegex {
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    #[serde(rename = "size")]
    Size {
        op: String,   // ">", ">=", "<", "<=", "==", "!="
        value: usize,
        #[serde(default)]
        field_name: Option<String>, // e.g. For HttpHeader scope, indexes header names
    },
    #[serde(rename = "always")]
    Always { value: bool },
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
