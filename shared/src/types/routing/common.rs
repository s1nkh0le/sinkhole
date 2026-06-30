use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response `data` for `/auth/ping` on **both** the filter and the controller.
///
/// `/auth/ping` is the authenticated reachability check the web UI uses: a 200
/// confirms the node is up *and* the caller's token is valid. `instance` tells
/// the frontend which kind of node answered without needing a separate call.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthPingData {
    /// Which kind of node answered: `"filter"` or `"controller"`.
    pub instance: String,
    /// The node process is up.
    pub healthy: bool,
}

impl crate::api::ok::sealed::Sealed for AuthPingData {}
impl crate::api::ok::OkData for AuthPingData {}

impl AuthPingData {
    pub const FILTER: &'static str = "filter";
    pub const CONTROLLER: &'static str = "controller";

    /// Construct a healthy ping for the given instance kind.
    pub fn healthy(instance: &str) -> Self {
        Self {
            instance: instance.to_string(),
            healthy: true,
        }
    }
}
