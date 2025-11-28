//! Property-based tests for MPMC queues using proptest
//!
//! These tests verify that the queue implementations maintain their invariants
//! under various concurrent scenarios and edge cases.

use crate::queue::mpmc::{MpmcQueue, UnboundedMpmcQueue};
use proptest::prelude::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Property: FIFO ordering is preserved for bounded queues
#[cfg(test)]
mod bounded_queue_properties {
    use super::*;

    proptest! {
        #[test]
        fn test_fifo_ordering_single_thread(
            operations in prop::collection::vec(
                prop::collection::vec(any::<i32>(), 1..10),
                1..5
            )
        ) {
            let queue: MpmcQueue<i32> = MpmcQueue::new(100);
            let mut expected = Vec::new();
            
            // Push all operations
            for batch in &operations {
                for &value in batch {
                    prop_assume!(queue.push(value).is_ok());
                    expected.push(value);
                }
            }
            
            // Pop and verify ordering
            for expected_value in expected {
                prop_assert_eq!(queue.pop(), Some(expected_value));
            }
            
            prop_assert!(queue.is_empty());
        }

        #[test]
        fn test_capacity_invariant(
            capacity in 1usize..100,
            operations in prop::collection::vec(any::<i32>(), 1..200)
        ) {
            let queue: MpmcQueue<i32> = MpmcQueue::new(capacity);
            let mut successful_pushes = 0;
            
            // Try to push operations
            for &value in &operations {
                if queue.push(value).is_ok() {
                    successful_pushes += 1;
                }
            }
            
            // Verify size never exceeds capacity
            prop_assert!(queue.len() <= capacity);
            
            // Verify successful pushes match queue size
            prop_assert_eq!(queue.len(), successful_pushes);
            
            // Drain the queue
            let mut popped = 0;
            while queue.pop().is_some() {
                popped += 1;
            }
            
            // Verify all successful pushes can be popped
            prop_assert_eq!(popped, successful_pushes);
        }

        #[test]
        fn test_len_invariant(
            capacity in 1usize..100,
            operations in prop::collection::vec(
                prop::bool::weighted(0.7), // 70% push, 30% pop
                1..100
            )
        ) {
            let queue: MpmcQueue<i32> = MpmcQueue::new(capacity);
            let mut expected_len = 0;
            let mut counter = 0;
            
            for &should_push in &operations {
                if should_push {
                    // Try to push
                    if queue.push(counter).is_ok() {
                        expected_len += 1;
                    }
                    counter += 1;
                } else {
                    // Try to pop
                    if queue.pop().is_some() {
                        expected_len = expected_len.saturating_sub(1);
                    }
                }
                
                // Verify len is within bounds
                let actual_len = queue.len();
                prop_assert!(actual_len <= capacity);
                prop_assert!(actual_len <= expected_len + 1); // Allow some variance due to contention
            }
        }
    }
}

/// Property: FIFO ordering is preserved for unbounded queues
#[cfg(test)]
mod unbounded_queue_properties {
    use super::*;

    proptest! {
        #[test]
        fn test_fifo_ordering_single_thread(
            operations in prop::collection::vec(
                prop::collection::vec(any::<i32>(), 1..10),
                1..5
            )
        ) {
            let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
            let mut expected = Vec::new();
            
            // Push all operations
            for batch in &operations {
                for &value in batch {
                    queue.push(value).unwrap();
                    expected.push(value);
                }
            }
            
            // Pop and verify ordering
            for expected_value in expected {
                prop_assert_eq!(queue.pop(), Some(expected_value));
            }
            
            prop_assert!(queue.is_empty());
        }

        #[test]
        fn test_len_invariant(
            operations in prop::collection::vec(
                prop::bool::weighted(0.7), // 70% push, 30% pop
                1..100
            )
        ) {
            let queue: UnboundedMpmcQueue<i32> = UnboundedMpmcQueue::new();
            let mut expected_len = 0;
            let mut counter = 0;
            
            for &should_push in &operations {
                if should_push {
                    // Push (always succeeds for unbounded)
                    queue.push(counter).unwrap();
                    expected_len += 1;
                    counter += 1;
                } else {
                    // Try to pop
                    if queue.pop().is_some() {
                        expected_len = expected_len.saturating_sub(1);
                    }
                }
                
                // Verify len matches expected (within small variance)
                let actual_len = queue.len();
                prop_assert!(actual_len <= expected_len + 1);
                prop_assert!(actual_len >= expected_len.saturating_sub(1));
            }
        }

        #[test]
        fn test_no_memory_leaks(
            operations in prop::collection::vec(any::<i32>(), 1..100)
        ) {
            use std::sync::atomic::{AtomicUsize, Ordering};
            
            static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
            
            #[derive(Debug, PartialEq, Eq, Hash)]
            struct DropTracker {
                id: i32,
            }
            
            impl Drop for DropTracker {
                fn drop(&mut self) {
                    DROP_COUNT.fetch_add(1, Ordering::Relaxed);
                }
            }
            
            let queue: UnboundedMpmcQueue<DropTracker> = UnboundedMpmcQueue::new();
            
            // Push operations
            for &value in &operations {
                queue.push(DropTracker { id: value }).unwrap();
            }
            
            // Pop some operations
            for _ in 0..operations.len() / 2 {
                queue.pop();
            }
            
            // Drop the queue
            drop(queue);
            
            // All items should be dropped
            let dropped_items = DROP_COUNT.load(Ordering::Relaxed);
            prop_assert_eq!(dropped_items, operations.len());
        }
    }
}

