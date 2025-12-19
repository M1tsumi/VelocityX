//! # VelocityX
//!
//! A comprehensive lock-free data structures library designed for high-performance concurrent programming in Rust.
//!
//! ## 🚀 Features
//!
//! - **MPMC Queue**: Multi-producer, multi-consumer bounded queue with zero locks
//! - **Concurrent HashMap**: Lock-free reads with concurrent modifications using striped locking
//! - **Work-Stealing Deque**: Chase-Lev deque for task scheduling and parallel workload distribution
//! - **Lock-Free Stack**: Treiber's algorithm stack with wait-free push and lock-free pop operations
//! - **Performance Metrics**: Real-time monitoring with `MetricsCollector` trait across all data structures
//!
//! ## 🎯 Philosophy
//!
//! VelocityX focuses on providing:
//! - Zero-cost abstractions with optimal performance
//! - Comprehensive safety guarantees through Rust's type system
//! - Ergonomic APIs that guide users toward correct concurrent programming patterns
//! - Extensive documentation and real-world usage examples
//! - Production-ready performance monitoring and metrics collection
//!
//! ## ⚡ Quick Start
//!
//! ```rust
//! use velocityx::{MpmcQueue, MetricsCollector};
//!
//! let queue = MpmcQueue::new(100);
//! queue.push(42);
//! assert_eq!(queue.pop(), Some(42));
//!
//! // Get performance metrics
//! let metrics = queue.metrics();
//! println!("Success rate: {:.2}%", metrics.success_rate());
//! ```
//!
//! ## 🔒 Thread Safety
//!
//! All data structures in VelocityX are designed to be thread-safe and can be safely shared
//! across threads without additional synchronization primitives.
//!
//! ## 📊 Performance
//!
//! VelocityX is optimized for modern multi-core processors with careful attention to:
//! - Cache-line alignment and padding to prevent false sharing
//! - Memory ordering semantics for correctness and performance
//! - Contention minimization through lock-free algorithms
//! - NUMA-aware design where applicable
//! - Real-time performance monitoring with minimal overhead

#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
pub mod deque;
#[cfg(feature = "std")]
pub mod map;
pub mod metrics;
#[cfg(feature = "std")]
pub mod queue;
#[cfg(feature = "std")]
pub mod stack;

