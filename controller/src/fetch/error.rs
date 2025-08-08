#[derive(Debug)]
pub enum CtrlErr {
    Timeout,
    Network(reqwest::Error),
    Http(u16),
    Auth,
    Json(anyhow::Error),
}

impl core::fmt::Display for CtrlErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use CtrlErr::*;
        match self {
            Timeout => write!(f, "request timed out"),
            Network(e) => write!(f, "network error: {}", e),
            Http(c) => write!(f, "HTTP {}", c),
            Auth => write!(f, "unauthorized"),
            Json(e) => write!(f, "decode error: {}", e),
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
