//! Platform-neutral core of Lustro.
//!
//! `model` and `scan` are the shared contracts (see `docs/CONTRACTS.md`).

#![forbid(unsafe_code)]

pub mod model;
pub mod scan;

pub mod category;
pub mod cleanup;
pub mod dto;
pub mod insights;
pub mod query;
pub mod snapshot;
pub mod tree;

/// Fake file trees (feature `fixtures`; always available to this crate's tests).
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