#[cfg(feature = "std")]
pub use crate::deque::work_stealing::WorkStealingDeque;
#[cfg(feature = "std")]
pub use crate::map::concurrent::ConcurrentHashMap;
#[cfg(feature = "std")]
pub use crate::metrics::{MetricsCollector, PerformanceMetrics};
#[cfg(all(feature = "std", feature = "lockfree"))]
pub use crate::queue::LockFreeMpmcQueue;
#[cfg(feature = "std")]
pub use crate::queue::MpmcQueue;
#[cfg(feature = "std")]
pub use crate::stack::LockFreeStack;

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
    impl CachePadded<core::sync::atomic::AtomicUsize> {
        /// Store a value into the atomic integer
        #[inline]
        pub fn store(&self, val: usize, order: core::sync::atomic::Ordering) {
            self.value.store(val, order);
        }

        /// Load a value from the atomic integer
        #[inline]
        pub fn load(&self, order: core::sync::atomic::Ordering) -> usize {
            self.value.load(order)
        }

        /// Compare and exchange operation on the atomic integer
        #[inline]
        pub fn compare_exchange(
            &self,
            current: usize,
            new: usize,
            success: core::sync::atomic::Ordering,
            failure: core::sync::atomic::Ordering,
        ) -> Result<usize, usize> {
            self.value.compare_exchange(current, new, success, failure)
        }
    }

    impl CachePadded<core::sync::atomic::AtomicIsize> {
        /// Store a value into the atomic integer
        #[inline]
        pub fn store(&self, val: isize, order: core::sync::atomic::Ordering) {
            self.value.store(val, order);
        }

        /// Load a value from the atomic integer
        #[inline]
        pub fn load(&self, order: core::sync::atomic::Ordering) -> isize {
            self.value.load(order)
        }

        /// Compare and exchange operation on the atomic integer
        #[inline]
        pub fn compare_exchange(
            &self,
            current: isize,
            new: isize,
            success: core::sync::atomic::Ordering,
            failure: core::sync::atomic::Ordering,
        ) -> Result<isize, isize> {
            self.value.compare_exchange(current, new, success, failure)
        }
    }

    impl CachePadded<core::sync::atomic::AtomicBool> {
        /// Store a value into the atomic boolean
        #[inline]
        pub fn store(&self, val: bool, order: core::sync::atomic::Ordering) {
            self.value.store(val, order);
        }

        /// Load a value from the atomic boolean
        #[inline]
        pub fn load(&self, order: core::sync::atomic::Ordering) -> bool {
            self.value.load(order)
        }

        /// Compare and exchange operation on the atomic boolean
        #[inline]
        pub fn compare_exchange(
            &self,
            current: bool,
            new: bool,
            success: core::sync::atomic::Ordering,
            failure: core::sync::atomic::Ordering,
        ) -> Result<bool, bool> {
            self.value.compare_exchange(current, new, success, failure)
        }
    }

    impl<T> CachePadded<core::sync::atomic::AtomicPtr<T>> {
        /// Store a value into the atomic pointer
        #[inline]
        pub fn store(&self, val: *mut T, order: core::sync::atomic::Ordering) {
            self.value.store(val, order);
        }

        /// Load a value from the atomic pointer
        #[inline]
        pub fn load(&self, order: core::sync::atomic::Ordering) -> *mut T {
            self.value.load(order)
        }

        /// Compare and exchange operation on the atomic pointer
        #[inline]
        pub fn compare_exchange(
            &self,
            current: *mut T,
            new: *mut T,
            success: core::sync::atomic::Ordering,
            failure: core::sync::atomic::Ordering,
        ) -> Result<*mut T, *mut T> {
            self.value.compare_exchange(current, new, success, failure)
        }
    }

    impl CachePadded<core::sync::atomic::AtomicU64> {
        /// Add to the atomic integer, returning the previous value
        #[inline]
        pub fn fetch_add(&self, val: u64, order: core::sync::atomic::Ordering) -> u64 {
            self.value.fetch_add(val, order)
        }
    }

    #[cfg(feature = "std")]
    impl<T> CachePadded<parking_lot::Mutex<T>> {
        /// Lock the mutex
        #[inline]
        pub fn lock(&self) -> parking_lot::MutexGuard<'_, T> {
            self.value.lock()
        }
    }

    impl<T> CachePadded<T> {
        /// Get a mutable reference to the inner value (for indexing)
        #[inline]
        pub fn inner_mut(&mut self) -> &mut T {
            &mut self.value
        }

        /// Get a reference to the inner value (for indexing)
        #[inline]
        pub fn inner(&self) -> &T {
            &self.value
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
    /// Capacity exceeded (for bounded structures)
    CapacityExceeded,
    /// Data structure is poisoned (thread panic occurred)
    Poisoned,
    /// Invalid argument provided
    InvalidArgument,
    /// Memory allocation failed
    OutOfMemory,
    /// Operation timed out
    Timeout,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::WouldBlock => write!(f, "Operation would block"),
            Error::Closed => write!(f, "Data structure is closed"),
            Error::InvalidState => write!(f, "Invalid operation for current state"),
            Error::CapacityExceeded => write!(f, "Capacity exceeded"),
            Error::Poisoned => write!(f, "Data structure is poisoned due to thread panic"),
            Error::InvalidArgument => write!(f, "Invalid argument provided"),
            Error::OutOfMemory => write!(f, "Memory allocation failed"),
            Error::Timeout => write!(f, "Operation timed out"),
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
        assert_eq!(
            Error::CapacityExceeded.to_string().trim(),
            "Capacity exceeded"
        );
        assert_eq!(
            Error::Poisoned.to_string().trim(),
            "Data structure is poisoned due to thread panic"
        );
        assert_eq!(
            Error::InvalidArgument.to_string().trim(),
            "Invalid argument provided"
        );
        assert_eq!(
            Error::OutOfMemory.to_string().trim(),
            "Memory allocation failed"
        );
        assert_eq!(Error::Timeout.to_string().trim(), "Operation timed out");
    }
}
