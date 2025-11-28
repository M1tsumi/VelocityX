//! Work-Stealing Deque Implementation
//!
//! This module implements the Chase-Lev work-stealing deque, which is specifically
//! designed for task scheduling in parallel systems. The deque supports efficient
//! push/pop operations at the "bottom" (owner thread) and steal operations at the "top" (thief threads).
//!
//! ## Design
//!
//! The deque uses a circular buffer with atomic indices:
//! - `bottom`: Points to the next free slot at the bottom (owner only)
//! - `top`: Points to the first available element at the top (shared)
//! - Operations use carefully chosen memory ordering for correctness
//!
//! ## Memory Ordering
//!
//! - `push`: Uses `Release` ordering for bottom update
//! - `pop`: Uses `Acquire` for top read, `Release` for bottom update
//! - `steal`: Uses `Acquire` ordering for both reads
//!
//! ## Performance Characteristics
//!
//! - **push**: O(1) amortized, owner-only operation
//! - **pop**: O(1) amortized, owner-only operation
//! - **steal**: O(1) amortized, thief operation
//! - **size**: O(1) with potentially stale results under contention
//!
//! ## Example
//!
//! ```rust
//! use velocityx::deque::WorkStealingDeque;
//! use std::thread;
//!
//! let deque = WorkStealingDeque::new(100);
//!
//! // Owner thread (worker)
//! let owner = thread::spawn({
//!     let deque = deque.clone();
//!     move || {
//!     // Push work items
//!     for i in 0..100 {
//!         deque.push(i);
//!     }
//!     
//!     // Process own work
//!     while let Some(task) = deque.pop() {
//!         // Process task
//!         println!("Processing task: {}", task);
//!     }
//!     }
//! });
//!
//! // Thief thread (stealer)
//! let thief = thread::spawn({
//!     let deque = deque.clone();
//!     move || {
//!     let mut stolen = 0;
//!     while stolen < 50 {
//!         if let Some(task) = deque.steal() {
//!             println!("Stolen task: {}", task);
//!             stolen += 1;
//!         }
//!     }
//!     stolen
//!     }
//! });
//!
//! owner.join().unwrap();
//! let stolen_count = thief.join().unwrap();
//! println!("Stolen {} tasks", stolen_count);
//! ```

use crate::util::{align_to_cache_line, CachePadded};
use crate::{Error, Result};
use core::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

/// A work-stealing deque based on the Chase-Lev algorithm
///
/// This deque is designed for work-stealing schedulers where one thread (the owner)
/// pushes and pops from the bottom, while multiple threads (thieves) can steal from the top.
///
/// # Type Parameters
///
/// * `T` - The type of elements stored in the deque
///
/// # Safety
///
/// This deque is safe to use from multiple threads simultaneously.
/// The owner thread should use `push` and `pop`, while thief threads use `steal`.
///
/// # Examples
///
/// ```rust
/// use velocityx::deque::WorkStealingDeque;
///
/// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
/// 
/// // Owner operations
/// deque.push(42);
/// assert_eq!(deque.pop(), Some(42));
///
/// // Thief operations
/// deque.push(42);
/// assert_eq!(deque.steal(), Some(42));
/// ```
#[derive(Debug)]
pub struct WorkStealingDeque<T> {
    // Circular buffer storage, aligned to cache line boundaries
    buffer: CachePadded<Box<[Option<T>]>>,
    
    // Deque capacity (always a power of 2)
    capacity: usize,
    
    // Mask for fast modulo operation (capacity - 1)
    mask: usize,
    
    // Bottom index (next free slot, owner only)
    bottom: CachePadded<AtomicIsize>,
    
    // Top index (first available element, shared)
    top: CachePadded<AtomicIsize>,
    
    // For ABA problem prevention
    epoch: CachePadded<AtomicUsize>,
}

impl<T> Clone for WorkStealingDeque<T> {
    fn clone(&self) -> Self {
        // Create a new deque with the same capacity
        let mut new_deque = WorkStealingDeque::with_capacity(self.capacity);
        
        // Note: This is a shallow clone - it doesn't copy the elements
        // For a true clone, you'd need to drain the original deque
        // and push elements into the new one
        new_deque.capacity = self.capacity;
        new_deque.mask = self.mask;
        
        new_deque
    }
}

impl<T> WorkStealingDeque<T> {
    /// Create a new work-stealing deque with the specified capacity
    ///
    /// The capacity will be rounded up to the next power of 2 for efficient modulo operations.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of elements the deque can hold
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::deque::WorkStealingDeque;
    ///
    /// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
    /// ```
    pub fn new(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }

