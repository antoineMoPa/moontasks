//! Visualizations are moon_core's - see [`moon_core::visualizations`]. What is left here is
//! how the HTTP server serves them.

pub(crate) use moon_core::visualizations::*;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod routes;
