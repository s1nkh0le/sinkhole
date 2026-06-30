//! Comparison operator for numeric matchers.
//!
//! `CmpOp` now lives in `shared` (it is part of the wire model via `MatcherDef`).
//! This re-export keeps the `crate::matcher::types::CmpOp` path working.
pub use shared::types::base_types::CmpOp;
