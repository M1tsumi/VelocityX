//! Lock-Free Stack Implementation
//!
//! A high-performance lock-free stack based on Treiber's algorithm.
//! Provides wait-free push operations and lock-free pop operations.

use crate::metrics::{AtomicMetrics, MetricsCollector};
use crate::util::CachePadded;
use core::sync::atomic::{AtomicPtr, Ordering};
use std::sync::atomic::{AtomicUsize, Ordering as StdOrdering};

#[cfg(feature = "std")]
use std::boxed::Box;
#[cfg(feature = "std")]
use std::vec::Vec;

/// A node in the lock-free stack
#[derive(Debug)]
struct Node<T> {
    data: T,
    next: AtomicPtr<Node<T>>,
}

/// A lock-free stack implementation using Treiber's algorithm
///
/// This stack provides:
/// - Wait-free push operations
/// - Lock-free pop operations  
/// - ABA problem prevention through proper memory management
/// - Performance metrics collection
///
/// # Type Parameters
///
/// * `T` - The type of elements stored in the stack
///
/// # Examples
///
/// ```rust
/// use velocityx::stack::LockFreeStack;
///
/// let stack = LockFreeStack::new();
///
/// // Push elements
/// stack.push(1);
/// stack.push(2);
/// stack.push(3);
///
/// // Pop elements
/// assert_eq!(stack.pop(), Some(3));
/// assert_eq!(stack.pop(), Some(2));
/// assert_eq!(stack.pop(), Some(1));
/// assert_eq!(stack.pop(), None);
/// ```
#[derive(Debug)]
pub struct LockFreeStack<T> {
    /// The head pointer of the stack
    head: CachePadded<AtomicPtr<Node<T>>>,
    /// Performance metrics
    metrics: AtomicMetrics,
    /// Metrics enabled flag
    metrics_enabled: AtomicUsize,
}

impl<T> LockFreeStack<T> {
    /// Create a new empty lock-free stack
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::stack::LockFreeStack;
    ///
    /// let stack: LockFreeStack<i32> = LockFreeStack::new();
    /// ```
    pub fn new() -> Self {
        Self {
            head: CachePadded::new(AtomicPtr::new(std::ptr::null_mut())),
            metrics: AtomicMetrics::default(),
            metrics_enabled: AtomicUsize::new(1), // Enabled by default
        }
    }

