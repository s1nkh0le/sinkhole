use std::fmt::{Display, Formatter};
use shared::api::err::ErrorEnvelope;
use crate::fetch::error::CtrlErr::{Auth, Timeout};

#[derive(Debug)]
pub enum CtrlErr {
    Timeout,
    Network(reqwest::Error),
    Http(u16),
    Auth,
    Json(anyhow::Error),
    CustomEnvelope(ErrorEnvelope),
}

impl Display for CtrlErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use CtrlErr::*;
        match self {
            Timeout => write!(f, "Request timed out"),
            Network(e) => write!(f, "Network error: {}", e),
            Http(c) => write!(f, "HTTP {}", c),
            Auth => write!(f, "Unauthorized"),
            Json(e) => write!(f, "JSON decode error: {}", e),
            CustomEnvelope(env) => write!(f, "ErrorEnvelope: {}", env), // assumes Display for ErrorEnvelope
        }
    }
}

impl std::error::Error for CtrlErr {}

impl From<reqwest::Error> for CtrlErr {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            CtrlErr::Timeout
        } else {
            CtrlErr::Network(e)
        }
    }
}
