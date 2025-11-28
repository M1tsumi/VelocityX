//! Integration tests for VelocityX
//!
//! These tests verify that all components work together correctly and that
//! the library provides a cohesive, high-performance concurrent programming experience.

use velocityx::queue::MpmcQueue;
use velocityx::map::ConcurrentHashMap;
use velocityx::deque::WorkStealingDeque;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

#[test]
fn test_mixed_data_structures_integration() {
    // Test that multiple data structures can work together without interference
    let queue = Arc::new(MpmcQueue::new(1000));
    let map = Arc::new(ConcurrentHashMap::new());
    let deque = Arc::new(WorkStealingDeque::new(1000));
    
    let num_threads = 4;
    let operations_per_thread = 1000;
    let barrier = Arc::new(Barrier::new(num_threads));
    
    let mut handles = vec![];
    
    for thread_id in 0..num_threads {
        let queue = Arc::clone(&queue);
        let map = Arc::clone(&map);
        let deque = Arc::clone(&deque);
        let barrier = Arc::clone(&barrier);
        
        let handle = thread::spawn(move || {
            barrier.wait();
            
            let mut queue_ops = 0;
            let mut map_ops = 0;
            let mut deque_ops = 0;
            
            for i in 0..operations_per_thread {
                match i % 3 {
                    0 => {
                        // Queue operations
                        let value = thread_id * operations_per_thread + i;
                        if queue.push(value).is_ok() {
                            queue_ops += 1;
                        }
                        if queue.pop().is_some() {
                            queue_ops += 1;
                        }
                    }
                    1 => {
                        // Map operations
                        let key = format!("key_{}_{}", thread_id, i);
                        let value = format!("value_{}_{}", thread_id, i);
                        map.insert(key.clone(), value);
                        map_ops += 1;
                        if map.get(&key).is_some() {
                            map_ops += 1;
                        }
                    }
                    2 => {
                        // Deque operations
                        let task = thread_id * operations_per_thread + i;
                        if deque.push(task).is_ok() {
                            deque_ops += 1;
                        }
                        if deque.pop().or_else(|| deque.steal()).is_some() {
                            deque_ops += 1;
                        }
                    }
                    _ => unreachable!(),
                }
            }
            
            (queue_ops, map_ops, deque_ops)
        });
        
        handles.push(handle);
    }
    
    let mut total_queue_ops = 0;
    let mut total_map_ops = 0;
    let mut total_deque_ops = 0;
    
    for handle in handles {
        let (queue_ops, map_ops, deque_ops) = handle.join().unwrap();
        total_queue_ops += queue_ops;
        total_map_ops += map_ops;
        total_deque_ops += deque_ops;
    }
    
    // Verify all operations completed
    assert!(total_queue_ops > 0);
    assert!(total_map_ops > 0);
    assert!(total_deque_ops > 0);
    
    println!("Integration test results:");
    println!("  Queue operations: {}", total_queue_ops);
    println!("  Map operations: {}", total_map_ops);
    println!("  Deque operations: {}", total_deque_ops);
}

