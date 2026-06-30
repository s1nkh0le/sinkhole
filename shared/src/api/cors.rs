//! Shared CORS policy for both control APIs (filter + controller).
//!
//! The two binaries previously each open-coded an identical `CorsLayer`; this is
//! the single source of truth. Only the *defaulting* and *logging* differ per
//! side and stay at the call site: the controller fails closed (config refuses to
//! start without an origin in non-debug mode, unless it serves the bundled
//! same-origin UI), the filter defaults permissive.

use axum::http::Method;
use axum::http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

/// Build the standard control-API CORS layer.
///
/// - `Some(origins)` — comma-separated allow-list → **strict**: only those
///   origins, methods `GET/POST/PUT/DELETE`.
/// - `None` → **permissive**: any origin, any method.
///
/// Allowed request headers are always `Authorization` / `Content-Type` /
/// `Accept`. Unparseable origins in the list are silently dropped.
pub fn build_cors_layer(cors_origin: Option<&str>) -> CorsLayer {
    match cors_origin {
        Some(origins) => {
            let list: Vec<_> = origins
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(list))
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([AUTHORIZATION, CONTENT_TYPE, ACCEPT])
        }
        None => CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers([AUTHORIZATION, CONTENT_TYPE, ACCEPT]),
    }
}

/// Build a CORS layer that allows **no cross-origin** requests (same-origin only).
///
/// Used by the controller when it serves the bundled web UI and no explicit origin
/// is configured: the UI is same-origin, so CORS never applies to it, and any
/// *cross*-origin browser request should simply be denied rather than allowed
/// wholesale (`Any`). An empty allow-list emits no `Access-Control-Allow-Origin`,
/// so browsers block cross-origin while same-origin requests are unaffected.
pub fn build_cors_layer_same_origin_only() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(Vec::<axum::http::HeaderValue>::new()))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE, ACCEPT])
}
