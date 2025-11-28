//! Performance benchmarks for VelocityX data structures
//!
//! This benchmark suite compares VelocityX implementations against standard library
//! alternatives and other popular concurrent data structure libraries.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

// Import VelocityX data structures
use velocityx::queue::MpmcQueue;
use velocityx::map::ConcurrentHashMap;
use velocityx::deque::WorkStealingDeque;

// Import standard library alternatives
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::collections::HashMap;
use std::sync::RwLock;

// Import crossbeam for comparison
use crossbeam::queue::SegQueue;
use crossbeam::deque::{Injector, Stealer};

// Benchmark configurations
const SMALL_QUEUE_SIZE: usize = 100;
const MEDIUM_QUEUE_SIZE: usize = 1_000;
const LARGE_QUEUE_SIZE: usize = 10_000;

const OPERATIONS_PER_THREAD: usize = 100_000;
const NUM_THREADS: usize = 4;

// MPMC Queue Benchmarks

fn bench_mpmc_queue_single_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpmc_queue_single_thread");
    
    for size in [SMALL_QUEUE_SIZE, MEDIUM_QUEUE_SIZE, LARGE_QUEUE_SIZE].iter() {
        group.bench_with_input(BenchmarkId::new("velocityx_push", size), size, |b, &size| {
            b.iter(|| {
                let queue = MpmcQueue::new(size);
                for i in 0..size {
                    black_box(queue.push(black_box(i)).unwrap());
                }
            })
        });
        
        group.bench_with_input(BenchmarkId::new("velocityx_pop", size), size, |b, &size| {
            b.iter(|| {
                let queue = MpmcQueue::new(size);
                for i in 0..size {
                    queue.push(i).unwrap();
                }
                for _ in 0..size {
                    black_box(queue.pop());
                }
            })
        });
        
        group.bench_with_input(BenchmarkId::new("std_channel_push", size), size, |b, &size| {
            b.iter(|| {
                let (tx, _rx) = channel();
                for i in 0..size {
                    black_box(tx.send(black_box(i)).unwrap());
                }
            })
        });
        
        group.bench_with_input(BenchmarkId::new("crossbeam_segqueue_push", size), size, |b, &size| {
            b.iter(|| {
                let queue = SegQueue::new();
                for i in 0..size {
                    black_box(queue.push(black_box(i)));
                }
            })
        });
    }
    
    group.finish();
}

