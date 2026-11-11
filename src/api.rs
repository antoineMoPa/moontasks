//! The wire types and the server's state are moon_core's - see [`moon_core::api`]. What is
//! left here is the window's side of them: applications, displays, folders and encrypted files,
//! which only a window or the HTTP server asks about.

pub(crate) use moon_core::api::*;

pub(crate) mod applications;
pub(crate) mod display;
pub(crate) mod encrypted_files;
pub(crate) mod folders;
#[cfg(not(target_arch = "wasm32"))]
mod window_state;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use window_state::{AppError, AppState};
