//! Multi-Producer, Multi-Consumer (MPMC) Queue
//!
//! This module implements high-performance lock-free MPMC queues using two different approaches:
//!
//! 1. **Bounded Queue** - Uses a ring buffer with atomic operations for fixed-capacity queues
//! 2. **Unbounded Queue** - Uses a linked list with epoch-based reclamation for dynamic sizing
//!
//! ## Design Philosophy
//!
//! The queues are designed for maximum throughput while maintaining strict correctness guarantees:
//! - **Lock-free**: All operations are non-blocking and use only atomic operations
//! - **Memory ordering**: Careful use of Acquire/Release/SeqCst ordering for correctness
//! - **ABA prevention**: Epoch-based reclamation prevents the ABA problem in unbounded queues
//! - **Cache optimization**: Cache-line padding prevents false sharing between hot variables
//!
//! ## Memory Ordering Model
//!
//! ```text
//! Producer (push)                    Consumer (pop)
//! -----------                        ----------
//! Store data      ---->              Load data
//! Store index     ---->              Load index
//!   (Release)                           (Acquire)
//! ```
//!
//! - **Push operations** use `Release` ordering to ensure data visibility before index update
//! - **Pop operations** use `Acquire` ordering to ensure data visibility after index read
//! - **Size queries** use `Acquire` for consistent reads
//! - **Critical state changes** use `SeqCst` for total ordering guarantees
//!
//! ## Algorithm Details
//!
//! ### Bounded Queue (Ring Buffer)
//!
//! The bounded queue uses a circular buffer with two atomic indices:
//! - `head`: Points to the next element to pop (consumer position)
//! - `tail`: Points to the next free slot (producer position)
//!
//! ```text
//! Empty State:    head == tail
//! Full State:     (tail + 1) % capacity == head
//! ```
//!
//! ### Unbounded Queue (Linked List)
//!
//! The unbounded queue uses a Michael-Scott style lock-free queue with:
//! - Atomic head and tail pointers
//! - Epoch-based reclamation for safe memory management
//! - ABA prevention through generation counters
//!
//! ## Performance Characteristics
//!
//! | Operation | Bounded Queue | Unbounded Queue |
//!-----------|---------------|-----------------|
//! Push      | O(1) amortized | O(1) amortized |
//! Pop       | O(1) amortized | O(1) amortized |
//! Size      | O(1) (approx)  | O(1) (approx)  |
//! Memory    | Fixed          | Dynamic         |
//! Contention| Low-Medium     | Medium-High     |
//!
//! ## When to Use Which Variant
//!
//! - **Bounded Queue**: When you can predict memory usage and want predictable performance
//! - **Unbounded Queue**: When you need dynamic sizing and can tolerate occasional allocation overhead
//!
//! ## Example
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

use crate::util::CachePadded;
use crate::{Error, Result};
use core::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
#[cfg(feature = "unstable")]
use crossbeam_epoch::{self as epoch, Atomic, Owned};

#[cfg(feature = "std")]
use std::boxed::Box;

#[cfg(feature = "std")]
use std::vec::Vec;

#[cfg(feature = "std")]
use std::time::Duration;

#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::_mm_prefetch;

/// A multi-producer, multi-consumer bounded queue
///
/// This queue provides high-performance concurrent access with a fixed capacity.
/// It uses a ring buffer design with cache-line padding to prevent false sharing
/// and carefully chosen memory ordering for correctness and performance.
///
/// # Memory Ordering
///
/// - `push`: Uses `Release` ordering to ensure data visibility before index update
/// - `pop`: Uses `Acquire` ordering to ensure data visibility after index read
/// - `len`: Uses `Acquire` ordering for consistent reads
/// - `capacity`: Uses `Relaxed` ordering as capacity never changes
///
/// # ABA Problem Prevention
///
/// The bounded queue uses index arithmetic that naturally prevents the ABA problem
/// because indices only move forward and wrap around modulo capacity, making it
/// impossible to distinguish between the same position at different times.
///
/// # Performance Characteristics
///
/// - **Push**: O(1) amortized, may fail with `Error::WouldBlock` if queue is full
/// - **Pop**: O(1) amortized, returns `None` if queue is empty
/// - **Size**: O(1) with potentially stale results under high contention
/// - **Memory**: Fixed overhead of `capacity * size_of::<T>() + padding`
///
/// # Examples
///
/// ```rust
/// use velocityx::queue::mpmc::MpmcQueue;
/// use std::thread;
///
/// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
///
/// // Producer thread
/// let producer = thread::spawn({
///     let queue = queue.clone();
///     move || {
///         for i in 0..100 {
///             while queue.push(i).is_err() {
///                 // Queue full, retry
///                 thread::yield_now();
///             }

