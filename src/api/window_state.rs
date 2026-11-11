//! What the window's server holds on top of moon_core's [`moon_core::api::AppState`]: the X
//! display applications are started on and the language servers - neither of which a moon
//! without a window has. Derefs to the core state, so everything that only needs the
//! sessions and shells is handed this as it is.

use std::{
    ops::Deref,
    sync::{Arc, Mutex},
    time::Instant,
};

use axum::response::IntoResponse;

/// The window's and the HTTP server's state - see the module.
#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) core: moon_core::api::AppState,
    /// The X display applications with windows are started on - see [`crate::display`].
    pub(crate) display: Arc<crate::display::ServerDisplay>,
    /// The language servers running for these reviews. Repo-side like the shells beside it,
    /// because a server has to read the files it answers about - see [`crate::lsp`].
    pub(crate) lsp: Arc<crate::lsp::LanguageServers>,
}

impl AppState {
    /// A window's or a server's state, as it starts - see [`moon_core::api::AppState::build`].
    pub(crate) fn build(
        last_activity: Arc<Mutex<Instant>>,
        home_repo: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            core: moon_core::api::AppState::build(
                last_activity,
                home_repo,
                crate::palette_history::path(),
            ),
            display: Arc::default(),
            lsp: Arc::new(crate::lsp::LanguageServers::new()),
        }
    }
}

impl Deref for AppState {
    type Target = moon_core::api::AppState;

    fn deref(&self) -> &Self::Target {
        &self.core
    }
}

/// An error a route answers with: the request was not one this server could do.
#[derive(Debug)]
pub(crate) struct AppError(pub(crate) anyhow::Error);

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(value: E) -> Self {
        Self(value.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (axum::http::StatusCode::BAD_REQUEST, self.0.to_string()).into_response()
    }
}
