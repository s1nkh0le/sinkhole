use axum::http::StatusCode;
use axum::Json;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{json, Value};

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Serialize, Clone)]
pub struct ErrorEnvelope {
    /// The status string; always "error"
    pub status: &'static str,

    /// HTTP status code for the error
    pub code: u16,

    /// A concise, human-readable summary of the error
    pub title: String,

    /// An optional detailed description providing more context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    /// An optional machine-readable error identifier.
    pub error: Option<String>,

    /// An optional URI reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl IntoResponse for ErrorEnvelope {
    fn into_response(self) -> Response {
        // Try to map your code into a StatusCode, falling back to 500 if it's invalid
        let status = StatusCode::from_u16(self.code)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        // (status, Json(body)) is already IntoResponse for a JSON payload
        (status, Json(self)).into_response()
    }
}


impl ErrorEnvelope {
    pub fn new(
        code: StatusCode,
        title: impl Into<String>
    ) -> Self {
        Self {
            status: "error",
            code: code.as_u16(),
            title: title.into(),
            detail: None,
            error: None,
            instance: None,
        }
    }

    pub fn full(
        code: StatusCode,
        title: impl Into<String>,
        detail: impl Into<String>,
        error: impl Into<String>,
        instance: impl Into<String>,
    ) -> (StatusCode, Json<Value>) {
        Self {
            status: "error",
            code: code.as_u16(),
            title: title.into(),
            detail: Some(detail.into()),
            error: Some(error.into()),
            instance: Some(instance.into()),
        }.into_result()
    }

    pub fn with_error(mut self, err: impl Into<String>) -> Self {
        self.error = Some(err.into());
        self
    }


    pub fn with_error_list<I, S>(mut self, msgs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        const ERROR_LIST_DELIM: &str = "; ";

        let joined = msgs
            .into_iter()
            .map(|s| s.as_ref().to_owned())
            .collect::<Vec<_>>()
            .join(ERROR_LIST_DELIM);
        self.error = Some(joined);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    pub fn into_result(self) -> (StatusCode, Json<Value>) {
        // Convert the stored numeric code back to StatusCode; default to 500.
        let code = StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (code, Json(json!(self)))
    }


    pub fn bad_request(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, reason_for(StatusCode::BAD_REQUEST))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn unauthorized(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, reason_for(StatusCode::UNAUTHORIZED))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn forbidden(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, reason_for(StatusCode::FORBIDDEN))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn not_found(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, reason_for(StatusCode::NOT_FOUND))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn conflict(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, reason_for(StatusCode::CONFLICT))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn unprocessable_entity(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, reason_for(StatusCode::UNPROCESSABLE_ENTITY))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn too_many_requests(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::TOO_MANY_REQUESTS, reason_for(StatusCode::TOO_MANY_REQUESTS))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn internal(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, reason_for(StatusCode::INTERNAL_SERVER_ERROR))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn service_unavailable(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, reason_for(StatusCode::SERVICE_UNAVAILABLE))
            .with_error(error)
            .with_instance(instance)
    }
    pub fn bad_gateway(error: impl Into<String>, instance: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, reason_for(StatusCode::BAD_GATEWAY))
            .with_error(error)
            .with_instance(instance)
    }
}

fn reason_for(code: StatusCode) -> &'static str {
    code.canonical_reason().unwrap_or("Error")
}



#[inline]
pub fn err(code: StatusCode, err: impl Into<String>) -> (StatusCode, Json<Value>) {
    ErrorEnvelope::new(code, reason_for(code))
        .with_error(err)
        .into_result()
}

#[inline]
pub fn err_with_detail(code: StatusCode, err: impl Into<String>, details: impl Into<String>) -> (StatusCode, Json<Value>) {
    ErrorEnvelope::new(code, reason_for(code))
        .with_error(err)
        .with_detail(details)
        .into_result()
}

#[inline]
pub fn err_with_instance(
    code: StatusCode,
    error: impl Into<String>,
    instance: impl Into<String>,
) -> (StatusCode, Json<Value>) {
    ErrorEnvelope::new(code, reason_for(code))
        .with_error(error)
        .with_instance(instance)
        .into_result()
}

#[inline]
pub fn err_full(code: StatusCode, title: impl Into<String>, detail: impl Into<String>, error: impl Into<String>, instance: impl Into<String>) -> (StatusCode, Json<Value>) {
    ErrorEnvelope::full(code, title, detail, error, instance)
}