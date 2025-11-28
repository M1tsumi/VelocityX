//! Map implementations
//!
//! This module provides concurrent map implementations optimized for different access patterns.
//!
//! ## Available Maps
//!
//! - [`ConcurrentHashMap`]: Lock-free reads with concurrent modifications
//!
//! ## Choosing a Map
//!
//! - Use `ConcurrentHashMap` for general-purpose concurrent key-value storage
//! - Consider read/write ratios when tuning performance
//! - Monitor contention levels for optimization opportunities

pub mod concurrent;

#[cfg(feature = "std")]
pub use self::concurrent::ConcurrentHashMap;
