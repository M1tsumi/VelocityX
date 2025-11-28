//! Integration tests for map implementations

use super::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_concurrent_hash_map_stress() {
    let map = Arc::new(ConcurrentHashMap::new());
    let num_threads = 8;
    let operations_per_thread = 10000;
    
    let mut handles = vec![];
    
    // Spawn threads that perform mixed operations
    for thread_id in 0..num_threads {
        let map = Arc::clone(&map);
        let handle = thread::spawn(move || {
            let mut local_sum = 0;
            
            for i in 0..operations_per_thread {
                let key = thread_id * operations_per_thread + i;
                
                // Insert
                map.insert(key, key * 2);
                
                // Read
                if let Some(value) = map.get(&key) {
                    local_sum += *value;
                }
                
                // Occasionally remove and re-insert
                if i % 100 == 0 {
                    map.remove(&key);
                    map.insert(key, key * 3);
                }
            }
            
            local_sum
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    let mut total_sum = 0;
    for handle in handles {
        total_sum += handle.join().unwrap();
    }
    
    // Verify final state
    let mut expected_sum = 0;
    for thread_id in 0..num_threads {
        for i in 0..operations_per_thread {
            let key = thread_id * operations_per_thread + i;
            let value = if i % 100 == 0 { key * 3 } else { key * 2 };
            expected_sum += value;
        }
    }
    
    assert_eq!(total_sum, expected_sum);
}

#[test]
fn test_concurrent_hash_map_resize_under_load() {
    let map = Arc::new(ConcurrentHashMap::with_capacity(16));
    let num_producers = 4;
    let num_consumers = 4;
    let items_per_producer = 1000;
    
    // Producer threads
    let mut producer_handles = vec![];
    for producer_id in 0..num_producers {
        let map = Arc::clone(&map);
        let handle = thread::spawn(move || {
            for i in 0..items_per_producer {
                let key = producer_id * items_per_producer + i;
                map.insert(key, format!("value_{}_{}", producer_id, i));
                
                // Add some delay to increase contention
                if i % 100 == 0 {
                    thread::sleep(Duration::from_micros(1));
                }
            }
        });
        producer_handles.push(handle);
    }
    
    // Consumer threads
    let mut consumer_handles = vec![];
    for _ in 0..num_consumers {
        let map = Arc::clone(&map);
        let handle = thread::spawn(move || {
            let mut count = 0;
            let mut found_keys = vec![];
            
            while count < items_per_producer * num_producers / num_consumers {
                // Try to read keys
                for key in 0..items_per_producer * num_producers {
                    if let Some(_value) = map.get(&key) {
                        if !found_keys.contains(&key) {
                            found_keys.push(key);
                            count += 1;
                        }
                    }
                }
                
                if found_keys.len() == count {
                    thread::yield_now();
                }
            }
            
            count
        });
        consumer_handles.push(handle);
    }
    
    // Wait for all threads
    for handle in producer_handles {
        handle.join().unwrap();
    }
    
    let mut total_consumed = 0;
    for handle in consumer_handles {
        total_consumed += handle.join().unwrap();
    }
    
    // Verify all items are accessible
    for key in 0..num_producers * items_per_producer {
        assert!(map.get(&key).is_some(), "Missing key: {}", key);
    }
    
    // Should have resized due to load
    assert!(map.capacity() > 16);
}

#[test]
fn test_concurrent_hash_map_memory_safety() {
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
    
    let map: Arc<ConcurrentHashMap<DropTracker, String>> = Arc::new(ConcurrentHashMap::new());
    let num_threads = 4;
    let items_per_thread = 100;
    
    let mut handles = vec![];
    
    // Spawn threads that insert and remove items
    for thread_id in 0..num_threads {
        let map = Arc::clone(&map);
        let handle = thread::spawn(move || {
            for i in 0..items_per_thread {
                let key = DropTracker { id: thread_id * items_per_thread + i };
                let value = format!("value_{}", key.id);
                
                map.insert(key, value);
                
                // Remove every other item
                if i % 2 == 1 {
                    let key = DropTracker { id: thread_id * items_per_thread + (i - 1) };
                    map.remove(&key);
                }
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Drop the map
    drop(map);
    
    // All items should be dropped
    let total_items = num_threads * items_per_thread;
    let dropped_items = DROP_COUNT.load(Ordering::Relaxed);
    assert!(dropped_items >= total_items / 2); // At least half were dropped during removal
}

#[test]
fn test_concurrent_hash_map_fairness() {
    let map = Arc::new(ConcurrentHashMap::new());
    let num_threads = 8;
    let items_per_thread = 100;
    
    let mut handles = vec![];
    
    // Each thread inserts a unique range of keys
    for thread_id in 0..num_threads {
        let map = Arc::clone(&map);
        let handle = thread::spawn(move || {
            for i in 0..items_per_thread {
                let key = thread_id * 10000 + i; // Large gaps to ensure uniqueness
                let value = format!("thread_{}_item_{}", thread_id, i);
                map.insert(key, value);
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Verify all threads' items are present
    for thread_id in 0..num_threads {
        for i in 0..items_per_thread {
            let key = thread_id * 10000 + i;
            let expected_value = format!("thread_{}_item_{}", thread_id, i);
            assert_eq!(map.get(&key), Some(&expected_value));
        }
    }
    
    assert_eq!(map.len(), num_threads * items_per_thread);
}

#[test]
fn test_concurrent_hash_map_high_contention() {
    let map = Arc::new(ConcurrentHashMap::new());
    let num_threads = 16;
    let operations_per_thread = 1000;
    
    let mut handles = vec![];
    
    // All threads work on the same small set of keys to maximize contention
    for thread_id in 0..num_threads {
        let map = Arc::clone(&map);
        let handle = thread::spawn(move || {
            for i in 0..operations_per_thread {
                let key = i % 10; // Only 10 different keys
                let value = format!("thread_{}_op_{}", thread_id, i);
                
                // Mixed operations
                match i % 3 {
                    0 => map.insert(key, value),
                    1 => map.get(&key),
                    2 => map.remove(&key),
                    _ => unreachable!(),
                };
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Verify map is still functional
    for key in 0..10 {
        map.insert(key, format!("final_value_{}", key));
        assert!(map.get(&key).is_some());
    }
}

#[test]
fn test_concurrent_hash_map_complex_keys() {
    use std::collections::HashMap;
    
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct ComplexKey {
        id: u64,
        name: String,
        tags: Vec<u32>,
    }
    
    let map: Arc<ConcurrentHashMap<ComplexKey, Vec<String>>> = Arc::new(ConcurrentHashMap::new());
    let num_threads = 4;
    let items_per_thread = 100;
    
    let mut handles = vec![];
    
    for thread_id in 0..num_threads {
        let map = Arc::clone(&map);
        let handle = thread::spawn(move || {
            for i in 0..items_per_thread {
                let key = ComplexKey {
                    id: (thread_id * items_per_thread + i) as u64,
                    name: format!("item_{}", i),
                    tags: vec![i as u32, (i + 1) as u32],
                };
                
                let value = vec![
                    format!("thread_{}", thread_id),
                    format!("item_{}", i),
                    format!("data_{}", i * 2),
                ];
                
                map.insert(key.clone(), value.clone());
                
                // Verify insertion
                assert_eq!(map.get(&key), Some(&value));
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Verify all items are present
    for thread_id in 0..num_threads {
        for i in 0..items_per_thread {
            let key = ComplexKey {
                id: (thread_id * items_per_thread + i) as u64,
                name: format!("item_{}", i),
                tags: vec![i as u32, (i + 1) as u32],
            };
            
            assert!(map.get(&key).is_some(), "Missing complex key: {:?}", key);
        }
    }
}