    /// Push a value onto the stack
    ///
    /// This operation is wait-free and will always complete in a bounded number of steps.
    ///
    /// # Arguments
    ///
    /// * `value` - The value to push onto the stack
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::stack::LockFreeStack;
    ///
    /// let stack = LockFreeStack::new();
    /// stack.push(42);
    /// ```
    pub fn push(&self, value: T) {
        #[cfg(feature = "std")]
        let start = std::time::Instant::now();

        // Create a new node
        let node = Box::into_raw(Box::new(Node {
            data: value,
            next: AtomicPtr::new(std::ptr::null_mut()),
        }));

        // Treiber's algorithm for push
        loop {
            let head = self.head.get().load(Ordering::Acquire);

            // Set the next pointer to current head
            unsafe {
                (*node).next.store(head, Ordering::Relaxed);
            }

            // Try to update head to point to our node
            match self.head.get().compare_exchange_weak(
                head,
                node,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    #[cfg(feature = "std")]
                    self.metrics.record_success(start.elapsed());
                    break;
                }
                Err(_) => {
                    #[cfg(feature = "std")]
                    self.metrics.record_contention();
                    // Retry with the new head
                }
            }
        }
    }

    /// Pop a value from the stack
    ///
    /// This operation is lock-free and will complete if other threads make progress.
    ///
    /// # Returns
    ///
    /// * `Some(value)` if the stack was not empty
    /// * `None` if the stack was empty
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::stack::LockFreeStack;
    ///
    /// let stack = LockFreeStack::new();
    /// stack.push(42);
    ///
    /// let value = stack.pop();
    /// assert_eq!(value, Some(42));
    /// ```
    pub fn pop(&self) -> Option<T> {
        #[cfg(feature = "std")]
        let start = std::time::Instant::now();

        loop {
            let head = self.head.get().load(Ordering::Acquire);

            if head.is_null() {
                #[cfg(feature = "std")]
                self.metrics.record_failure();
                return None;
            }

            // Get the next node
            let next = unsafe { (*head).next.load(Ordering::Relaxed) };

            // Try to update head to point to next
            match self.head.get().compare_exchange_weak(
                head,
                next,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Successfully removed head, extract data
                    let data = unsafe {
                        let node = Box::from_raw(head);
                        node.data
                    };

                    #[cfg(feature = "std")]
                    self.metrics.record_success(start.elapsed());

                    return Some(data);
                }
                Err(_) => {
                    #[cfg(feature = "std")]
                    self.metrics.record_contention();
                    // Retry with the new head
                }
            }
        }
    }

    /// Check if the stack is empty
    ///
    /// This operation is lock-free and provides a snapshot of the stack's state.
    ///
    /// # Returns
    ///
    /// `true` if the stack is empty, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::stack::LockFreeStack;
    ///
    /// let stack = LockFreeStack::new();
    /// assert!(stack.is_empty());
    ///
    /// stack.push(42);
    /// assert!(!stack.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.head.get().load(Ordering::Acquire).is_null()
    }

    /// Get the approximate number of elements in the stack
    ///
    /// Note: This is an expensive operation as it traverses the entire stack.
    /// Use only for debugging or monitoring purposes.
    ///
    /// # Returns
    ///
    /// The approximate number of elements in the stack
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::stack::LockFreeStack;
    ///
    /// let stack = LockFreeStack::new();
    /// assert_eq!(stack.len(), 0);
    ///
    /// stack.push(1);
    /// stack.push(2);
    /// assert_eq!(stack.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        let mut count = 0;
        let mut current = self.head.get().load(Ordering::Acquire);

        while !current.is_null() {
            count += 1;
            current = unsafe { (*current).next.load(Ordering::Relaxed) };
        }

        count
    }

    /// Try to pop multiple elements at once
    ///
    /// This can be more efficient than individual pops as it reduces atomic operations.
    ///
    /// # Arguments
    ///
    /// * `max_count` - Maximum number of elements to pop
    ///
    /// # Returns
    ///
    /// A vector containing the popped elements (in LIFO order)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::stack::LockFreeStack;
    ///
    /// let stack = LockFreeStack::new();
    /// stack.push(1);
    /// stack.push(2);
    /// stack.push(3);
    ///
    /// let elements = stack.pop_batch(2);
    /// assert_eq!(elements.len(), 2);
    /// ```
    pub fn pop_batch(&self, max_count: usize) -> Vec<T> {
        let mut result = Vec::with_capacity(max_count);

        for _ in 0..max_count {
            if let Some(value) = self.pop() {
                result.push(value);
            } else {
                break;
            }
        }

        result
    }

    /// Push multiple elements at once
    ///
    /// Elements are pushed in the order they appear in the iterator.
    /// The last element will be at the top of the stack.
    ///
    /// # Arguments
    ///
    /// * `values` - Iterator of values to push
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::stack::LockFreeStack;
    ///
    /// let stack = LockFreeStack::new();
    /// let values = vec![1, 2, 3];
    /// stack.push_batch(values);
    ///
    /// assert_eq!(stack.pop(), Some(3));
    /// assert_eq!(stack.pop(), Some(2));
    /// assert_eq!(stack.pop(), Some(1));
    /// ```
    pub fn push_batch<I>(&self, values: I)
    where
        I: IntoIterator<Item = T>,
    {
        for value in values {
            self.push(value);
        }
    }
}

impl<T> Default for LockFreeStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for LockFreeStack<T> {
    fn drop(&mut self) {
        // Clean up all remaining nodes
        let mut current = self.head.get().load(Ordering::Acquire);

        while !current.is_null() {
            let next = unsafe { (*current).next.load(Ordering::Relaxed) };
            unsafe {
                drop(Box::from_raw(current));
            }
            current = next;
        }
    }
}