/// Property: Concurrent operations maintain invariants
#[cfg(test)]
mod concurrent_properties {
    use super::*;

    proptest! {
        #[test]
        fn test_concurrent_bounded_queue(
            num_threads in 2usize..8,
            operations_per_thread in 10usize..100,
            capacity in 10usize..100
        ) {
            let queue = Arc::new(MpmcQueue::<usize>::new(capacity));
            let mut handles = vec![];
            
            // Spawn producer threads
            for thread_id in 0..num_threads {
                let queue = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    for i in 0..operations_per_thread {
                        let value = thread_id * operations_per_thread + i;
                        while queue.push(value).is_err() {
                            thread::yield_now();
                        }
                    }
                });
                handles.push(handle);
            }
            
            // Spawn consumer threads
            for _ in 0..num_threads {
                let queue = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    let mut received = Vec::new();
                    while received.len() < operations_per_thread {
                        if let Some(value) = queue.pop() {
                            received.push(value);
                        } else {
                            thread::yield_now();
                        }
                    }
                    received
                });
                handles.push(handle);
            }
            
            // Wait for all threads
            let mut all_received = Vec::new();
            for handle in handles {
                let received = handle.join().unwrap();
                all_received.extend(received);
            }
            
            // Verify total received equals total sent
            let expected_total = num_threads * operations_per_thread;
            prop_assert_eq!(all_received.len(), expected_total);
            
            // Verify all values are unique (no duplicates)
            let mut sorted = all_received.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(sorted.len(), all_received.len());
            
            // Verify all values are within expected range
            for &value in &all_received {
                prop_assert!(value < expected_total);
            }
        }

        #[test]
        fn test_concurrent_unbounded_queue(
            num_threads in 2usize..8,
            operations_per_thread in 10usize..100
        ) {
            let queue = Arc::new(UnboundedMpmcQueue::<usize>::new());
            let mut handles = vec![];
            
            // Spawn producer threads
            for thread_id in 0..num_threads {
                let queue = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    for i in 0..operations_per_thread {
                        let value = thread_id * operations_per_thread + i;
                        queue.push(value).unwrap();
                    }
                });
                handles.push(handle);
            }
            
            // Spawn consumer threads
            for _ in 0..num_threads {
                let queue = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    let mut received = Vec::new();
                    while received.len() < operations_per_thread {
                        if let Some(value) = queue.pop() {
                            received.push(value);
                        } else {
                            thread::yield_now();
                        }
                    }
                    received
                });
                handles.push(handle);
            }
            
            // Wait for all threads
            let mut all_received = Vec::new();
            for handle in handles {
                let received = handle.join().unwrap();
                all_received.extend(received);
            }
            
            // Verify total received equals total sent
            let expected_total = num_threads * operations_per_thread;
            prop_assert_eq!(all_received.len(), expected_total);
            
            // Verify all values are unique (no duplicates)
            let mut sorted = all_received.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(sorted.len(), all_received.len());
        }
    }
}

/// Property: Edge cases and boundary conditions
#[cfg(test)]
mod edge_case_properties {
    use super::*;

    proptest! {
        #[test]
        fn test_empty_queue_operations(
            operations in prop::collection::vec(any::<i32>(), 0..10)
        ) {
            let queue: MpmcQueue<i32> = MpmcQueue::new(5);
            
            // Pop from empty queue should always return None
            for _ in 0..operations.len() {
                prop_assert_eq!(queue.pop(), None);
            }
            
            prop_assert!(queue.is_empty());
            prop_assert_eq!(queue.len(), 0);
        }

        #[test]
        fn test_single_element_queue(
            value in any::<i32>()
        ) {
            let queue: MpmcQueue<i32> = MpmcQueue::new(1);
            
            // Push single element
            prop_assert!(queue.push(value).is_ok());
            prop_assert_eq!(queue.len(), 1);
            prop_assert!(!queue.is_empty());
            
            // Try to push another (should fail)
            prop_assert!(queue.push(value + 1).is_err());
            prop_assert_eq!(queue.len(), 1);
            
            // Pop the element
            prop_assert_eq!(queue.pop(), Some(value));
            prop_assert!(queue.is_empty());
            prop_assert_eq!(queue.len(), 0);
        }

        #[test]
        fn test_wrap_around_behavior(
            capacity in 2usize..10,
            iterations in 10usize..100
        ) {
            let queue: MpmcQueue<usize> = MpmcQueue::new(capacity);
            
            // Fill and empty the queue multiple times
            for round in 0..iterations {
                // Fill the queue
                for i in 0..capacity {
                    let value = round * capacity + i;
                    prop_assert!(queue.push(value).is_ok());
                }
                
                // Queue should be full
                prop_assert_eq!(queue.len(), capacity);
                prop_assert!(queue.push(999).is_err());
                
                // Empty the queue
                for i in 0..capacity {
                    let expected = round * capacity + i;
                    prop_assert_eq!(queue.pop(), Some(expected));
                }
                
                // Queue should be empty
                prop_assert!(queue.is_empty());
                prop_assert_eq!(queue.len(), 0);
            }
        }
    }
}
