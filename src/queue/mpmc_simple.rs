//! Simple MPMC Queue for VelocityX
//!
//! A basic multi-producer, multi-consumer queue implementation.

use crate::util::CachePadded;
use crate::metrics::{AtomicMetrics, MetricsCollector};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::vec::Vec;

#[cfg(feature = "std")]
use std::time::Duration;

/// A simple bounded MPMC queue
#[derive(Debug)]
pub struct MpmcQueue<T> {
    buffer: Arc<Mutex<Vec<Option<T>>>>,
    capacity: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
    push_lock: Mutex<()>,
    pop_lock: Mutex<()>,
    metrics: AtomicMetrics,
    metrics_enabled: AtomicUsize,
}

impl<T> MpmcQueue<T> {
    /// Create a new MPMC queue with the specified capacity
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be greater than 0");

        Self {
            buffer: Arc::new(Mutex::new((0..capacity).map(|_| None).collect())),
            capacity,
            head: CachePadded::new(AtomicUsize::new(0)),
            tail: CachePadded::new(AtomicUsize::new(0)),
            push_lock: Mutex::new(()),
            pop_lock: Mutex::new(()),
            metrics: AtomicMetrics::default(),
            metrics_enabled: AtomicUsize::new(1), // Enabled by default
        }
    }

    /// Push a value into the queue
    pub fn push(&self, value: T) -> Result<(), crate::Error> {
        #[cfg(feature = "std")]
        let start = std::time::Instant::now();
        
        let _guard = self.push_lock.lock().unwrap();

        let tail = self.tail.get().load(Ordering::Relaxed);
        let head = self.head.get().load(Ordering::Relaxed);

        if tail - head >= self.capacity {
            #[cfg(feature = "std")]
            self.metrics.record_failure();
            return Err(crate::Error::WouldBlock);
        }

        let index = tail % self.capacity;
        {
            let mut buffer = self.buffer.lock().unwrap();
            buffer[index] = Some(value);
        }
        self.tail.get().store(tail + 1, Ordering::Release);

        #[cfg(feature = "std")]
        self.metrics.record_success(start.elapsed());
        
        Ok(())
    }

    /// Pop a value from the queue
    pub fn pop(&self) -> Option<T> {
        #[cfg(feature = "std")]
        let start = std::time::Instant::now();
        
        let _guard = self.pop_lock.lock().unwrap();

        let head = self.head.get().load(Ordering::Relaxed);
        let tail = self.tail.get().load(Ordering::Relaxed);

        if head == tail {
            #[cfg(feature = "std")]
            self.metrics.record_failure();
            return None;
        }

        let index = head % self.capacity;
        let value = {
            let mut buffer = self.buffer.lock().unwrap();
            buffer[index].take()
        };
        self.head.get().store(head + 1, Ordering::Release);

        #[cfg(feature = "std")]
        self.metrics.record_success(start.elapsed());

        value
    }

    /// Try to push without blocking
    pub fn try_push(&self, value: T) -> Result<(), crate::Error> {
        self.push(value)
    }

    /// Try to pop without blocking
    pub fn try_pop(&self) -> Option<T> {
        self.pop()
    }

    /// Get the current number of elements in the queue
    pub fn len(&self) -> usize {
        let head = self.head.get().load(Ordering::Relaxed);
        let tail = self.tail.get().load(Ordering::Relaxed);
        tail - head
    }

    /// Check if the queue is empty
    pub fn is_empty(&self) -> bool {
        let head = self.head.get().load(Ordering::Relaxed);
        let tail = self.tail.get().load(Ordering::Relaxed);
        head == tail
    }

    /// Push multiple elements to the queue in a single operation
    ///
    /// This is more efficient than individual pushes as it reduces lock contention.
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
    /// use velocityx::queue::MpmcQueue;
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
        let _guard = self.push_lock.lock().unwrap();
        let mut pushed = 0;
        
        let tail = self.tail.get().load(Ordering::Relaxed);
        let head = self.head.get().load(Ordering::Relaxed);
        let available_space = self.capacity - (tail - head);
        
        for value in values.into_iter().take(available_space) {
            let index = (tail + pushed) % self.capacity;
            {
                let mut buffer = self.buffer.lock().unwrap();
                buffer[index] = Some(value);
            }
            pushed += 1;
        }
        
        self.tail.get().store(tail + pushed, Ordering::Release);
        pushed
    }

    /// Pop multiple elements from the queue in a single operation
    ///
    /// This is more efficient than individual pops as it reduces lock contention.
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
    /// use velocityx::queue::MpmcQueue;
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
        let _guard = self.pop_lock.lock().unwrap();
        let head = self.head.get().load(Ordering::Relaxed);
        let tail = self.tail.get().load(Ordering::Relaxed);
        
        let available_items = tail - head;
        let items_to_pop = std::cmp::min(max_values, available_items);
        
        let mut values = Vec::with_capacity(items_to_pop);
        
        for i in 0..items_to_pop {
            let index = (head + i) % self.capacity;
            {
                let mut buffer = self.buffer.lock().unwrap();
                if let Some(value) = buffer[index].take() {
                    values.push(value);
                }
            }
        }
        
        self.head.get().store(head + items_to_pop, Ordering::Release);
        values
    }

    /// Try to push an element with a timeout
    ///
    /// This operation will retry for the specified duration before giving up.
    /// Uses adaptive backoff to reduce contention.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Duration to wait before giving up
    /// * `value_factory` - Closure that produces the value to push (called only when needed)
    ///
    /// # Returns
    ///
    /// * `Ok(())` if the element was successfully pushed
    /// * `Err(crate::Error::Timeout)` if the timeout expired
    ///
    /// # Examples
    ///
    /// ```rust
    /// use velocityx::queue::MpmcQueue;
    /// use std::time::Duration;
    ///
    /// let queue: MpmcQueue<i32> = MpmcQueue::new(1);
    /// queue.push(42).unwrap();
    ///
    /// let result = queue.push_with_timeout(Duration::from_millis(100), || 43);
    /// assert!(result.is_err()); // Queue full, timeout
    /// ```
    #[cfg(feature = "std")]
    pub fn push_with_timeout<F>(&self, timeout: Duration, mut value_factory: F) -> Result<(), crate::Error>
    where
        F: FnMut() -> T,
    {
        let start = std::time::Instant::now();
        let mut backoff = Duration::from_nanos(100);
        
        while start.elapsed() < timeout {
            match self.push(value_factory()) {
                Ok(()) => return Ok(()),
                Err(crate::Error::WouldBlock) => {
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
        
        Err(crate::Error::Timeout)
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
    /// use velocityx::queue::MpmcQueue;
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

impl<T> Clone for MpmcQueue<T> {
    fn clone(&self) -> Self {
        Self {
            buffer: Arc::clone(&self.buffer),
            capacity: self.capacity,
            head: CachePadded::new(AtomicUsize::new(self.head.get().load(Ordering::Relaxed))),
            tail: CachePadded::new(AtomicUsize::new(self.tail.get().load(Ordering::Relaxed))),
            push_lock: Mutex::new(()),
            pop_lock: Mutex::new(()),
            metrics: AtomicMetrics::default(),
            metrics_enabled: AtomicUsize::new(self.metrics_enabled.load(Ordering::Relaxed)),
        }
    }
}

impl<T> Drop for MpmcQueue<T> {
    fn drop(&mut self) {
        // Clear all remaining elements
        if let Ok(mut buffer) = self.buffer.lock() {
            for item in buffer.iter_mut() {
                *item = None;
            }
        }
    }
}

#[cfg(feature = "std")]
impl<T> MetricsCollector for MpmcQueue<T> {
    fn metrics(&self) -> crate::metrics::PerformanceMetrics {
        self.metrics.snapshot()
    }
    
    fn reset_metrics(&self) {
        self.metrics.reset();
    }
    
    fn set_metrics_enabled(&self, enabled: bool) {
        self.metrics_enabled.store(enabled as usize, std::sync::atomic::Ordering::Relaxed);
    }
    
    fn is_metrics_enabled(&self) -> bool {
        self.metrics_enabled.load(std::sync::atomic::Ordering::Relaxed) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let queue: MpmcQueue<i32> = MpmcQueue::new(3);

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
        assert_eq!(queue.pop(), None);

        queue.push(1).unwrap();
        queue.push(2).unwrap();
        queue.push(3).unwrap();

        assert_eq!(queue.len(), 3);
        assert!(!queue.is_empty());

        assert_eq!(queue.push(4), Err(crate::Error::WouldBlock));

        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
        assert_eq!(queue.pop(), None);

        assert!(queue.is_empty());
        assert_eq!(queue.len(), 0);
    }
}
