use std::sync::Arc;
use axum::extract::FromRef;
use crate::state::ctrl::ControllerState;
use crate::supervisor::supervisor::Supervisor;

#[derive(Clone)]
pub struct AppState {
    pub ctrl: ControllerState,
    pub supervisor: Arc<Supervisor>,
}

impl FromRef<AppState> for ControllerState {
    fn from_ref(app: &AppState) -> ControllerState {
        app.ctrl.clone()
    }
}

impl FromRef<AppState> for Arc<Supervisor> {
    fn from_ref(app: &AppState) -> Arc<Supervisor> {
        app.supervisor.clone()
    }
}