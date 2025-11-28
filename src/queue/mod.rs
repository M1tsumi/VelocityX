//! Queue implementations for VelocityX
//!
//! This module provides various queue implementations optimized for different use cases.
//!
//! ## Available Queues
//!
//! - [`MpmcQueue`]: Multi-producer, multi-consumer bounded queue
//!
//! ## Choosing a Queue
//!
//! - **Simple implementation**: Uses Mutex for thread safety
//! - **Memory ordering**: Basic atomic operations for head/tail tracking
//! - **Cache optimization**: Cache-line padding prevents false sharing
//! - **Comprehensive testing**: Unit tests included
//!
//! ## Performance Characteristics
//!
//! | Queue Type | Push | Pop | Memory | Contention |
//! |------------|------|-----|---------|------------|
//! | Bounded MPMC | O(1) | O(1) | Fixed | Low-Medium |
//!
//! ## Examples
//!
//! ```rust
//! use velocityx::queue::MpmcQueue;
//!
//! // Bounded queue for predictable memory usage
//! let bounded = MpmcQueue::new(1000);
//! bounded.push(42)?;
//!
//! # Ok::<(), velocityx::Error>(())
//! ```

pub mod mpmc_simple;

// Re-export the main queue types
pub use mpmc_simple::MpmcQueue;
