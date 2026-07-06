//! Binary wire protocol for the Velocity low-latency agent tool-call runtime.
//!
//! This crate provides a custom length-prefixed binary protocol that replaces
//! JSON serialization on every tool-call hop. All message schemas are compile-time
//! known — there is no reflection or runtime schema negotiation.
//!
//! # Performance Target
//!
//! Encode + decode round trip for a representative payload must complete
//! in under 5 microseconds.

pub mod codec;
pub mod messages;

pub use codec::*;
pub use messages::*;
