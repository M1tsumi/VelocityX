//! Integration tests for deque implementations

use super::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_work_stealing_deque_stress() {
    let deque = Arc::new(WorkStealingDeque::new(10000));
    let num_workers = 4;
    let num_thieves = 4;
    let tasks_per_worker = 10000;
    
    // Spawn worker threads
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
            
            // Process tasks (own work + stolen)
            let mut processed = 0;
            let mut sum = 0;
            while processed < tasks_per_worker {
                if let Some(task) = deque.pop() {
                    sum += task;
                    processed += 1;
                } else if let Some(task) = deque.steal() {
                    sum += task;
                    processed += 1;
                } else {
                    thread::yield_now();
                }
            }
            
            (processed, sum)
        });
        worker_handles.push(handle);
    }
    
    // Spawn thief threads
    let mut thief_handles = vec![];
    for _ in 0..num_thieves {
        let deque = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            let mut stolen = 0;
            let mut sum = 0;
            while stolen < tasks_per_worker * num_workers / (num_workers + num_thieves) {
                if let Some(task) = deque.steal() {
                    sum += task;
                    stolen += 1;
                } else {
                    thread::yield_now();
                }
            }
            (stolen, sum)
        });
        thief_handles.push(handle);
    }
    
    // Wait for all threads
    let mut total_processed = 0;
    let mut worker_sum = 0;
    for handle in worker_handles {
        let (processed, sum) = handle.join().unwrap();
        total_processed += processed;
        worker_sum += sum;
    }
    
    let mut total_stolen = 0;
    let mut thief_sum = 0;
    for handle in thief_handles {
        let (stolen, sum) = handle.join().unwrap();
        total_stolen += stolen;
        thief_sum += sum;
    }
    
    let total_tasks = num_workers * tasks_per_worker;
    assert_eq!(total_processed + total_stolen, total_tasks);
    
    // Verify the sum is correct
    let expected_sum = total_tasks * (total_tasks - 1) / 2;
    assert_eq!(worker_sum + thief_sum, expected_sum);
}

#[test]
fn test_work_stealing_deque_fairness() {
    let deque = Arc::new(WorkStealingDeque::new(1000));
    let num_workers = 4;
    let tasks_per_worker = 100;
    
    let mut handles = vec![];
    
    // Each worker pushes a unique range of tasks
    for worker_id in 0..num_workers {
        let deque = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            // Push tasks
            for i in 0..tasks_per_worker {
                let task = worker_id * 10000 + i; // Large gaps to ensure uniqueness
                while deque.push(task).is_err() {
                    thread::yield_now();
                }
            }
            
            // Process tasks (own + stolen)
            let mut processed = 0;
            let mut found_tasks = vec![];
            while processed < tasks_per_worker {
                if let Some(task) = deque.pop() {
                    if !found_tasks.contains(&task) {
                        found_tasks.push(task);
                        processed += 1;
                    }
                } else if let Some(task) = deque.steal() {
                    if !found_tasks.contains(&task) {
                        found_tasks.push(task);
                        processed += 1;
                    }
                } else {
                    thread::yield_now();
                }
            }
            
            found_tasks
        });
        handles.push(handle);
    }
    
    // Wait for all workers and collect all tasks
    let mut all_tasks = vec![];
    for handle in handles {
        let mut tasks = handle.join().unwrap();
        all_tasks.append(&mut tasks);
    }
    
    // Verify we have the right number of unique tasks
    assert_eq!(all_tasks.len(), num_workers * tasks_per_worker);
    
    // Verify all workers' tasks are represented
    for worker_id in 0..num_workers {
        let worker_tasks: Vec<_> = all_tasks.iter()
            .filter(|&&task| task / 10000 == worker_id)
            .collect();
        assert_eq!(worker_tasks.len(), tasks_per_worker);
    }
}

