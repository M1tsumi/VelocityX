//! Queue implementations
//!
//! This module provides various lock-free queue implementations for different use cases.
//!
//! ## Available Queues
//!
//! - [`MpmcQueue`]: Multi-producer, multi-consumer bounded queue
//!
//! ## Choosing a Queue
//!
//! - **Lock-free**: All operations use only atomic primitives
//! - **Memory ordering**: Careful use of Acquire/Release/SeqCst ordering
//! - **ABA prevention**: Epoch-based reclamation in unbounded queues
//! - **Cache optimization**: Cache-line padding prevents false sharing
//! - **Comprehensive testing**: Unit tests, stress tests, property tests, and formal verification
//!
//! ## Performance Characteristics
//!
//! | Queue Type | Push | Pop | Memory | Contention |
//! |------------|------|-----|---------|------------|
//! | Bounded MPMC | O(1) | O(1) | Fixed | Low-Medium |
//! | Unbounded MPMC | O(1) | O(1) | Dynamic | Medium-High |
//!
//! ## Examples
//!
//! ```rust
//! use velocityx::queue::mpmc::{MpmcQueue, UnboundedMpmcQueue};
//! use std::thread;
//!
//! // Bounded queue for predictable memory usage
//! let bounded = MpmcQueue::new(1000);
//! bounded.push(42)?;
//!
//! // Unbounded queue for dynamic workloads
//! let unbounded = UnboundedMpmcQueue::new();
//! unbounded.push("hello")?;
//!
//! # Ok::<(), velocityx::Error>(())
//! ```
pub mod mpmc;

// Re-export main types for convenience
pub use mpmc::{MpmcQueue, UnboundedMpmcQueue, BoundedMpmcQueue};

// Include test modules
#[cfg(test)]
mod tests;

#[cfg(test)]
mod proptests;

#[cfg(test)]
mod loom_tests;