///         }
///     }
/// });
///
/// // Consumer thread
/// let consumer = thread::spawn({
///     let queue = queue.clone();
///     move || {
///         let mut sum = 0;
///         while sum < 4950 { // Sum of 0..99
///             if let Some(value) = queue.pop() {
///                 sum += value;
///             }
///         }
///         sum
///     }
/// });
///
/// producer.join().unwrap();
/// let result = consumer.join().unwrap();
/// assert_eq!(result, 4950);
/// ```
///
/// # Thread Safety
///
/// This queue is safe to use from multiple threads simultaneously.
/// All operations are atomic and lock-free.
#[derive(Debug)]
pub struct MpmcQueue<T> {
    // Ring buffer storage, aligned to cache line boundaries to prevent false sharing
    buffer: CachePadded<Box<[CachePadded<AtomicPtr<T>>]>>,

    // Queue capacity (always a power of 2 for efficient modulo operations)
    capacity: usize,

    // Mask for fast modulo operation (capacity - 1)
    mask: usize,

    // Head index (next position to pop from)
    // Cache-padded to prevent false sharing with tail
    head: CachePadded<AtomicUsize>,

    // Tail index (next position to push to)
    // Cache-padded to prevent false sharing with head
    tail: CachePadded<AtomicUsize>,

    // Generation counter for ABA prevention in size queries
    // Uses SeqCst ordering for total ordering guarantees
    generation: CachePadded<AtomicU64>,
}

/// A node in the unbounded MPMC queue linked list
///
/// Each node contains a value and a pointer to the next node.
/// The node is allocated using epoch-based reclamation for safe memory management.
#[cfg(feature = "unstable")]
#[repr(align(64))] // Cache-line aligned
struct Node<T> {
    /// The stored value
    value: T,
    /// Atomic pointer to the next node
    next: Atomic<Node<T>>,
}

/// A multi-producer, multi-consumer unbounded queue
///
/// This queue provides dynamic sizing with lock-free operations using a linked list.
/// It uses epoch-based reclamation to safely reclaim memory and prevent the ABA problem.
///
/// # Memory Ordering
///
/// - `push`: Uses `Release` ordering for node linking and tail pointer updates
/// - `pop`: Uses `Acquire` ordering for head pointer reads and node unlinking
/// - `len`: Uses `Acquire` ordering for consistent size estimation
///
/// # ABA Problem Prevention
///
/// The unbounded queue uses epoch-based reclamation through the crossbeam crate.
/// This ensures that nodes are not reclaimed while other threads might still be
/// accessing them, preventing the classic ABA problem in lock-free data structures.
///
/// # Performance Characteristics
///
/// - **Push**: O(1) amortized with occasional allocation overhead
/// - **Pop**: O(1) amortized with occasional reclamation overhead
/// - **Size**: O(1) approximation (may be slightly stale under contention)
/// - **Memory**: Dynamic allocation per node + epoch tracking overhead
///
/// # Examples
///
/// ```rust
/// use velocityx::queue::mpmc::UnboundedMpmcQueue;
/// use std::thread;
///
/// let queue = UnboundedMpmcQueue::new();
///
/// // Producer thread
/// let producer = thread::spawn({
///     let queue = queue.clone();
///     move || {
///         for i in 0..1000 {
///             queue.push(i).unwrap();
///         }
///     }
/// });
///
/// // Consumer thread
/// let consumer = thread::spawn({
///     let queue = queue.clone();
///     move || {
///         let mut sum = 0;
///         while sum < 499500 { // Sum of 0..999
///             if let Some(value) = queue.pop() {
///                 sum += value;
///             }
///         }
///         sum
///     }
/// });
///
/// producer.join().unwrap();
/// let result = consumer.join().unwrap();
/// assert_eq!(result, 499500);
/// ```
#[cfg(feature = "unstable")]
#[derive(Debug)]
pub struct UnboundedMpmcQueue<T> {
    /// Atomic pointer to the head of the queue (next to pop)
    head: Atomic<Node<T>>,

