//! Ruleset compilation and validation.
//!
//! Operators submit a declarative ruleset; this module checks it for legality,
//! validates field combinations, and compiles it into the runtime matcher
//! structures the engine evaluates.

pub(crate) mod compile;
pub mod legality;
pub mod validate;

pub use compile::CompiledRuleSet;
pub use shared::types::ruleset_types::RuleSetRaw;
pub use shared::utils::version::calc_version_ruleset;
