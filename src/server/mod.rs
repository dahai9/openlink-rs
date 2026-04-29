pub mod handlers;

use std::sync::Arc;

use axum::middleware;
use axum::routing::{get, post};
use axum::Router;

use crate::config::Config;
use crate::executor::Executor;
use crate::security::auth::auth_middleware;

pub struct AppState {
    pub config: Arc<Config>,
    pub executor: Executor,
}

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/auth", post(handlers::auth))
        .route("/config", get(handlers::config))
        .route("/tools", get(handlers::list_tools))
        .route("/exec", post(handlers::exec))
        .route("/prompt", get(handlers::prompt))
        .route("/skills", get(handlers::list_skills))
        .route("/files", get(handlers::list_files))
        .layer(middleware::from_fn_with_state(
            state.config.clone(),
            auth_middleware,
        ))
        .with_state(state)
}