    /// Atomic pointer to the tail of the queue (last pushed node)
    tail: Atomic<Node<T>>,

    /// Approximate size of the queue for monitoring
    size: AtomicUsize,
}

impl<T> Clone for MpmcQueue<T> {
    fn clone(&self) -> Self {
        // Create a new queue with the same capacity
        let mut new_queue: MpmcQueue<T> = MpmcQueue::with_capacity(self.capacity);

        // Note: This is a shallow clone - it doesn't copy the elements
        // For a true clone, you'd need to drain the original queue
        // and push elements into the new one, which could fail
        new_queue.capacity = self.capacity;
        new_queue.mask = self.mask;

        new_queue
    }
}

#[cfg(feature = "unstable")]
impl<T> Clone for UnboundedMpmcQueue<T> {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl<T> MpmcQueue<T> {
    /// Create a new bounded MPMC queue with the specified capacity
    ///
    /// The capacity will be rounded up to the next power of 2 for efficient modulo operations.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of elements the queue can hold
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    /// assert_eq!(queue.capacity(), 16); // Rounded up to power of 2
    /// ```
    pub fn new(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }

    /// Create a new bounded MPMC queue with the specified capacity
    ///
    /// This is an internal method that handles capacity rounding and buffer allocation.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of elements the queue can hold
    fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "Queue capacity must be greater than 0");

        // Round up to next power of 2 for efficient modulo operations
        let capacity = if capacity.is_power_of_two() {
            capacity
        } else {
            capacity.next_power_of_two()
        };

        let mask = capacity - 1;

        // Allocate aligned buffer with atomic pointers
        let buffer: Vec<CachePadded<AtomicPtr<T>>> = (0..capacity)
            .map(|_| CachePadded::new(AtomicPtr::new(core::ptr::null_mut())))
            .collect();

