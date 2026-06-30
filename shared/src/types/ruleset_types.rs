use crate::types::base_types::{CmpOp, Mode, PortChannel, RuleDirection, Scope};
use serde::{Deserialize, Serialize};
use std::fmt;
use utoipa::ToSchema;

fn default_enabled() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
#[serde(tag = "type")]
pub enum MatcherDef {
    /// Applies a regex to the request/response stream
    #[serde(rename = "stream_regex")]
    StreamRegex {
        /// The regex pattern to match against the stream
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Applies a regex to the request/response path
    #[serde(rename = "path_regex")]
    PathRegex {
        /// The regex pattern to match against the path
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Applies a regex to the request/response's message body
    #[serde(rename = "body_regex")]
    BodyRegex {
        /// The regex pattern to match against the body
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Applies a regex to a specific header’s value
    #[serde(rename = "header_regex")]
    HeaderRegex {
        /// The name of the header to inspect
        header_name: String,
        /// The regex pattern to match against the header value
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Applies a regex to the request/response HTTP method name
    #[serde(rename = "method_regex")]
    MethodRegex {
        /// The regex pattern to match against the HTTP method
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Applies a regex to the response status code (as text)
    #[serde(rename = "status_regex")]
    StatusRegex {
        /// The regex pattern to match against the status code
        pattern: String,
        #[serde(default)]
        case_sensitive: bool,
    },
    /// Compares the size of the scope with a fixed value
    #[serde(rename = "size")]
    Size {
        /// Comparison operator: `"<"`, `"<="`, `"=="`, `"!="`, `">="`, or `">"`
        op: CmpOp,
        /// The size threshold value to compare against
        value: usize,
        /// Optional field name (for header scopes, selects header names).
        #[serde(default)]
        field_name: Option<String>, // for HttpHeader scope, selects header names
    },
    /// Matches session lifetime
    #[serde(rename = "session.lifetime")]
    SessionLifetime {
        /// Comparison operator: `"<"`, `"<="`, `"=="`, `"!="`, `">="`, or `">"`
        op: CmpOp,
        /// The time threshold value to compare session lifetime against
        value: f64,
    },
    /// Matches session idle time
    #[serde(rename = "session.idle")]
    SessionIdle {
        /// Comparison operator: `"<"`, `"<="`, `"=="`, `"!="`, `">="`, or `">"`
        op: CmpOp,
        /// The time threshold value to compare session idle time against
        value: f64,
    },
    /// Applies a byte-regex to the raw request/response stream (binary-safe).
    ///
    /// Compiled with non-Unicode semantics, so `.` matches any byte and `\xNN`
    /// is a literal byte. Use this instead of `stream_regex` for binary traffic.
    #[serde(rename = "stream_bytes")]
    StreamBytes {
        /// The byte-regex pattern to match against the raw stream
        pattern: String,
    },
    /// Applies a byte-regex to the message body (binary-safe).
    ///
    /// Like `body_regex` but matches raw bytes; see [`MatcherDef::StreamBytes`].
    #[serde(rename = "body_bytes")]
    BodyBytes {
        /// The byte-regex pattern to match against the body
        pattern: String,
    },
    /// Matches a literal hex byte signature anywhere in the scope's bytes.
    #[serde(rename = "bytes_hex")]
    BytesHex {
        /// Hex signature, e.g. `"deadbeef"` (whitespace allowed)
        value: String,
    },
    /// Always matches or never matches, based on the boolean value.
    #[serde(rename = "always")]
    Always {
        /// If `true`, always matches; if `false`, never matches.
        value: bool,
    },
}

/// Definition of a marker rule
#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct MarkerDef {
    /// Unique identifier for this marker
    pub id: String,
    /// Optional human-readable name for the marker
    pub name: Option<String>,
    /// Direction (client request or service response) the marker applies to
    pub direction: RuleDirection,
    /// Scope in which to apply the marker
    pub scope: Scope,
    // The matcher that decides whether to apply the marker logic
    pub matcher: MatcherDef,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Definition of a filter rule
#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct FilterDef {
    /// Unique identifier for this filter
    pub id: String,
    /// Optional human-readable name for the filter
    pub name: Option<String>,
    /// Direction (client request or service response) the filter applies to
    pub direction: RuleDirection,
    /// Scope in which to apply the filter
    pub scope: Scope,
    // The matcher that decides whether to apply the filter logic
    pub matcher: MatcherDef,
    /// Mode of the filter
    pub mode: Mode,
    /// List of marker IDs, all of which must be present for the filter to apply
    #[serde(default)]
    pub needed_markers_all: Vec<String>,
    /// List of marker IDs, at least one of which must be present for the filter to apply
    #[serde(default)]
    pub needed_markers_any: Vec<String>,
    /// Whether this filter is currently enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

/// Port information and metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PortInfo {
    /// The public-facing port number that clients connect to
    pub public: u16,
    /// The internal/upstream port number to which traffic is forwarded
    pub upstream: u16,
    /// Optional human-readable name for this port mapping
    pub name: Option<String>,
    /// Optional free-form comment or description
    pub comment: Option<String>,
    /// Optional hex color code (e.g. `"#ff00ff"`) used for UI indicators
    pub hex_color: Option<String>,
    /// List of scopes that are enabled for this port
    pub enabled_scopes: Vec<Scope>,
    /// Transport channel (immutable): `tcp` for cleartext, `tls` for a
    /// TLS-terminated+re-encrypted port. Derived from the port's certificate
    /// configuration at filter startup.
    #[serde(default)]
    pub channel: PortChannel,
}

/// Set of markers and filters for a given port runtime configuration
#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct RuleSetRaw {
    /// List of markers
    pub markers: Vec<MarkerDef>,
    /// List of filters
    pub filters: Vec<FilterDef>,
    /// Optional git-style commit message describing this ruleset revision.
    pub commit_message: Option<String>,
}

impl fmt::Display for RuleSetRaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#?}", self)
    }
}