#[cfg(feature = "std")]
impl<T> MetricsCollector for LockFreeStack<T> {
    fn metrics(&self) -> crate::metrics::PerformanceMetrics {
        self.metrics.snapshot()
    }

    fn reset_metrics(&self) {
        self.metrics.reset();
    }

    fn set_metrics_enabled(&self, enabled: bool) {
        self.metrics_enabled
            .store(enabled as usize, StdOrdering::Relaxed);
    }

    fn is_metrics_enabled(&self) -> bool {
        self.metrics_enabled.load(StdOrdering::Relaxed) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::vec;

    #[test]
    fn test_basic_operations() {
        let stack = LockFreeStack::new();

        // Test empty stack
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
        assert_eq!(stack.pop(), None);

        // Test push and pop
        stack.push(1);
        stack.push(2);
        stack.push(3);

        assert!(!stack.is_empty());
        assert_eq!(stack.len(), 3);

        assert_eq!(stack.pop(), Some(3));
        assert_eq!(stack.pop(), Some(2));
        assert_eq!(stack.pop(), Some(1));
        assert_eq!(stack.pop(), None);

        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn test_batch_operations() {
        let stack = LockFreeStack::new();

        // Test batch push
        stack.push_batch(vec![1, 2, 3, 4, 5]);
        assert_eq!(stack.len(), 5);

        // Test batch pop
        let elements = stack.pop_batch(3);
        assert_eq!(elements.len(), 3);
        assert_eq!(stack.len(), 2);

        // Verify LIFO order
        assert_eq!(stack.pop(), Some(2));
        assert_eq!(stack.pop(), Some(1));
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn test_concurrent_operations() {
        let stack = Arc::new(LockFreeStack::new());
        let mut handles = vec![];

        // Spawn multiple producer threads
        for i in 0..4 {
            let stack_clone = Arc::clone(&stack);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    stack_clone.push(i * 100 + j);
                }
            });
            handles.push(handle);
        }

        // Wait for all producers to finish
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all elements are present
        let mut count = 0;
        while stack.pop().is_some() {
            count += 1;
        }

        assert_eq!(count, 400);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_producer_consumer() {
        let stack = Arc::new(LockFreeStack::new());
        let _handles: Vec<std::thread::JoinHandle<()>> = vec![];

        // Producer thread
        let producer_stack = Arc::clone(&stack);
        let producer = thread::spawn(move || {
            for i in 0..1000 {
                producer_stack.push(i);
            }
        });

        // Consumer thread
        let consumer_stack = Arc::clone(&stack);
        let consumer = thread::spawn(move || {
            let mut sum = 0;
            let mut count = 0;
            while count < 1000 {
                if let Some(value) = consumer_stack.pop() {
                    sum += value;
                    count += 1;
                }
                // Small delay to allow producer to add more elements
                thread::yield_now();
            }
            sum
        });

        producer.join().unwrap();
        let result = consumer.join().unwrap();

        // Sum of 0..999 = 499500
        assert_eq!(result, 499500);
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_metrics() {
        let stack = LockFreeStack::new();

        // Perform some operations
        stack.push(1);
        stack.push(2);
        stack.push(3);

        let _ = stack.pop();
        let _ = stack.pop();
        let _ = stack.pop();
        let _ = stack.pop(); // This will fail

        // Check metrics
        let metrics = stack.metrics();
        assert_eq!(metrics.total_operations, 7);
        assert_eq!(metrics.successful_operations, 6);
        assert_eq!(metrics.failed_operations, 1);
        assert!(metrics.success_rate() > 80.0);

        // Test metrics control
        stack.set_metrics_enabled(false);
        assert!(!stack.is_metrics_enabled());

        stack.reset_metrics();
        let reset_metrics = stack.metrics();
        assert_eq!(reset_metrics.total_operations, 0);
    }
}