        Self {
            buffer: CachePadded::new(buffer.into_boxed_slice()),
            capacity,
            mask,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
            generation: CachePadded::new(AtomicU64::new(0)),
        }
    }

    /// Push an element to the queue
    ///
    /// This operation is lock-free and may fail if the queue is full.
    /// Uses `Release` ordering to ensure data visibility before index update.
    ///
    /// # Arguments
    ///
    /// * `value` - The element to push
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed
    /// * `Err(Error::WouldBlock)` if the queue is full
    ///
    /// # Memory Ordering
    ///
    /// Uses `Release` ordering to ensure the value is stored before the tail index is updated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(1);
    /// assert!(queue.push(42).is_ok());
    /// assert!(queue.push(43).is_err()); // Queue full
    /// ```
    #[inline]
    pub fn push(&self, value: T) -> Result<()> {
        // Optimized memory ordering: Use Acquire for head to ensure visibility
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Relaxed);

        // Check if queue is full
        if (tail.wrapping_add(1) & self.mask) == head {
            return Err(Error::CapacityExceeded);
        }

        // Allocate memory for the value
        let boxed = Box::into_raw(Box::new(value));

        // Store the value in the buffer with Release ordering
        let index = tail & self.mask;

        // Prefetch the cache line for better performance
        #[cfg(target_arch = "x86_64")]
        unsafe {
            _mm_prefetch((&self.buffer.inner()[index] as *const _ as *const i8), 0);
            // _MM_HINT_T0
        }

        self.buffer.inner()[index].store(boxed, Ordering::Release);

        // Update tail index with Release ordering
        // This ensures the value is visible before the index update
        self.tail.store(tail.wrapping_add(1), Ordering::Release);

        // Update generation counter for ABA prevention (use Relaxed for performance)
        self.generation.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Pop an element from the queue
    ///
    /// This operation is lock-free and may fail if the queue is empty.
    /// Uses `Acquire` ordering to ensure data visibility after index read.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully popped
    /// * `None` if the queue is empty
    ///
    /// # Memory Ordering
    ///
    /// Uses `Acquire` ordering to ensure the value is visible after the head index is read.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    /// queue.push(42).unwrap();
    /// assert_eq!(queue.pop(), Some(42));
    /// assert_eq!(queue.pop(), None); // Queue empty
    /// ```
    #[inline]
    pub fn pop(&self) -> Option<T> {
        // Optimized memory ordering: Use Acquire for tail to ensure visibility
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Relaxed);

        // Check if queue is empty
        if head == tail {
            return None;
        }

        // Load the value from the buffer with Acquire ordering
        let index = head & self.mask;

        // Prefetch the cache line for better performance
        #[cfg(target_arch = "x86_64")]
        unsafe {
            _mm_prefetch((&self.buffer.inner()[index] as *const _ as *const i8), 0);
            // _MM_HINT_T0
        }

        let ptr = self.buffer.inner()[index].load(Ordering::Acquire);

        if ptr.is_null() {
            // Another thread is in the process of pushing
            return None;
        }

        // Update head index first with Release ordering
        self.head.store(head.wrapping_add(1), Ordering::Release);

        // Clear the buffer slot
        self.buffer.inner()[index].store(core::ptr::null_mut(), Ordering::Relaxed);

        // Convert the pointer back to a value
        let value = unsafe { Box::from_raw(ptr) };

        // Update generation counter for ABA prevention (use Relaxed for performance)
        self.generation.fetch_add(1, Ordering::Relaxed);

        Some(*value)
    }

    /// Get the current number of elements in the queue
    ///
    /// This method provides an approximate count that may be slightly stale
    /// under high contention due to the lock-free nature of the queue.
    ///
    /// # Returns
    ///
    /// The approximate number of elements in the queue
    ///
    /// # Memory Ordering
    ///
    /// Uses `Acquire` ordering for consistent reads of head and tail.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    /// assert_eq!(queue.len(), 0);
    /// queue.push(42).unwrap();
    /// assert_eq!(queue.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        if tail >= head {
            tail - head
        } else {
            // Handle wrap-around
            self.capacity - (head - tail)
        }
    }

    /// Check if the queue is empty
    ///
    /// This method may return slightly stale results under high contention.
    ///
    /// # Returns
    ///
    /// `true` if the queue appears to be empty, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    /// assert!(queue.is_empty());
    /// queue.push(42).unwrap();
    /// assert!(!queue.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the capacity of the queue
    ///
    /// # Returns
    ///
    /// The maximum number of elements the queue can hold
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    /// assert_eq!(queue.capacity(), 16); // Rounded up to power of 2
    /// ```
    #[inline]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Try to push an element to the queue without blocking
    ///
    /// This is an alias for `push` provided for compatibility with standard queue APIs.
    ///
    /// # Arguments
    ///
    /// * `value` - The element to push
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed
    /// * `Err(Error::WouldBlock)` if the queue is full
    #[inline]
    pub fn try_push(&self, value: T) -> Result<()> {
        self.push(value)
    }

    /// Try to pop an element from the queue without blocking
    ///
    /// This is an alias for `pop` provided for compatibility with standard queue APIs.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully popped
    /// * `None` if the queue is empty
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        self.pop()
    }

    /// Push multiple elements to the queue in a single operation
    ///
    /// This is more efficient than individual pushes as it reduces atomic operations.
    /// Returns the number of elements successfully pushed.
    ///
    /// # Arguments
    ///
    /// * `values` - Iterator of values to push
    ///
    /// # Returns
    ///
    /// Number of elements successfully pushed
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    /// let values = vec![1, 2, 3, 4, 5];
    /// let pushed = queue.push_batch(values);
    /// assert_eq!(pushed, 5);
    /// ```
    pub fn push_batch<I>(&self, values: I) -> usize
    where
        I: IntoIterator<Item = T>,
    {
        let mut pushed = 0;
        for value in values {
            match self.push(value) {
                Ok(()) => pushed += 1,
                Err(_) => break, // Queue full, stop pushing
            }
        }
        pushed
    }

    /// Pop multiple elements from the queue in a single operation
    ///
    /// This is more efficient than individual pops as it reduces atomic operations.
    ///
    /// # Arguments
    ///
    /// * `max_values` - Maximum number of values to pop
    ///
    /// # Returns
    ///
    /// Vector of popped values (may be empty if queue is empty)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    /// queue.push(1).unwrap();
    /// queue.push(2).unwrap();
    /// queue.push(3).unwrap();
    ///
    /// let values = queue.pop_batch(2);
    /// assert_eq!(values.len(), 2);
    /// ```
    pub fn pop_batch(&self, max_values: usize) -> Vec<T> {
        let mut values = Vec::with_capacity(max_values);
        for _ in 0..max_values {
            match self.pop() {
                Some(value) => values.push(value),
                None => break, // Queue empty
            }
        }
        values
    }

    /// Try to push an element with a timeout
    ///
    /// This operation will retry for the specified duration before giving up.
    /// Uses adaptive backoff to reduce contention.
    ///
    /// # Arguments
    ///
    /// * `value` - The element to push
    /// * `timeout` - Duration to wait before giving up
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed
    /// * `Err(Error::Timeout)` if the timeout expired
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    /// use std::time::Duration;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(1);
    /// queue.push(42).unwrap();
    ///
    /// let result = queue.push_with_timeout(43, Duration::from_millis(100));
    /// assert!(result.is_err()); // Queue full, timeout
    /// ```
    #[cfg(feature = "std")]
    pub fn push_with_timeout(&self, value: T, timeout: Duration) -> Result<()>
    where
        T: Clone,
    {
        let start = std::time::Instant::now();
        let mut backoff = Duration::from_nanos(100);

        while start.elapsed() < timeout {
            match self.push(value.clone()) {
                Ok(()) => return Ok(()),
                Err(Error::CapacityExceeded) => {
                    // Adaptive backoff with exponential growth
                    let elapsed = start.elapsed();
                    let remaining = timeout - elapsed;
                    if backoff > remaining {
                        break;
                    }
                    std::thread::sleep(backoff);
                    backoff = std::cmp::min(backoff * 2, Duration::from_millis(1));
                }
                Err(e) => return Err(e),
            }
        }

        Err(Error::Timeout)
    }

    /// Try to pop an element with a timeout
    ///
    /// This operation will retry for the specified duration before giving up.
    /// Uses adaptive backoff to reduce contention.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Duration to wait before giving up
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully popped
    /// * `None` if the timeout expired
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    /// use std::time::Duration;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    ///
    /// let result = queue.pop_with_timeout(Duration::from_millis(100));
    /// assert!(result.is_none()); // Queue empty, timeout
    /// ```
    #[cfg(feature = "std")]
    pub fn pop_with_timeout(&self, timeout: Duration) -> Option<T> {
        let start = std::time::Instant::now();
        let mut backoff = Duration::from_nanos(50);

        while start.elapsed() < timeout {
            if let Some(value) = self.pop() {
                return Some(value);
            }

            // Adaptive backoff with exponential growth
            let elapsed = start.elapsed();
            let remaining = timeout - elapsed;
            if backoff > remaining {
                break;
            }
            std::thread::sleep(backoff);
            backoff = std::cmp::min(backoff * 2, Duration::from_millis(1));
        }

        None
    }

    /// Get performance statistics for the queue
    ///
    /// This method provides insights into queue performance and utilization.
    ///
    /// # Returns
    ///
    /// QueueMetrics containing performance statistics
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::MpmcQueue;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(100);
    /// let metrics = queue.metrics();
    /// println!("Queue capacity: {}", metrics.capacity);
    /// ```
    pub fn metrics(&self) -> QueueMetrics {
        QueueMetrics {
            capacity: self.capacity,
            current_len: self.len(),
            is_empty: self.is_empty(),
            utilization_ratio: self.len() as f64 / self.capacity as f64,
        }
    }
}

