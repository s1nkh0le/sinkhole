use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use serde_json::Value;

pub trait OkData: sealed::Sealed + Serialize {}

pub mod sealed {
    pub trait Sealed {}
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(bound = "T: serde::Serialize + utoipa::ToSchema"))]
pub struct OkEnvelope<T: OkData> {
    pub status: &'static str,
    pub data: T,
}

pub fn ok<T: OkData + Serialize>(data: T) -> (StatusCode, Json<Value>) {
    let env = OkEnvelope { status: "ok", data };
    (StatusCode::OK, Json(serde_json::to_value(env).expect("serialize ok() envelope")))
}
