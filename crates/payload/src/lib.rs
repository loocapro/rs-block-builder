//! This crate defines different ways of building payloads.

#![warn(missing_docs, unreachable_pub, rustdoc::all)]
#![deny(unused_must_use, rust_2018_idioms)]

/// OFAC blacklisted addresses
pub mod utils;

/// Bidder implementation driving bids
pub mod bidder;
/// Generator responsible for generating jobs
pub mod generator;
/// Job responsible for building a block
pub mod job;
/// Service responsible for managing builds and communication
pub mod service;
/// Strategy algorithms used during block building
pub mod strategy;
/// Trait definitions for job and generator
pub mod traits;

/// Utilities for unit testing
#[cfg(test)]
pub mod test_utils;
