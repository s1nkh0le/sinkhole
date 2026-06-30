use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use strum_macros::Display;
use utoipa::ToSchema;

/// Comparison operator used by numeric matchers (`size`, `session.lifetime`,
/// `session.idle`). Serialized as its symbol (`"<"`, `">="`, …) so existing
/// rulesets keep working, and parsed exactly once at deserialization time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum CmpOp {
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
    #[serde(rename = ">=")]
    Ge,
    #[serde(rename = ">")]
    Gt,
}

impl CmpOp {
    /// Compare two `u64` (or `usize as u64`).
    #[inline]
    pub fn eval_u64(self, lhs: u64, rhs: u64) -> bool {
        match self {
            CmpOp::Lt => lhs < rhs,
            CmpOp::Le => lhs <= rhs,
            CmpOp::Eq => lhs == rhs,
            CmpOp::Ne => lhs != rhs,
            CmpOp::Ge => lhs >= rhs,
            CmpOp::Gt => lhs > rhs,
        }
    }

    /// Compare two `f64`. Comparison is exact: `==`/`!=` on floats means bit-wise
    /// value equality, which is the intended semantics for timing thresholds
    /// (operators are author-chosen, not derived from lossy arithmetic).
    #[inline]
    pub fn eval_f64(self, lhs: f64, rhs: f64) -> bool {
        match self {
            CmpOp::Lt => lhs < rhs,
            CmpOp::Le => lhs <= rhs,
            CmpOp::Eq => lhs == rhs,
            CmpOp::Ne => lhs != rhs,
            CmpOp::Ge => lhs >= rhs,
            CmpOp::Gt => lhs > rhs,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Ge => ">=",
            CmpOp::Gt => ">",
        }
    }
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CmpOp {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim() {
            "<" => CmpOp::Lt,
            "<=" => CmpOp::Le,
            "==" => CmpOp::Eq,
            "!=" => CmpOp::Ne,
            ">=" => CmpOp::Ge,
            ">" => CmpOp::Gt,
            other => return Err(format!("invalid comparator op '{}'", other)),
        })
    }
}

/// The set of scopes in which matchers and filters can operate
#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    Hash,
    Ord,
    Display,
    PartialOrd,
    ToSchema,
)]
pub enum Scope {
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

/// Wire protocol detected for a single evaluated message.
///
/// `http/*` are framed HTTP messages; only 1.0 and 1.1 are parsed. HTTP/2 is
/// detected from its connection preface and tagged but not parsed (its fields are
/// not extracted, and stream matchers still see the raw bytes). `raw-*` are
/// unframed stream chunks, split by whether the bytes look like text (`raw-ascii`,
/// for example a CLI or TUI protocol) or not (`raw-binary`, unknown binary
/// protocols). Detection is best-effort and per-message.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
pub enum Protocol {
    #[serde(rename = "http/1.0")]
    Http10,
    #[serde(rename = "http/1.1")]
    Http11,
    #[serde(rename = "http/2")]
    Http2,
    #[serde(rename = "raw-ascii")]
    RawAscii,
    #[serde(rename = "raw-binary")]
    RawBinary,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Http10 => "http/1.0",
            Protocol::Http11 => "http/1.1",
            Protocol::Http2 => "http/2",
            Protocol::RawAscii => "raw-ascii",
            Protocol::RawBinary => "raw-binary",
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Protocol {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "http/1.0" => Protocol::Http10,
            "http/1.1" => Protocol::Http11,
            "http/2" => Protocol::Http2,
            "raw-ascii" => Protocol::RawAscii,
            "raw-binary" => Protocol::RawBinary,
            other => return Err(format!("invalid protocol '{}'", other)),
        })
    }
}

/// Transport channel a guarded service speaks on a given public port.
///
/// Immutable: it is fixed at filter configuration time. A port is `Tls` when it
/// has a certificate and key configured (`TLS_MAP`), in which case the filter
/// terminates client TLS and re-encrypts to the upstream; otherwise it is plain
/// `Tcp`. Surfaced in [`PortInfo`](crate::types::ruleset_types::PortInfo) so the
/// UI can show the division at a glance.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, ToSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum PortChannel {
    /// Bare TCP (cleartext).
    #[default]
    Tcp,
    /// TLS over TCP (terminated + re-encrypted by the filter).
    Tls,
}

impl PortChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            PortChannel::Tcp => "tcp",
            PortChannel::Tls => "tls",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmpop_serde_uses_symbols() {
        let cases = [
            (CmpOp::Lt, "\"<\""),
            (CmpOp::Le, "\"<=\""),
            (CmpOp::Eq, "\"==\""),
            (CmpOp::Ne, "\"!=\""),
            (CmpOp::Ge, "\">=\""),
            (CmpOp::Gt, "\">\""),
        ];
        for (op, sym) in cases {
            assert_eq!(serde_json::to_string(&op).unwrap(), sym);
            let back: CmpOp = serde_json::from_str(sym).unwrap();
            assert_eq!(back, op);
        }
    }

    #[test]
    fn cmpop_rejects_invalid_symbol() {
        assert!(serde_json::from_str::<CmpOp>("\"=>\"").is_err());
        assert!("=>".parse::<CmpOp>().is_err());
        assert_eq!("<=".parse::<CmpOp>().unwrap(), CmpOp::Le);
    }

    #[test]
    fn cmpop_eval_u64() {
        assert!(CmpOp::Gt.eval_u64(5, 3));
        assert!(!CmpOp::Gt.eval_u64(3, 5));
        assert!(CmpOp::Le.eval_u64(3, 3));
        assert!(CmpOp::Ne.eval_u64(3, 4));
    }

    #[test]
    fn cmpop_eval_f64_exact() {
        assert!(CmpOp::Eq.eval_f64(2.5, 2.5));
        assert!(!CmpOp::Eq.eval_f64(2.5, 2.5000001));
        assert!(CmpOp::Lt.eval_f64(1.0, 2.0));
    }
}
