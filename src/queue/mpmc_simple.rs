//! Simple MPMC Queue for VelocityX
//!
//! A basic multi-producer, multi-consumer queue implementation.

use crate::util::CachePadded;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::vec::Vec;

/// A simple bounded MPMC queue
#[derive(Debug)]
pub struct MpmcQueue<T> {
    buffer: Arc<Mutex<Vec<Option<T>>>>,
    capacity: usize,
    head: CachePadded<AtomicUsize>,
    tail: CachePadded<AtomicUsize>,
    push_lock: Mutex<()>,
    pop_lock: Mutex<()>,
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
        }
    }

    /// Push a value into the queue
    pub fn push(&self, value: T) -> Result<(), crate::Error> {
        let _guard = self.push_lock.lock().unwrap();

        let tail = self.tail.get().load(Ordering::Relaxed);
        let head = self.head.get().load(Ordering::Relaxed);

        if tail - head >= self.capacity {
            return Err(crate::Error::WouldBlock);
        }

        let index = tail % self.capacity;
        {
            let mut buffer = self.buffer.lock().unwrap();
            buffer[index] = Some(value);
        }
        self.tail.get().store(tail + 1, Ordering::Release);

        Ok(())
    }

    /// Pop a value from the queue
    pub fn pop(&self) -> Option<T> {
        let _guard = self.pop_lock.lock().unwrap();

        let head = self.head.get().load(Ordering::Relaxed);
        let tail = self.tail.get().load(Ordering::Relaxed);

        if head == tail {
            return None;
        }

        let index = head % self.capacity;
        let value = {
            let mut buffer = self.buffer.lock().unwrap();
            buffer[index].take()
        };
        self.head.get().store(head + 1, Ordering::Release);

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

    /// Get the capacity of the queue
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
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
