//! # VelocityX
//!
//! A comprehensive lock-free data structures library designed for high-performance concurrent programming.
//!
//! ## Features
//!
//! - **MPMC Queue**: Multi-producer, multi-consumer bounded queue
//! - **Concurrent HashMap**: Lock-free reads with concurrent modifications
//! - **Work-Stealing Deque**: For task scheduling and parallel workload distribution
//!
//! ## Philosophy
//!
//! VelocityX focuses on providing:
//! - Zero-cost abstractions with optimal performance
//! - Comprehensive safety guarantees through Rust's type system
//! - Ergonomic APIs that guide users toward correct concurrent programming patterns
//! - Extensive documentation and real-world usage examples
//!
//! ## Quick Start
//!
//! ```rust
//! use velocityx::queue::mpmc::MpmcQueue;
//!
//! let queue = MpmcQueue::new(100);
//! queue.push(42);
//! assert_eq!(queue.pop(), Some(42));
//! ```
//!
//! ## Thread Safety
//!
//! All data structures in VelocityX are designed to be thread-safe and can be safely shared
//! across threads without additional synchronization primitives.
//!
//! ## Performance
//!
//! VelocityX is optimized for modern multi-core processors with careful attention to:
//! - Cache-line alignment and padding
//! - Memory ordering semantics
//! - Contention minimization
//! - NUMA-aware design where applicable

#![no_std]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(feature = "unstable", feature(doc_cfg))]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod queue;
// pub mod map;  // Temporarily disabled for publishing
// pub mod deque;  // Temporarily disabled for publishing

#[cfg(feature = "std")]
pub use crate::queue::MpmcQueue;
// #[cfg(feature = "std")]
// pub use crate::map::concurrent::ConcurrentHashMap;
// #[cfg(feature = "std")]
// pub use crate::deque::work_stealing::WorkStealingDeque;

/// Common utilities and helper types
pub mod util {
    /// Cache line size for alignment purposes
    pub const CACHE_LINE_SIZE: usize = 64;

    /// Align a value to cache line boundaries
    #[inline]
    pub const fn align_to_cache_line(size: usize) -> usize {
        (size + CACHE_LINE_SIZE - 1) & !(CACHE_LINE_SIZE - 1)
    }

    /// Pad a struct to cache line size
    #[repr(align(64))]
    pub struct CachePadded<T> {
        value: T,
    }

    impl<T> CachePadded<T> {
        /// Create a new cache-padded value
        #[inline]
        pub const fn new(value: T) -> Self {
            Self { value }
        }

        /// Get a reference to the inner value
        #[inline]
        pub const fn get(&self) -> &T {
            &self.value
        }

        /// Get a mutable reference to the inner value
        #[inline]
        pub fn get_mut(&mut self) -> &mut T {
            &mut self.value
        }

        /// Get the inner value
        #[inline]
        pub fn into_inner(self) -> T {
            self.value
        }
    }

    // Implement atomic operations for common atomic types
    impl CachePadded<std::sync::atomic::AtomicUsize> {
        #[inline]
        pub fn store(&self, val: usize, order: std::sync::atomic::Ordering) {
            self.value.store(val, order);
        }

        #[inline]
        pub fn load(&self, order: std::sync::atomic::Ordering) -> usize {
            self.value.load(order)
        }

        #[inline]
        pub fn compare_exchange(
            &self,
            current: usize,
            new: usize,
            success: std::sync::atomic::Ordering,
            failure: std::sync::atomic::Ordering,
        ) -> Result<usize, usize> {
            self.value.compare_exchange(current, new, success, failure)
        }
    }

    impl CachePadded<std::sync::atomic::AtomicIsize> {
        #[inline]
        pub fn store(&self, val: isize, order: std::sync::atomic::Ordering) {
            self.value.store(val, order);
        }

        #[inline]
        pub fn load(&self, order: std::sync::atomic::Ordering) -> isize {
            self.value.load(order)
        }

        #[inline]
        pub fn compare_exchange(
            &self,
            current: isize,
            new: isize,
            success: std::sync::atomic::Ordering,
            failure: std::sync::atomic::Ordering,
        ) -> Result<isize, isize> {
            self.value.compare_exchange(current, new, success, failure)
        }
    }

    impl<T: Clone> Clone for CachePadded<T> {
        fn clone(&self) -> Self {
            Self::new(self.value.clone())
        }
    }

    impl<T: Copy> Copy for CachePadded<T> {}

    impl<T: core::fmt::Debug> core::fmt::Debug for CachePadded<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            core::fmt::Debug::fmt(&self.value, f)
        }
    }
}

/// Error types for VelocityX operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Operation would block (queue full, etc.)
    WouldBlock,
    /// Queue or data structure is closed
    Closed,
    /// Invalid operation for current state
    InvalidState,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::WouldBlock => write!(f, "Operation would block"),
            Error::Closed => write!(f, "Data structure is closed"),
            Error::InvalidState => write!(f, "Invalid operation for current state"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// Result type for VelocityX operations
pub type Result<T> = core::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::ToString;

    #[test]
    fn test_cache_line_alignment() {
        assert_eq!(util::align_to_cache_line(1), 64);
        assert_eq!(util::align_to_cache_line(64), 64);
        assert_eq!(util::align_to_cache_line(65), 128);
        assert_eq!(util::align_to_cache_line(127), 128);
        assert_eq!(util::align_to_cache_line(128), 128);
    }

    #[test]
    fn test_cache_padded() {
        let padded = util::CachePadded::new(42);
        assert_eq!(*padded.get(), 42);

        let mut padded = padded;
        *padded.get_mut() = 100;
        assert_eq!(padded.into_inner(), 100);
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            Error::WouldBlock.to_string().trim(),
            "Operation would block"
        );
        assert_eq!(Error::Closed.to_string().trim(), "Data structure is closed");
        assert_eq!(
            Error::InvalidState.to_string().trim(),
            "Invalid operation for current state"
        );
    }
}
