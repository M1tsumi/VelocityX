//! Integration tests for queue implementations

use super::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_mpmc_stress_test() {
    let queue = Arc::new(MpmcQueue::new(10000));
    let num_producers = 8;
    let num_consumers = 8;
    let items_per_producer = 10000;
    
    // Spawn producer threads
    let mut producer_handles = vec![];
    for producer_id in 0..num_producers {
        let queue = Arc::clone(&queue);
        let handle = thread::spawn(move || {
            let mut produced = 0;
            for i in 0..items_per_producer {
                let value = producer_id * items_per_producer + i;
                match queue.push(value) {
                    Ok(()) => produced += 1,
                    Err(_) => {
                        // Queue full, retry
                        i -= 1; // Retry this item
                        thread::yield_now();
                    }
                }
            }
            produced
        });
        producer_handles.push(handle);
    }
    
    // Spawn consumer threads
    let mut consumer_handles = vec![];
    for _ in 0..num_consumers {
        let queue = Arc::clone(&queue);
        let handle = thread::spawn(move || {
            let mut consumed = 0;
            let mut sum = 0;
            while consumed < items_per_producer * num_producers / num_consumers {
                match queue.pop() {
                    Some(value) => {
                        consumed += 1;
                        sum += value;
                    }
                    None => {
                        // Queue empty, check if producers are done
                        if queue.len() == 0 {
                            thread::sleep(Duration::from_millis(1));
                        }
                    }
                }
            }
            (consumed, sum)
        });
        consumer_handles.push(handle);
    }
    
    // Wait for all producers
    let mut total_produced = 0;
    for handle in producer_handles {
        total_produced += handle.join().unwrap();
    }
    
    // Wait for all consumers
    let mut total_consumed = 0;
    let mut total_sum = 0;
    for handle in consumer_handles {
        let (consumed, sum) = handle.join().unwrap();
        total_consumed += consumed;
        total_sum += sum;
    }
    
    assert_eq!(total_produced, num_producers * items_per_producer);
    assert_eq!(total_consumed, num_producers * items_per_producer);
    
    // Verify the sum is correct (sum of 0..N-1 where N = total_produced)
    let expected_sum = total_produced * (total_produced - 1) / 2;
    assert_eq!(total_sum, expected_sum);
}

#[test]
fn test_mpmc_drop_safety() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    
    struct DropCounter {
        id: usize,
    }
    
    impl Drop for DropCounter {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    let queue: MpmcQueue<DropCounter> = MpmcQueue::new(100);
    
    // Push items
    for i in 0..50 {
        queue.push(DropCounter { id: i }).unwrap();
    }
    
    // Drop some items
    for _ in 0..25 {
        queue.pop();
    }
    
    // Drop the queue
    drop(queue);
    
    // All remaining items should be dropped
    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), 50);
}

#[test]
fn test_mpmc_memory_ordering() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Barrier;
    
    let queue = Arc::new(MpmcQueue::new(10));
    let barrier = Arc::new(Barrier::new(3));
    let ready = Arc::new(AtomicBool::new(false));
    
    // Producer thread
    let producer = {
        let queue = Arc::clone(&queue);
        let barrier = Arc::clone(&barrier);
        let ready = Arc::clone(&ready);
        thread::spawn(move || {
            barrier.wait();
            ready.store(true, Ordering::Release);
            queue.push(42).unwrap();
        })
    };
    
    // Consumer thread
    let consumer = {
        let queue = Arc::clone(&queue);
        let barrier = Arc::clone(&barrier);
        let ready = Arc::clone(&ready);
        thread::spawn(move || {
            barrier.wait();
            while !ready.load(Ordering::Acquire) {
                thread::yield_now();
            }
            let value = queue.pop();
            assert_eq!(value, Some(42));
        })
    };
    
    // Observer thread
    let observer = {
        let queue = Arc::clone(&queue);
        let barrier = Arc::clone(&barrier);
        let ready = Arc::clone(&ready);
        thread::spawn(move || {
            barrier.wait();
            while !ready.load(Ordering::Acquire) {
                thread::yield_now();
            }
            // Should eventually see the item
            let mut attempts = 0;
            while attempts < 1000 {
                if queue.len() > 0 {
                    break;
                }
                attempts += 1;
                thread::yield_now();
            }
            assert!(attempts < 1000, "Item should be visible within reasonable time");
        })
    };
    
    producer.join().unwrap();
    consumer.join().unwrap();
    observer.join().unwrap();
}

#[test]
fn test_mpmc_capacity_edge_cases() {
    // Test capacity of 1
    let queue: MpmcQueue<i32> = MpmcQueue::new(1);
    assert_eq!(queue.capacity(), 1);
    assert!(queue.push(1).is_ok());
    assert!(queue.push(2).is_err());
    assert_eq!(queue.pop(), Some(1));
    assert!(queue.push(3).is_ok());
    
    // Test large capacity
    let queue: MpmcQueue<i32> = MpmcQueue::new(1000000);
    assert!(queue.capacity() >= 1000000);
    assert!(queue.capacity().is_power_of_two());
}

#[test]
fn test_mpmc_fairness() {
    use std::collections::HashSet;
    
    let queue = Arc::new(MpmcQueue::new(1000));
    let num_threads = 4;
    let items_per_thread = 100;
    
    let mut handles = vec![];
    
    // Each thread pushes a unique range of values
    for thread_id in 0..num_threads {
        let queue = Arc::clone(&queue);
        let handle = thread::spawn(move || {
            for i in 0..items_per_thread {
                let value = thread_id * 1000 + i; // Unique values per thread
                while queue.push(value).is_err() {
                    thread::yield_now();
                }
            }
        });
        handles.push(handle);
    }
    
    // Wait for all producers
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Collect all values
    let mut values = HashSet::new();
    let mut count = 0;
    
    while count < num_threads * items_per_thread {
        if let Some(value) = queue.pop() {
            assert!(!values.contains(&value), "Duplicate value: {}", value);
            values.insert(value);
            count += 1;
        }
    }
    
    assert_eq!(values.len(), num_threads * items_per_thread);
    
    // Verify all threads contributed
    for thread_id in 0..num_threads {
        let thread_values: HashSet<_> = values.iter()
            .filter(|&&v| v / 1000 == thread_id)
            .collect();
        assert_eq!(thread_values.len(), items_per_thread);
    }
}
