use serde::de::DeserializeOwned;
use serde_json::Value;
use crate::fetch::error::CtrlErr;

const STATUS: &'static str = "status";
const STATUS_OK: &'static str = "ok";
const DATA: &'static str = "data";

/// Unwraps json data within [shared::api::ok::OkEnvelope]
#[inline]
pub fn extract_ok_data<T: DeserializeOwned>(v: Value) -> Result<T, CtrlErr> {
    if v.get(STATUS) != Some(&Value::String(STATUS_OK.into())) {
        return Err(CtrlErr::Json(anyhow::anyhow!("status != ok")));
    }

    let data = v.get(DATA).ok_or_else(|| CtrlErr::Json(anyhow::anyhow!("missing data field")))?;
    serde_json::from_value(data.clone()).map_err(|e| CtrlErr::Json(e.into()))
}