//! The board's review files as rows: each one with where its work is checked out, how much of
//! it is uncommitted, and whether its commit has been made - see [`crate::moontasks::reviews`]
//! for the files themselves.
//!
//! The window asks the server for the rows, like everything else on the board: the folder is on
//! the machine the repo is on, and whether a review's commit has been made is git run in the
//! repos it names, which only that machine can do. See
//! `crate::moontasks::service::list_review_requests`.

// The files are read where the board's folder is, which is the server's side.
#[cfg(not(target_arch = "wasm32"))]
mod on_disk;

#[cfg(not(target_arch = "wasm32"))]
pub use on_disk::*;