fn bench_mpmc_queue_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpmc_queue_concurrent");
    group.measurement_time(Duration::from_secs(10));
    
    for &num_threads in [2, 4, 8].iter() {
        let operations_per_thread = OPERATIONS_PER_THREAD / num_threads;
        
        group.bench_with_input(
            BenchmarkId::new("velocityx", num_threads), 
            &num_threads, 
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(MpmcQueue::new(10_000));
                    let barrier = Arc::new(Barrier::new(num_threads * 2));
                    let mut handles = vec![];
                    
                    // Producers
                    for producer_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..operations_per_thread {
                                let value = producer_id * operations_per_thread + i;
                                while queue.push(black_box(value)).is_err() {
                                    thread::yield_now();
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    // Consumers
                    for _ in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            let mut received = 0;
                            while received < operations_per_thread {
                                if queue.pop().is_some() {
                                    received += 1;
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            }
        );
        
        group.bench_with_input(
            BenchmarkId::new("std_channel", num_threads), 
            &num_threads, 
            |b, &num_threads| {
                b.iter(|| {
                    let (tx, rx) = channel();
                    let tx = Arc::new(Mutex::new(tx));
                    let rx = Arc::new(Mutex::new(rx));
                    let barrier = Arc::new(Barrier::new(num_threads * 2));
                    let mut handles = vec![];
                    
                    // Producers
                    for producer_id in 0..num_threads {
                        let tx = Arc::clone(&tx);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..operations_per_thread {
                                let value = producer_id * operations_per_thread + i;
                                loop {
                                    match tx.lock().unwrap().send(black_box(value)) {
                                        Ok(_) => break,
                                        Err(_) => thread::yield_now(),
                                    }
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    // Consumers
                    for _ in 0..num_threads {
                        let rx = Arc::clone(&rx);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            let mut received = 0;
                            while received < operations_per_thread {
                                match rx.lock().unwrap().try_recv() {
                                    Ok(_) => received += 1,
                                    Err(_) => thread::yield_now(),
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            }
        );
    }
    
    group.finish();
}

// Concurrent HashMap Benchmarks

fn bench_concurrent_hashmap_single_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_hashmap_single_thread");
    
    for size in [100, 1_000, 10_000].iter() {
        group.bench_with_input(BenchmarkId::new("velocityx_insert", size), size, |b, &size| {
            b.iter(|| {
                let map = ConcurrentHashMap::new();
                for i in 0..size {
                    black_box(map.insert(black_box(i), black_box(i * 2)));
                }
            })
        });
        
        group.bench_with_input(BenchmarkId::new("velocityx_get", size), size, |b, &size| {
            b.iter(|| {
                let map = ConcurrentHashMap::new();
                for i in 0..size {
                    map.insert(i, i * 2);
                }
                for i in 0..size {
                    black_box(map.get(&i));
                }
            })
        });
        
        group.bench_with_input(BenchmarkId::new("std_hashmap_insert", size), size, |b, &size| {
            b.iter(|| {
                let mut map = HashMap::new();
                for i in 0..size {
                    black_box(map.insert(black_box(i), black_box(i * 2)));
                }
            })
        });
        
        group.bench_with_input(BenchmarkId::new("std_hashmap_get", size), size, |b, &size| {
            b.iter(|| {
                let mut map = HashMap::new();
                for i in 0..size {
                    map.insert(i, i * 2);
                }
                for i in 0..size {
                    black_box(map.get(&i));
                }
            })
        });
        
        group.bench_with_input(BenchmarkId::new("rwlock_hashmap_get", size), size, |b, &size| {
            b.iter(|| {
                let map = Arc::new(RwLock::new(HashMap::new()));
                for i in 0..size {
                    map.write().unwrap().insert(i, i * 2);
                }
                for i in 0..size {
                    black_box(map.read().unwrap().get(&i));
                }
            })
        });
    }
    
    group.finish();
}

fn bench_concurrent_hashmap_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_hashmap_concurrent");
    group.measurement_time(Duration::from_secs(10));
    
    for &num_threads in [2, 4, 8].iter() {
        let operations_per_thread = OPERATIONS_PER_THREAD / num_threads;
        
        group.bench_with_input(
            BenchmarkId::new("velocityx_mixed", num_threads), 
            &num_threads, 
            |b, &num_threads| {
                b.iter(|| {
                    let map = Arc::new(ConcurrentHashMap::new());
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];
                    
                    for thread_id in 0..num_threads {
                        let map = Arc::clone(&map);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..operations_per_thread {
                                let key = thread_id * operations_per_thread + i;
                                match i % 3 {
                                    0 => {
                                        // Insert
                                        black_box(map.insert(key, key * 2));
                                    }
                                    1 => {
                                        // Get
                                        black_box(map.get(&key));
                                    }
                                    2 => {
                                        // Remove
                                        black_box(map.remove(&key));
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
                })
            }
        );
        
        group.bench_with_input(
            BenchmarkId::new("rwlock_hashmap_mixed", num_threads), 
            &num_threads, 
            |b, &num_threads| {
                b.iter(|| {
                    let map = Arc::new(RwLock::new(HashMap::new()));
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];
                    
                    for thread_id in 0..num_threads {
                        let map = Arc::clone(&map);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..operations_per_thread {
                                let key = thread_id * operations_per_thread + i;
                                match i % 3 {
                                    0 => {
                                        // Insert
                                        black_box(map.write().unwrap().insert(key, key * 2));
                                    }
                                    1 => {
                                        // Get
                                        black_box(map.read().unwrap().get(&key));
                                    }
                                    2 => {
                                        // Remove
                                        black_box(map.write().unwrap().remove(&key));
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
                })
            }
        );
    }
    
    group.finish();
}

// Work-Stealing Deque Benchmarks

fn bench_work_stealing_deque(c: &mut Criterion) {
    let mut group = c.benchmark_group("work_stealing_deque");
    group.measurement_time(Duration::from_secs(10));
    
    for &num_threads in [2, 4, 8].iter() {
        let tasks_per_thread = OPERATIONS_PER_THREAD / num_threads;
        
        group.bench_with_input(
            BenchmarkId::new("velocityx", num_threads), 
            &num_threads, 
            |b, &num_threads| {
                b.iter(|| {
                    let deques: Vec<Arc<WorkStealingDeque<usize>>> = (0..num_threads)
                        .map(|_| Arc::new(WorkStealingDeque::new(10_000)))
                        .collect();
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];
                    
                    for worker_id in 0..num_threads {
                        let worker_deque = Arc::clone(&deques[worker_id]);
                        let steal_targets: Vec<Arc<WorkStealingDeque<usize>>> = deques
                            .iter()
                            .enumerate()
                            .filter(|&(i, _)| i != worker_id)
                            .map(|(_, deque)| Arc::clone(deque))
                            .collect();
                        let barrier = Arc::clone(&barrier);
                        
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            
                            // Push initial tasks
                            for i in 0..tasks_per_thread {
                                let task = worker_id * tasks_per_thread + i;
                                while worker_deque.push(black_box(task)).is_err() {
                                    thread::yield_now();
                                }
                            }
                            
                            // Process tasks (own + stolen)
                            let mut processed = 0;
                            while processed < tasks_per_thread {
                                if worker_deque.pop().is_some() {
                                    processed += 1;
                                } else {
                                    // Try to steal
                                    let mut stolen = false;
                                    for target in &steal_targets {
                                        if target.steal().is_some() {
                                            processed += 1;
                                            stolen = true;
                                            break;
                                        }
                                    }
                                    if !stolen {
                                        thread::yield_now();
                                    }
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            }
        );
        
        group.bench_with_input(
            BenchmarkId::new("crossbeam", num_threads), 
            &num_threads, 
            |b, &num_threads| {
                b.iter(|| {
                    let deques: Vec<Arc<Injector<usize>>> = (0..num_threads)
                        .map(|_| Arc::new(Injector::new()))
                        .collect();
                    let stealers: Vec<Arc<Stealer<usize>>> = deques
                        .iter()
                        .map(|deque| Arc::new(deque.stealer()))
                        .collect();
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];
                    
                    for worker_id in 0..num_threads {
                        let worker_deque = Arc::clone(&deques[worker_id]);
                        let steal_targets: Vec<Arc<Stealer<usize>>> = stealers
                            .iter()
                            .enumerate()
                            .filter(|&(i, _)| i != worker_id)
                            .map(|(_, stealer)| Arc::clone(stealer))
                            .collect();
                        let barrier = Arc::clone(&barrier);
                        
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            
                            // Push initial tasks
                            for i in 0..tasks_per_thread {
                                let task = worker_id * tasks_per_thread + i;
                                worker_deque.push(black_box(task));
                            }
                            
                            // Process tasks (own + stolen)
                            let mut processed = 0;
                            while processed < tasks_per_thread {
                                if worker_deque.steal().is_some() {
                                    processed += 1;
                                } else {
                                    // Try to steal
                                    let mut stolen = false;
                                    for target in &steal_targets {
                                        if target.steal().is_some() {
                                            processed += 1;
                                            stolen = true;
                                            break;
                                        }
                                    }
                                    if !stolen {
                                        thread::yield_now();
                                    }
                                }
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.join().unwrap();
                    }
                })
            }
        );
    }
    
    group.finish();
}

// Memory Usage Benchmarks

fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    group.bench_function("velocityx_mpmc_queue_memory", |b| {
        b.iter(|| {
            let queues: Vec<MpmcQueue<usize>> = (0..1000)
                .map(|_| MpmcQueue::new(100))
                .collect();
            black_box(queues);
        })
    });
    
    group.bench_function("std_channel_memory", |b| {
        b.iter(|| {
            let channels: Vec<(Sender<usize>, Receiver<usize>)> = (0..1000)
                .map(|_| channel())
                .collect();
            black_box(channels);
        })
    });
    
    group.bench_function("velocityx_hashmap_memory", |b| {
        b.iter(|| {
            let maps: Vec<ConcurrentHashMap<usize, usize>> = (0..1000)
                .map(|_| ConcurrentHashMap::new())
                .collect();
            black_box(maps);
        })
    });
    
    group.bench_function("std_hashmap_memory", |b| {
        b.iter(|| {
            let maps: Vec<HashMap<usize, usize>> = (0..1000)
                .map(|_| HashMap::new())
                .collect();
            black_box(maps);
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_mpmc_queue_single_thread,
    bench_mpmc_queue_concurrent,
    bench_concurrent_hashmap_single_thread,
    bench_concurrent_hashmap_concurrent,
    bench_work_stealing_deque,
    bench_memory_usage
);

criterion_main!(benches);