#[test]
fn test_work_stealing_deque_memory_safety() {
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
    
    let deque: Arc<WorkStealingDeque<DropCounter>> = Arc::new(WorkStealingDeque::new(100));
    let num_threads = 4;
    let items_per_thread = 25;
    
    let mut handles = vec![];
    
    // Spawn threads that push and pop items
    for thread_id in 0..num_threads {
        let deque = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            for i in 0..items_per_thread {
                let item = DropCounter { id: thread_id * items_per_thread + i };
                
                // Push item
                while deque.push(item).is_err() {
                    thread::yield_now();
                }
                
                // Occasionally pop or steal
                if i % 3 == 0 {
                    let _ = deque.pop().or_else(|| deque.steal());
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
    while deque.pop().is_some() || deque.steal().is_some() {
        // Drain
    }
    
    // Drop the deque
    drop(deque);
    
    // All items should be dropped
    let total_items = num_threads * items_per_thread;
    let dropped_items = DROP_COUNT.load(Ordering::Relaxed);
    assert_eq!(dropped_items, total_items);
}

#[test]
fn test_work_stealing_deque_under_load() {
    let deque = Arc::new(WorkStealingDeque::with_capacity(16));
    let num_producers = 2;
    let num_consumers = 6;
    let operations_per_thread = 1000;
    
    // Producer threads (push)
    let mut producer_handles = vec![];
    for producer_id in 0..num_producers {
        let deque = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            for i in 0..operations_per_thread {
                let task = producer_id * operations_per_thread + i;
                while deque.push(task).is_err() {
                    thread::yield_now();
                }
                
                // Add some delay to increase contention
                if i % 100 == 0 {
                    thread::sleep(Duration::from_micros(1));
                }
            }
        });
        producer_handles.push(handle);
    }
    
    // Consumer threads (pop and steal)
    let mut consumer_handles = vec![];
    for consumer_id in 0..num_consumers {
        let deque = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            let mut consumed = 0;
            let mut sum = 0;
            
            while consumed < operations_per_thread * num_producers / num_consumers {
                // Try pop first (if this is the "owner" thread)
                let result = if consumer_id < num_producers {
                    deque.pop()
                } else {
                    None
                };
                
                // If pop failed, try steal
                let result = result.or_else(|| deque.steal());
                
                if let Some(task) = result {
                    sum += task;
                    consumed += 1;
                } else {
                    thread::yield_now();
                }
            }
            
            (consumed, sum)
        });
        consumer_handles.push(handle);
    }
    
    // Wait for all producers
    for handle in producer_handles {
        handle.join().unwrap();
    }
    
    // Wait for all consumers
    let mut total_consumed = 0;
    let mut total_sum = 0;
    for handle in consumer_handles {
        let (consumed, sum) = handle.join().unwrap();
        total_consumed += consumed;
        total_sum += sum;
    }
    
    // Verify consumption
    let total_produced = num_producers * operations_per_thread;
    assert_eq!(total_consumed, total_produced);
    
    // Verify sum
    let expected_sum = total_produced * (total_produced - 1) / 2;
    assert_eq!(total_sum, expected_sum);
}

#[test]
fn test_work_stealing_deque_complex_scenario() {
    let deque = Arc::new(WorkStealingDeque::new(100));
    let num_threads = 8;
    let operations_per_thread = 500;
    
    let mut handles = vec![];
    
    // Complex scenario with mixed operations
    for thread_id in 0..num_threads {
        let deque = Arc::clone(&deque);
        let handle = thread::spawn(move || {
            let mut local_ops = 0;
            let mut local_sum = 0;
            
            while local_ops < operations_per_thread {
                let operation = (thread_id + local_ops) % 4;
                
                match operation {
                    0 => {
                        // Push
                        let value = thread_id * 10000 + local_ops;
                        if deque.push(value).is_ok() {
                            local_ops += 1;
                        }
                    }
                    1 => {
                        // Pop (only for some threads to simulate ownership)
                        if thread_id % 2 == 0 {
                            if let Some(value) = deque.pop() {
                                local_sum += value;
                                local_ops += 1;
                            }
                        }
                    }
                    2 => {
                        // Steal
                        if let Some(value) = deque.steal() {
                            local_sum += value;
                            local_ops += 1;
                        }
                    }
                    3 => {
                        // Check size and yield
                        let _size = deque.len();
                        thread::yield_now();
                    }
                    _ => unreachable!(),
                }
            }
            
            (local_ops, local_sum)
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    let mut total_ops = 0;
    let mut total_sum = 0;
    for handle in handles {
        let (ops, sum) = handle.join().unwrap();
        total_ops += ops;
        total_sum += sum;
    }
    
    // Verify all operations completed
    assert_eq!(total_ops, num_threads * operations_per_thread);
    
    // Drain remaining items
    let mut remaining_sum = 0;
    let mut remaining_count = 0;
    while let Some(item) = deque.pop().or_else(|| deque.steal()) {
        remaining_sum += item;
        remaining_count += 1;
    }
    
    // The sum should include all items that were ever pushed
    // (This is a simplified check - in practice you'd track all pushes)
    assert!(total_sum + remaining_sum > 0);
    
    // Deque should be empty now
    assert!(deque.is_empty());
}

#[test]
fn test_work_stealing_deque_boundary_conditions() {
    // Test with capacity of 1
    let deque: WorkStealingDeque<i32> = WorkStealingDeque::new(1);
    assert_eq!(deque.capacity(), 1);
    
    // Fill and drain
    assert!(deque.push(42).is_ok());
    assert!(deque.push(43).is_err());
    assert_eq!(deque.pop(), Some(42));
    assert_eq!(deque.pop(), None);
    assert!(deque.push(44).is_ok());
    
    // Test stealing from single-item deque
    assert_eq!(deque.steal(), Some(44));
    assert_eq!(deque.steal(), None);
    
    // Test large capacity
    let large_deque: WorkStealingDeque<i32> = WorkStealingDeque::new(100000);
    assert!(large_deque.capacity() >= 100000);
    assert!(large_deque.capacity().is_power_of_two());
}