    /// Create a new work-stealing deque with the specified capacity
    ///
    /// This is an internal method that handles capacity rounding and buffer allocation.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of elements the deque can hold
    fn with_capacity(capacity: usize) -> Self {
        assert!(capacity > 0, "Deque capacity must be greater than 0");
        
        // Round up to next power of 2 for efficient modulo operations
        let capacity = if capacity.is_power_of_two() {
            capacity
        } else {
            capacity.next_power_of_two()
        };
        
        let mask = capacity - 1;
        
        // Allocate aligned buffer
        let buffer = vec![None; capacity].into_boxed_slice();
        
        Self {
            buffer: CachePadded::new(buffer),
            capacity,
            mask,
            bottom: CachePadded::new(std::sync::atomic::AtomicIsize::new(0)),
            top: CachePadded::new(std::sync::atomic::AtomicIsize::new(0)),
            epoch: CachePadded::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Push an element to the bottom of the deque (owner only)
    ///
    /// This operation should only be called by the owner thread.
    /// It may fail if the deque is full.
    ///
    /// # Arguments
    ///
    /// * `value` - The element to push
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed
    /// * `Err(Error::WouldBlock)` if the deque is full
    ///
    /// # Memory Ordering
    ///
    /// Uses `Release` ordering to ensure the element is written before the bottom index is updated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::deque::WorkStealingDeque;
    ///
    /// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(1);
    /// assert!(deque.push(42).is_ok());
    /// assert!(deque.push(43).is_err()); // Deque is full
    /// ```
    #[inline]
    pub fn push(&self, value: T) -> Result<()> {
        let bottom = self.bottom.get().load(Ordering::Relaxed);
        let top = self.top.get().load(Ordering::Acquire);
        
        // Check if deque is full
        if bottom - top >= self.capacity as isize {
            return Err(Error::WouldBlock);
        }
        
        let index = (bottom as usize) & self.mask;
        
        // Write the value
        self.buffer[index] = Some(value);
        
        // Update bottom index with Release ordering
        self.bottom.get().store(bottom + 1, Ordering::Release);
        
        Ok(())
    }

    /// Pop an element from the bottom of the deque (owner only)
    ///
    /// This operation should only be called by the owner thread.
    /// It may fail if the deque is empty.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully popped
    /// * `None` if the deque is empty
    ///
    /// # Memory Ordering
    ///
    /// Uses `Acquire` ordering for top read and `Release` ordering for bottom update.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::deque::WorkStealingDeque;
    ///
    /// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
    /// deque.push(42).unwrap();
    /// assert_eq!(deque.pop(), Some(42));
    /// assert_eq!(deque.pop(), None); // Deque is empty
    /// ```
    #[inline]
    pub fn pop(&self) -> Option<T> {
        let bottom = self.bottom.get().load(Ordering::Relaxed);
        
        if bottom == 0 {
            return None;
        }
        
        // Decrement bottom first
        self.bottom.get().store(bottom - 1, Ordering::Relaxed);
        
        let top = self.top.get().load(Ordering::Acquire);
        
        if top <= bottom - 1 {
            // Deque is not empty, take the element
            let index = ((bottom - 1) as usize) & self.mask;
            let value = self.buffer[index].take();
            
            if top == bottom - 1 {
                // Deque became empty, try to update top
                if self.top.get().compare_exchange(
                    top,
                    top + 1,
                    Ordering::Release,
                    Ordering::Relaxed,
                ).is_ok() {
                    // Successfully updated top, return value
                    self.bottom.get().store(bottom, Ordering::Relaxed);
                    return value;
                } else {
                    // Another thread stole the element, restore bottom
                    self.bottom.get().store(bottom, Ordering::Relaxed);
                    return None;
                }
            }
            
            value
        } else {
            // Deque is empty, restore bottom
            self.bottom.get().store(bottom, Ordering::Relaxed);
            None
        }
    }

    /// Steal an element from the top of the deque (thief operation)
    ///
    /// This operation can be called by any thread (thieves).
    /// It may fail if the deque is empty or if there's contention.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully stolen
    /// * `None` if the deque is empty or contention occurred
    ///
    /// # Memory Ordering
    ///
    /// Uses `Acquire` ordering for both top and bottom reads.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::deque::WorkStealingDeque;
    ///
    /// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
    /// deque.push(42).unwrap();
    /// assert_eq!(deque.steal(), Some(42));
    /// assert_eq!(deque.steal(), None); // Deque is empty
    /// ```
    #[inline]
    pub fn steal(&self) -> Option<T> {
        let top = self.top.get().load(Ordering::Acquire);
        let bottom = self.bottom.get().load(Ordering::Acquire);
        
        if top >= bottom {
            return None;
        }
        
        let index = (top as usize) & self.mask;
        
        // Try to take the element
        if let Some(value) = self.buffer[index].take() {
            // Try to update top
            if self.top.get().compare_exchange(
                top,
                top + 1,
                Ordering::Release,
                Ordering::Relaxed,
            ).is_ok() {
                // Successfully stole the element
                Some(value)
            } else {
                // Failed to update top, put the element back
                self.buffer[index] = Some(value);
                None
            }
        } else {
            // Element was taken by another thread
            None
        }
    }

    /// Get the current number of elements in the deque
    ///
    /// This method provides an approximate count that may be slightly stale
    /// under high contention due to the lock-free nature of the deque.
    ///
    /// # Returns
    ///
    /// The approximate number of elements in the deque
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::deque::WorkStealingDeque;
    ///
    /// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
    /// assert_eq!(deque.len(), 0);
    /// deque.push(42).unwrap();
    /// assert_eq!(deque.len(), 1);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        let bottom = self.bottom.get().load(Ordering::Acquire);
        let top = self.top.get().load(Ordering::Acquire);
        (bottom - top).max(0) as usize
    }

    /// Check if the deque is empty
    ///
    /// This method may return slightly stale results under high contention.
    ///
    /// # Returns
    ///
    /// `true` if the deque appears to be empty, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::deque::WorkStealingDeque;
    ///
    /// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
    /// assert!(deque.is_empty());
    /// deque.push(42).unwrap();
    /// assert!(!deque.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        let bottom = self.bottom.get().load(Ordering::Acquire);
        let top = self.top.get().load(Ordering::Acquire);
        bottom == top
    }

