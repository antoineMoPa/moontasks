//! The socket a shell reaches a moon through is moon_core's - see [`moon_core::instances`].
//! What is left here is the window's side of it.

pub(crate) use moon_core::instances::*;

pub(crate) mod window;

#[cfg(test)]
mod tests;