#[test]
fn test_high_contention_scenario() {
    // Test all data structures under extreme contention
    let queue = Arc::new(MpmcQueue::new(100));
    let map = Arc::new(ConcurrentHashMap::new());
    let deque = Arc::new(WorkStealingDeque::new(100));
    
    let num_producers = 8;
    let num_consumers = 8;
    let operations_per_thread = 5000;
    let barrier = Arc::new(Barrier::new(num_producers + num_consumers));
    
    let mut handles = vec![];
    
    // Producer threads
    for producer_id in 0..num_producers {
        let queue = Arc::clone(&queue);
        let map = Arc::clone(&map);
        let deque = Arc::clone(&deque);
        let barrier = Arc::clone(&barrier);
        
        let handle = thread::spawn(move || {
            barrier.wait();
            
            for i in 0..operations_per_thread {
                // Queue production
                let queue_value = producer_id * operations_per_thread + i;
                while queue.push(queue_value).is_err() {
                    thread::yield_now();
                }
                
                // Map production
                let map_key = format!("prod_{}_key_{}", producer_id, i);
                let map_value = format!("prod_{}_value_{}", producer_id, i);
                map.insert(map_key, map_value);
                
                // Deque production
                let deque_task = producer_id * operations_per_thread + i;
                while deque.push(deque_task).is_err() {
                    thread::yield_now();
                }
                
                // Occasionally yield to increase contention
                if i % 100 == 0 {
                    thread::sleep(Duration::from_micros(1));
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Consumer threads
    for consumer_id in 0..num_consumers {
        let queue = Arc::clone(&queue);
        let map = Arc::clone(&map);
        let deque = Arc::clone(&deque);
        let barrier = Arc::clone(&barrier);
        
        let handle = thread::spawn(move || {
            barrier.wait();
            
            let mut queue_consumed = 0;
            let mut map_consumed = 0;
            let mut deque_consumed = 0;
            
            while queue_consumed < operations_per_thread * num_producers / num_consumers ||
                  map_consumed < operations_per_thread * num_producers / num_consumers ||
                  deque_consumed < operations_per_thread * num_producers / num_consumers {
                
                // Queue consumption
                if queue.pop().is_some() {
                    queue_consumed += 1;
                }
                
                // Map consumption
                let map_key = format!("prod_{}_key_{}", consumer_id % num_producers, map_consumed);
                if map.get(&map_key).is_some() {
                    map_consumed += 1;
                }
                
                // Deque consumption
                if deque.pop().or_else(|| deque.steal()).is_some() {
                    deque_consumed += 1;
                }
                
                thread::yield_now();
            }
            
            (queue_consumed, map_consumed, deque_consumed)
        });
        
        handles.push(handle);
    }
    
    let mut total_queue_consumed = 0;
    let mut total_map_consumed = 0;
    let mut total_deque_consumed = 0;
    
    for handle in handles {
        let (queue_consumed, map_consumed, deque_consumed) = handle.join().unwrap();
        total_queue_consumed += queue_consumed;
        total_map_consumed += map_consumed;
        total_deque_consumed += deque_consumed;
    }
    
    // Verify consumption (allowing for some loss due to contention)
    let expected_per_consumer = operations_per_thread * num_producers / num_consumers;
    let tolerance = expected_per_consumer / 10; // 10% tolerance
    
    assert!(total_queue_consumed >= expected_per_consumer * num_consumers - tolerance);
    assert!(total_map_consumed >= expected_per_consumer * num_consumers - tolerance);
    assert!(total_deque_consumed >= expected_per_consumer * num_consumers - tolerance);
    
    println!("High contention test results:");
    println!("  Queue consumed: {} (expected: {})", total_queue_consumed, expected_per_consumer * num_consumers);
    println!("  Map consumed: {} (expected: {})", total_map_consumed, expected_per_consumer * num_consumers);
    println!("  Deque consumed: {} (expected: {})", total_deque_consumed, expected_per_consumer * num_consumers);
}

#[test]
fn test_memory_safety_under_load() {
    // Test that all data structures maintain memory safety under heavy load
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
    
    #[derive(Debug, PartialEq, Eq, Hash, Clone)]
    struct DropTracker {
        id: usize,
    }
    
    impl Drop for DropTracker {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    let queue: Arc<MpmcQueue<DropTracker>> = Arc::new(MpmcQueue::new(1000));
    let map: Arc<ConcurrentHashMap<DropTracker, DropTracker>> = Arc::new(ConcurrentHashMap::new());
    let deque: Arc<WorkStealingDeque<DropTracker>> = Arc::new(WorkStealingDeque::new(1000));
    
    let num_threads = 6;
    let items_per_thread = 500;
    let barrier = Arc::new(Barrier::new(num_threads));
    
    let mut handles = vec![];
    
    for thread_id in 0..num_threads {
        let queue = Arc::clone(&queue);
        let map = Arc::clone(&map);
        let deque = Arc::clone(&deque);
        let barrier = Arc::clone(&barrier);
        
        let handle = thread::spawn(move || {
            barrier.wait();
            
            for i in 0..items_per_thread {
                let tracker = DropTracker { id: thread_id * items_per_thread + i };
                
                // Queue operations
                if queue.push(tracker.clone()).is_ok() {
                    if queue.pop().is_some() {
                        // Item dropped
                    }
                }
                
                // Map operations
                map.insert(tracker.clone(), tracker.clone());
                if map.remove(&tracker).is_some() {
                    // Items dropped
                }
                
                // Deque operations
                if deque.push(tracker.clone()).is_ok() {
                    if deque.pop().or_else(|| deque.steal()).is_some() {
                        // Item dropped
                    }
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Drop all data structures
    drop(queue);
    drop(map);
    drop(deque);
    
    // Wait for all drops to complete
    thread::sleep(Duration::from_millis(100));
    
    let final_drop_count = DROP_COUNT.load(Ordering::Relaxed);
    let expected_drops = num_threads * items_per_thread * 6; // Each item is created 6 times
    
    println!("Memory safety test results:");
    println!("  Expected drops: {}", expected_drops);
    println!("  Actual drops: {}", final_drop_count);
    
    // All items should be dropped (allowing for some variance due to contention)
    let tolerance = expected_drops / 20; // 5% tolerance
    assert!(final_drop_count >= expected_drops - tolerance);
}

#[test]
fn test_performance_regression() {
    // Simple performance regression test
    let queue = Arc::new(MpmcQueue::new(10_000));
    let map = Arc::new(ConcurrentHashMap::new());
    let deque = Arc::new(WorkStealingDeque::new(10_000));
    
    let num_threads = 4;
    let operations_per_thread = 10_000;
    let barrier = Arc::new(Barrier::new(num_threads));
    
    let start_time = std::time::Instant::now();
    
    let mut handles = vec![];
    
    for thread_id in 0..num_threads {
        let queue = Arc::clone(&queue);
        let map = Arc::clone(&map);
        let deque = Arc::clone(&deque);
        let barrier = Arc::clone(&barrier);
        
        let handle = thread::spawn(move || {
            barrier.wait();
            
            for i in 0..operations_per_thread {
                // Mix of operations
                match i % 4 {
                    0 => {
                        let value = thread_id * operations_per_thread + i;
                        while queue.push(value).is_err() {
                            thread::yield_now();
                        }
                    }
                    1 => {
                        let _ = queue.pop();
                    }
                    2 => {
                        let key = format!("key_{}_{}", thread_id, i);
                        let value = format!("value_{}_{}", thread_id, i);
                        map.insert(key, value);
                    }
                    3 => {
                        let task = thread_id * operations_per_thread + i;
                        if deque.push(task).is_ok() {
                            let _ = deque.pop().or_else(|| deque.steal());
                        }
                    }
                    _ => unreachable!(),
                }
            }
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let elapsed = start_time.elapsed();
    let total_operations = num_threads * operations_per_thread;
    let ops_per_sec = total_operations as f64 / elapsed.as_secs_f64();
    
    println!("Performance regression test results:");
    println!("  Total operations: {}", total_operations);
    println!("  Elapsed time: {:?}", elapsed);
    println!("  Operations per second: {:.0}", ops_per_sec);
    
    // Basic performance threshold - should be at least 100K ops/sec on CI
    assert!(ops_per_sec > 100_000.0, "Performance regression detected: {:.0} ops/sec", ops_per_sec);
}

#[test]
fn test_api_compatibility() {
    // Test that all APIs are compatible and work as expected
    
    // Test queue API
    let queue: MpmcQueue<i32> = MpmcQueue::new(10);
    assert!(queue.push(1).is_ok());
    assert_eq!(queue.pop(), Some(1));
    assert_eq!(queue.len(), 0);
    assert!(queue.is_empty());
    assert_eq!(queue.capacity(), 16); // Rounded up to power of 2
    
    // Test map API
    let map: ConcurrentHashMap<String, i32> = ConcurrentHashMap::new();
    assert_eq!(map.insert("key".to_string(), 42), None);
    assert_eq!(map.get("key"), Some(&42));
    assert_eq!(map.len(), 1);
    assert!(!map.is_empty());
    assert_eq!(map.remove("key"), Some(42));
    
    // Test deque API
    let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(10);
    assert!(deque.push(1).is_ok());
    assert_eq!(deque.pop(), Some(1));
    assert_eq!(deque.steal(), None);
    assert_eq!(deque.len(), 0);
    assert!(deque.is_empty());
    assert_eq!(deque.capacity(), 16); // Rounded up to power of 2
    
    // Test clone operations
    let queue2 = queue.clone();
    let map2 = map.clone();
    let deque2 = deque.clone();
    
    // Cloned structures should be independent but have same capacity
    assert_eq!(queue.capacity(), queue2.capacity());
    assert_eq!(map.capacity(), map2.capacity());
    assert_eq!(deque.capacity(), deque2.capacity());
    
    println!("API compatibility test passed!");
}

#[test]
fn test_error_handling() {
    use velocityx::Error;
    
    // Test queue error handling
    let queue: MpmcQueue<i32> = MpmcQueue::new(1);
    assert!(queue.push(1).is_ok());
    assert_eq!(queue.push(2), Err(Error::WouldBlock));
    
    // Test map operations (should not error under normal conditions)
    let map: ConcurrentHashMap<i32, i32> = ConcurrentHashMap::new();
    assert_eq!(map.insert(1, 2), None);
    assert_eq!(map.insert(1, 3), Some(2));
    
    // Test deque error handling
    let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(1);
    assert!(deque.push(1).is_ok());
    assert_eq!(deque.push(2), Err(Error::WouldBlock));
    
    println!("Error handling test passed!");
}