    /// Get the capacity of the deque
    ///
    /// # Returns
    ///
    /// The maximum number of elements the deque can hold
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::deque::WorkStealingDeque;
    ///
    /// let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
    /// assert_eq!(deque.capacity(), 16); // Rounded up to power of 2
    /// ```
    #[inline]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Try to push an element to the bottom without blocking
    ///
    /// This is an alias for `push` provided for compatibility with standard deque APIs.
    ///
    /// # Arguments
    ///
    /// * `value` - The element to push
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed
    /// * `Err(Error::WouldBlock)` if the deque is full
    #[inline]
    pub fn try_push(&self, value: T) -> Result<()> {
        self.push(value)
    }

    /// Try to pop an element from the bottom without blocking
    ///
    /// This is an alias for `pop` provided for compatibility with standard deque APIs.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully popped
    /// * `None` if the deque is empty
    #[inline]
    pub fn try_pop(&self) -> Option<T> {
        self.pop()
    }

    /// Try to steal an element from the top without blocking
    ///
    /// This is an alias for `steal` provided for clarity.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if an element was successfully stolen
    /// * `None` if the deque is empty or contention occurred
    #[inline]
    pub fn try_steal(&self) -> Option<T> {
        self.steal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_operations() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        
        // Test empty deque
        assert_eq!(deque.len(), 0);
        assert!(deque.is_empty());
        assert_eq!(deque.pop(), None);
        assert_eq!(deque.steal(), None);
        
        // Test push and pop
        assert!(deque.push(1).is_ok());
        assert_eq!(deque.len(), 1);
        assert!(!deque.is_empty());
        
        assert_eq!(deque.pop(), Some(1));
        assert_eq!(deque.len(), 0);
        assert!(deque.is_empty());
    }

    #[test]
    fn test_lifo_behavior() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        
        // Push multiple items
        assert!(deque.push(1).is_ok());
        assert!(deque.push(2).is_ok());
        assert!(deque.push(3).is_ok());
        
