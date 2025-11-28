//! Deque (double-ended queue) implementations
//!
//! This module provides specialized deque implementations optimized for different concurrent access patterns.
//!
//! ## Available Deques
//!
//! - [`WorkStealingDeque`]: Chase-Lev deque for work-stealing schedulers
//!
//! ## Choosing a Deque
//!
//! - Use `WorkStealingDeque` for task scheduling and parallel workload distribution
//! - Ideal for fork/join frameworks and thread pool implementations
//! - Provides efficient push/pop at both ends with stealing support

pub mod work_stealing;

#[cfg(feature = "std")]
pub use self::work_stealing::WorkStealingDeque;