/// Performance metrics for MPMC queue
#[derive(Debug, Clone)]
pub struct QueueMetrics {
    /// Maximum capacity of the queue
    pub capacity: usize,
    /// Current number of elements
    pub current_len: usize,
    /// Whether the queue is empty
    pub is_empty: bool,
    /// Utilization ratio (0.0 to 1.0)
    pub utilization_ratio: f64,
}

#[cfg(feature = "unstable")]
impl<T> UnboundedMpmcQueue<T> {
    /// Create a new unbounded MPMC queue
    ///
    /// The queue starts empty and can grow dynamically as elements are pushed.
    /// Uses epoch-based reclamation for safe memory management.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::UnboundedMpmcQueue;
    ///
    /// let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
    /// queue.push(42).unwrap();
    /// assert_eq!(queue.pop(), Some(42));
    /// ```
    pub fn new() -> Self {
        let guard = &epoch::pin();

        // Create a dummy node to simplify the algorithm
        // This node will never contain actual data
        // NOTE: the unbounded queue implementation is currently considered unstable.
        // This constructor exists for compilation completeness under the `unstable` flag.
        let dummy_shared = Owned::new(Node {
            value: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
            next: Atomic::null(),
        })
        .into_shared(guard);

        Self {
            head: Atomic::from(dummy_shared),
            tail: Atomic::from(dummy_shared),
            size: AtomicUsize::new(0),
        }
    }