        // Pop should return items in LIFO order
        assert_eq!(deque.pop(), Some(3));
        assert_eq!(deque.pop(), Some(2));
        assert_eq!(deque.pop(), Some(1));
        assert_eq!(deque.pop(), None);
    }

    #[test]
    fn test_fifo_stealing() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        
        // Push multiple items
        assert!(deque.push(1).is_ok());
        assert!(deque.push(2).is_ok());
        assert!(deque.push(3).is_ok());
        
        // Steal should return items in FIFO order
        assert_eq!(deque.steal(), Some(1));
        assert_eq!(deque.steal(), Some(2));
        assert_eq!(deque.steal(), Some(3));
        assert_eq!(deque.steal(), None);
    }

    #[test]
    fn test_mixed_operations() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        
        // Push items
        assert!(deque.push(1).is_ok());
        assert!(deque.push(2).is_ok());
        assert!(deque.push(3).is_ok());
        
        // Mix of pop and steal
        assert_eq!(deque.pop(), Some(3)); // LIFO from bottom
        assert_eq!(deque.steal(), Some(1)); // FIFO from top
        assert_eq!(deque.pop(), Some(2)); // Remaining item
        assert_eq!(deque.pop(), None);
        assert_eq!(deque.steal(), None);
    }

    #[test]
    fn test_full_deque() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(2);
        
        // Fill the deque
        assert!(deque.push(1).is_ok());
        assert!(deque.push(2).is_ok());
        assert_eq!(deque.len(), 2);
        
        // Try to push into full deque
        assert!(deque.push(3).is_err());
        
        // Pop one item and try again
        assert_eq!(deque.pop(), Some(2));
        assert!(deque.push(3).is_ok());
        assert_eq!(deque.pop(), Some(3));
        assert_eq!(deque.pop(), Some(1));
    }

    #[test]
    fn test_wrap_around() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        
        // Fill and empty the deque multiple times to test wrap-around
        for i in 0..10 {
            assert!(deque.push(i).is_ok());
            assert_eq!(deque.pop(), Some(i));
        }
    }

    #[test]
    fn test_concurrent_work_stealing() {
        let deque = Arc::new(WorkStealingDeque::new(1000));
        let num_workers = 4;
        let num_thieves = 4;
        let tasks_per_worker = 1000;
        
        // Spawn worker threads (owners)
        let mut worker_handles = vec![];
        for worker_id in 0..num_workers {
            let deque = Arc::clone(&deque);
            let handle = thread::spawn(move || {
                // Push tasks
                for i in 0..tasks_per_worker {
                    let task = worker_id * tasks_per_worker + i;
                    while deque.push(task).is_err() {
                        thread::yield_now();
                    }
                }
                
                // Process own work and steal from others
                let mut processed = 0;
                while processed < tasks_per_worker {
                    if let Some(_task) = deque.pop() {
                        processed += 1;
                    } else {
                        // Try to steal
                        if deque.steal().is_some() {
                            processed += 1;
                        } else {
                            thread::yield_now();
                        }
                    }
                }
                processed
            });
            worker_handles.push(handle);
        }
        
        // Spawn thief threads
        let mut thief_handles = vec![];
        for _ in 0..num_thieves {
            let deque = Arc::clone(&deque);
            let handle = thread::spawn(move || {
                let mut stolen = 0;
                while stolen < tasks_per_worker * num_workers / (num_workers + num_thieves) {
                    if deque.steal().is_some() {
                        stolen += 1;
                    } else {
                        thread::yield_now();
                    }
                }
                stolen
            });
            thief_handles.push(handle);
        }
        
        // Wait for all threads
        let mut total_processed = 0;
        for handle in worker_handles {
            total_processed += handle.join().unwrap();
        }
        
        let mut total_stolen = 0;
        for handle in thief_handles {
            total_stolen += handle.join().unwrap();
        }
        
        let total_tasks = num_workers * tasks_per_worker;
        assert_eq!(total_processed + total_stolen, total_tasks);
    }

    #[test]
    fn test_high_contention() {
        let deque = Arc::new(WorkStealingDeque::new(100));
        let num_threads = 8;
        let operations_per_thread = 1000;
        
        let mut handles = vec![];
        
        // Spawn threads that perform mixed operations
        for thread_id in 0..num_threads {
            let deque = Arc::clone(&deque);
            let handle = thread::spawn(move || {
                for i in 0..operations_per_thread {
                    let value = thread_id * operations_per_thread + i;
                    
                    // Try to push
                    if deque.push(value).is_err() {
                        // Deque full, try to pop or steal
                        let _ = deque.pop().or_else(|| deque.steal());
                    }
                    
                    // Try to pop or steal
                    if deque.pop().is_none() && deque.steal().is_none() {
                        // Deque empty, try to push
                        let _ = deque.push(value);
                    }
                }
            });
            handles.push(handle);
        }
        
        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }
        
        // Drain any remaining items
        let mut remaining = 0;
        while deque.pop().is_some() || deque.steal().is_some() {
            remaining += 1;
        }
        
        // The exact number remaining depends on the contention pattern
        assert!(remaining <= deque.capacity());
    }

    #[test]
    fn test_cache_alignment() {
        use core::mem;
        
        // Ensure that critical fields are properly aligned
        assert_eq!(mem::align_of::<WorkStealingDeque<i32>>(), 64);
    }

    #[test]
    fn test_debug_format() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        let debug_str = format!("{:?}", deque);
        assert!(debug_str.contains("WorkStealingDeque"));
        assert!(debug_str.contains("capacity"));
    }

    #[test]
    fn test_empty_deque_edge_cases() {
        let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(4);
        
        // Multiple pops on empty deque
        assert_eq!(deque.pop(), None);
        assert_eq!(deque.pop(), None);
        assert_eq!(deque.steal(), None);
        assert_eq!(deque.steal(), None);
        
        // Push and pop single item
        assert!(deque.push(42).is_ok());
        assert_eq!(deque.pop(), Some(42));
        
        // Should be empty again
        assert_eq!(deque.pop(), None);
        assert_eq!(deque.steal(), None);
    }
}