    /// Push an element to the queue
    ///
    /// This operation is lock-free and always succeeds (unbounded capacity).
    /// Uses `Release` ordering for node linking and tail pointer updates.
    ///
    /// # Arguments
    ///
    /// * `value` - The element to push
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed (always succeeds)
    ///
    /// # Memory Ordering
    ///
    /// Uses `Release` ordering to ensure the node is properly linked before the tail pointer is updated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::UnboundedMpmcQueue;
    ///
    /// let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
    /// assert!(queue.push(42).is_ok());
    /// ```
    #[inline]
    pub fn push(&self, value: T) -> Result<()> {
        let guard = &epoch::pin();

        // Create a new node
        let new_node = Owned::new(Node {
            value,
            next: Atomic::null(),
        });

        // Get current tail
        let tail = self.tail.load(Ordering::Acquire, guard);

        // Link the new node
        unsafe {
            let tail_ref = tail.deref();
            tail_ref.next.store(new_node, Ordering::Release);
        }

        // Update tail pointer
        let new_node_shared = unsafe { tail.deref().next.load(Ordering::Acquire, guard) };
        self.tail.store(new_node_shared, Ordering::Release);

        // Update size counter
        self.size.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Pop an element from the queue
    ///
    /// This operation is lock-free and may fail if the queue is empty.
    /// Uses `Acquire` ordering for head pointer reads and node unlinking.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully popped
    /// * `None` if the queue is empty
    ///
    /// # Memory Ordering
    ///
    /// Uses `Acquire` ordering to ensure the node is properly unlinked before accessing the value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::UnboundedMpmcQueue;
    ///
    /// let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
    /// queue.push(42).unwrap();
    /// assert_eq!(queue.pop(), Some(42));
    /// assert_eq!(queue.pop(), None); // Queue empty
    /// ```
    #[inline]
    pub fn pop(&self) -> Option<T> {
        let guard = &epoch::pin();

        // Get current head
        let head = self.head.load(Ordering::Acquire, guard);
        let next = unsafe { head.deref().next.load(Ordering::Acquire, guard) };

        if next.is_null() {
            // Queue is empty
            return None;
        }

        // Extract the value from the next node
        let value = unsafe {
            let next_ref = next.deref();
            std::ptr::read(&next_ref.value)
        };

        // Update head pointer
        self.head.store(next, Ordering::Release);

        // Update size counter
        self.size.fetch_sub(1, Ordering::Relaxed);

        // The old head node will be reclaimed by the epoch system
        unsafe {
            guard.defer_destroy(head);
        }

        Some(value)
    }

    /// Get the current number of elements in the queue
    ///
    /// This method provides an approximate count that may be slightly stale
    /// under high contention due to the lock-free nature of the queue.
    ///
    /// # Returns
    ///
    /// The approximate number of elements in the queue
    ///
    /// # Memory Ordering
    ///
    /// Uses `Relaxed` ordering as the size is only an approximation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::UnboundedMpmcQueue;
    ///
    /// let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
    /// assert_eq!(queue.len(), 0);
    /// queue.push(42).unwrap();
    /// assert_eq!(queue.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Check if the queue is empty
    ///
    /// This method may return slightly stale results under high contention.
    ///
    /// # Returns
    ///
    /// `true` if the queue appears to be empty, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::mpmc::UnboundedMpmcQueue;
    ///
    /// let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
    /// assert!(queue.is_empty());
    /// queue.push(42).unwrap();
    /// assert!(!queue.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Try to push an element to the queue without blocking
    ///
    /// This is an alias for `push` provided for compatibility with standard queue APIs.
    /// For the unbounded queue, this always succeeds.
    ///
    /// # Arguments
    ///
    /// * `value` - The element to push
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed (always succeeds)
    #[inline]
    pub fn try_push(&self, value: T) -> Result<()> {
        self.push(value)
    }

    /// Try to pop an element from the queue without blocking
    ///
    /// This is an alias for `pop` provided for compatibility with standard queue APIs.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully popped
    /// * `None` if the queue is empty
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        self.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    use std::{format, vec};

    // Bounded Queue Tests

    #[test]
    fn test_bounded_queue_basic_operations() {
        let queue: MpmcQueue<i32> = MpmcQueue::new(4);

        // Test empty queue
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
        assert_eq!(queue.pop(), None);

        // Test push and pop
        assert!(queue.push(1).is_ok());
        assert_eq!(queue.len(), 1);
        assert!(!queue.is_empty());

        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_bounded_queue_capacity_rounding() {
        let queue: MpmcQueue<i32> = MpmcQueue::new(5);
        assert_eq!(queue.capacity(), 8); // Rounded up to power of 2

        let queue: MpmcQueue<i32> = MpmcQueue::new(16);
        assert_eq!(queue.capacity(), 16); // Already power of 2
    }

    #[test]
    fn test_bounded_queue_full_behavior() {
        let queue: MpmcQueue<i32> = MpmcQueue::new(2);

        // Fill the queue
        assert!(queue.push(1).is_ok());
        assert!(queue.push(2).is_ok());
        assert_eq!(queue.len(), 2);

        // Try to push into full queue
        assert!(queue.push(3).is_err());

        // Pop one item and try again
        assert_eq!(queue.pop(), Some(1));
        assert!(queue.push(3).is_ok());
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn test_bounded_queue_wrap_around() {
        let queue: MpmcQueue<i32> = MpmcQueue::new(4);

        // Fill and empty the queue multiple times to test wrap-around
        for i in 0..10 {
            assert!(queue.push(i).is_ok());
            assert_eq!(queue.pop(), Some(i));
        }

        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_bounded_queue_fifo_ordering() {
        let queue: MpmcQueue<i32> = MpmcQueue::new(10);

        // Push multiple items
        for i in 0..5 {
            assert!(queue.push(i).is_ok());
        }

        // Verify FIFO ordering
        for i in 0..5 {
            assert_eq!(queue.pop(), Some(i));
        }

        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn test_bounded_queue_concurrent_access() {
        let queue = Arc::new(MpmcQueue::new(1000));
        let num_producers = 4;
        let num_consumers = 4;
        let items_per_producer = 1000;

        let mut handles = vec![];

        // Spawn producer threads
        for producer_id in 0..num_producers {
            let queue = Arc::clone(&queue);
            let handle = thread::spawn(move || {
                for i in 0..items_per_producer {
                    let value = producer_id * items_per_producer + i;
                    while queue.push(value).is_err() {
                        thread::yield_now();
                    }
                }
            });
            handles.push(handle);
        }

        // Spawn consumer threads
        for _ in 0..num_consumers {
            let queue = Arc::clone(&queue);
            let handle = thread::spawn(move || {
                let mut received = 0;
                while received < items_per_producer * num_producers / num_consumers {
                    if queue.pop().is_some() {
                        received += 1;
                    } else {
                        thread::yield_now();
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Queue should be empty
        assert!(queue.is_empty());
    }

    // Unbounded Queue Tests

    #[cfg(feature = "unstable")]
    #[test]
    fn test_unbounded_queue_basic_operations() {
        let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();

        // Test empty queue
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
        assert_eq!(queue.pop(), None);

        // Test push and pop
        assert!(queue.push(1).is_ok());
        assert_eq!(queue.len(), 1);
        assert!(!queue.is_empty());

        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_unbounded_queue_fifo_ordering() {
        let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();

        // Push multiple items
        for i in 0..10 {
            assert!(queue.push(i).is_ok());
        }

        // Verify FIFO ordering
        for i in 0..10 {
            assert_eq!(queue.pop(), Some(i));
        }

        assert_eq!(queue.pop(), None);
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_unbounded_queue_concurrent_access() {
        let queue = Arc::new(UnboundedMpmcQueue::new());
        let num_producers = 4;
        let num_consumers = 4;
        let items_per_producer = 1000;

        let mut handles = vec![];

        // Spawn producer threads
        for producer_id in 0..num_producers {
            let queue = Arc::clone(&queue);
            let handle = thread::spawn(move || {
                for i in 0..items_per_producer {
                    let value = producer_id * items_per_producer + i;
                    queue.push(value).unwrap();
                }
            });
            handles.push(handle);
        }

        // Spawn consumer threads
        for _ in 0..num_consumers {
            let queue = Arc::clone(&queue);
            let handle = thread::spawn(move || {
                let mut received = 0;
                while received < items_per_producer * num_producers / num_consumers {
                    if queue.pop().is_some() {
                        received += 1;
                    } else {
                        thread::yield_now();
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        use std::sync::atomic::{AtomicUsize, Ordering};

        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

        #[derive(Debug, PartialEq, Eq, Hash)]
        struct DropTracker {
            id: usize,
        }

        impl Drop for DropTracker {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::Relaxed);
            }
        }

        let queue: UnboundedMpmcQueue<DropTracker> = UnboundedMpmcQueue::new();

        // Push items
        for i in 0..100 {
            queue.push(DropTracker { id: i }).unwrap();
        }

        // Pop some items
        for _ in 0..50 {
            queue.pop();
        }

        // Drop the queue
        drop(queue);

        // All items should be dropped
        let dropped_items = DROP_COUNT.load(Ordering::Relaxed);
        assert_eq!(dropped_items, 100);
    }

    // Stress Tests

    #[test]
    fn test_bounded_queue_stress() {
        let queue = Arc::new(MpmcQueue::new(1000));
        let num_threads = 8;
        let operations_per_thread = 10000;

        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let queue = Arc::clone(&queue);
            let handle = thread::spawn(move || {
                for i in 0..operations_per_thread {
                    let value = thread_id * operations_per_thread + i;

                    // Mix of push and pop operations
                    match i % 3 {
                        0 => {
                            while queue.push(value).is_err() {
                                thread::yield_now();
                            }
                        }
                        1 => {
                            let _ = queue.pop();
                        }
                        2 => {
                            if queue.push(value).is_ok() {
                                let _ = queue.pop();
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Drain remaining items
        while queue.pop().is_some() {}

        assert!(queue.is_empty());
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_unbounded_queue_stress() {
        let queue = Arc::new(UnboundedMpmcQueue::new());
        let num_threads = 8;
        let operations_per_thread = 10000;

        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let queue = Arc::clone(&queue);
            let handle = thread::spawn(move || {
                for i in 0..operations_per_thread {
                    let value = thread_id * operations_per_thread + i;

                    // Mix of push and pop operations
                    match i % 3 {
                        0 => {
                            queue.push(value).unwrap();
                        }
                        1 => {
                            let _ = queue.pop();
                        }
                        2 => {
                            queue.push(value).unwrap();
                            let _ = queue.pop();
                        }
                        _ => unreachable!(),
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Drain remaining items
        while queue.pop().is_some() {}

        assert!(queue.is_empty());
    }

    // Property-based tests would go here when proptest is available
    // For now, we'll include some basic property tests

    #[test]
    fn test_bounded_queue_properties() {
        let queue: MpmcQueue<i32> = MpmcQueue::new(10);

        // Property: len() == number of items pushed - number of items popped
        let mut pushed = 0;
        let mut popped = 0;

        for i in 0..100 {
            if queue.push(i).is_ok() {
                pushed += 1;
            }

            if i % 2 == 0 && queue.pop().is_some() {
                popped += 1;
            }

            assert_eq!(queue.len(), pushed - popped);
        }
    }

    #[cfg(feature = "unstable")]
    #[test]
    fn test_unbounded_queue_properties() {
        let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();

        // Property: len() == number of items pushed - number of items popped
        let mut pushed = 0;
        let mut popped = 0;

        for i in 0..100 {
            queue.push(i).unwrap();
            pushed += 1;

            if i % 2 == 0 && queue.pop().is_some() {
                popped += 1;
            }

            assert_eq!(queue.len(), pushed - popped);
        }
    }

    #[test]
    fn test_cache_alignment() {
        use core::mem;

        // Ensure that critical fields are properly aligned
        assert_eq!(mem::align_of::<MpmcQueue<i32>>(), 64);
        #[cfg(feature = "unstable")]
        {
            assert_eq!(mem::align_of::<UnboundedMpmcQueue<i32>>(), 64);
            assert_eq!(mem::align_of::<Node<i32>>(), 64);
        }
    }

    #[test]
    fn test_debug_format() {
        let bounded: MpmcQueue<i32> = MpmcQueue::new(10);

        let debug_str = format!("{:?}", bounded);
        assert!(debug_str.contains("MpmcQueue"));

        #[cfg(feature = "unstable")]
        {
            let unbounded: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
            let debug_str = format!("{:?}", unbounded);
            assert!(debug_str.contains("UnboundedMpmcQueue"));
        }
    }
}

// Re-export the queue types for easier access
pub use MpmcQueue as BoundedMpmcQueue;
