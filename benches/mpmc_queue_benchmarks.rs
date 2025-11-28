//! Comprehensive performance benchmarks for MPMC queues
//!
//! This benchmark suite compares VelocityX MPMC queues against:
//! - std::sync::mpsc (standard library channels)
//! - crossbeam::queue (SegQueue and ArrayQueue)
//! - crossbeam::channel (bounded and unbounded channels)

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::sync::{Arc, Barrier, mpsc as std_mpsc};
use std::thread;
use std::time::Duration;

// Import VelocityX queues
use velocityx::queue::mpmc::{MpmcQueue, UnboundedMpmcQueue};

// Import crossbeam queues
use crossbeam::queue::{SegQueue, ArrayQueue};
use crossbeam::channel::{bounded as crossbeam_bounded, unbounded as crossbeam_unbounded};

// Benchmark configurations
const OPERATIONS_PER_THREAD: usize = 100_000;
const MESSAGE_SIZES: &[usize] = &[8, 64, 256, 1024];
const THREAD_COUNTS: &[usize] = &[1, 2, 4, 8, 16];

// Benchmark message types
#[derive(Clone, Copy)]
struct SmallMessage {
    id: u64,
}

#[derive(Clone)]
struct MediumMessage {
    id: u64,
    data: [u8; 64],
}

#[derive(Clone)]
struct LargeMessage {
    id: u64,
    data: [u8; 1024],
}

// Helper functions to create benchmark scenarios
fn create_bounded_queue<T>(capacity: usize) -> MpmcQueue<T> {
    MpmcQueue::new(capacity)
}

fn create_unbounded_queue<T>() -> UnboundedMpmcQueue<T> {
    UnboundedMpmcQueue::new()
}

fn create_std_bounded<T>(capacity: usize) -> (std_mpsc::Sender<T>, std_mpsc::Receiver<T>) {
    std_mpsc::bounded(capacity)
}

fn create_std_unbounded<T>() -> (std_mpsc::Sender<T>, std_mpsc::Receiver<T>) {
    std_mpsc::channel()
}

fn create_crossbeam_array_queue<T>(capacity: usize) -> ArrayQueue<T> {
    ArrayQueue::new(capacity)
}

fn create_crossbeam_seg_queue<T>() -> SegQueue<T> {
    SegQueue::new()
}

fn create_crossbeam_bounded<T>(capacity: usize) -> (crossbeam::channel::Sender<T>, crossbeam::channel::Receiver<T>) {
    crossbeam_bounded(capacity)
}

fn create_crossbeam_unbounded<T>() -> (crossbeam::channel::Sender<T>, crossbeam::channel::Receiver<T>) {
    crossbeam_unbounded()
}

// Single-threaded benchmarks
fn bench_single_thread_push_pop(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_thread_push_pop");
    
    for &size in MESSAGE_SIZES {
        group.throughput(Throughput::Elements(size as u64));
        
        // VelocityX Bounded
        group.bench_with_input(
            BenchmarkId::new("velocityx_bounded", size),
            &size,
            |b, &size| {
                let queue: MpmcQueue<u64> = MpmcQueue::new(1000);
                b.iter(|| {
                    for i in 0..size {
                        black_box(queue.push(black_box(i)).unwrap());
                    }
                    for _ in 0..size {
                        black_box(queue.pop());
                    }
                })
            }
        );
        
        // VelocityX Unbounded
        group.bench_with_input(
            BenchmarkId::new("velocityx_unbounded", size),
            &size,
            |b, &size| {
                let queue: UnboundedMpmcQueue<u64> = UnboundedMpmcQueue::new();
                b.iter(|| {
                    for i in 0..size {
                        black_box(queue.push(black_box(i)).unwrap());
                    }
                    for _ in 0..size {
                        black_box(queue.pop());
                    }
                })
            }
        );
        
        // Crossbeam ArrayQueue
        group.bench_with_input(
            BenchmarkId::new("crossbeam_array", size),
            &size,
            |b, &size| {
                let queue: ArrayQueue<u64> = ArrayQueue::new(1000);
                b.iter(|| {
                    for i in 0..size {
                        black_box(queue.push(black_box(i)).unwrap());
                    }
                    for _ in 0..size {
                        black_box(queue.pop());
                    }
                })
            }
        );
        
        // Crossbeam SegQueue
        group.bench_with_input(
            BenchmarkId::new("crossbeam_seg", size),
            &size,
            |b, &size| {
                let queue: SegQueue<u64> = SegQueue::new();
                b.iter(|| {
                    for i in 0..size {
                        black_box(queue.push(black_box(i)));
                    }
                    for _ in 0..size {
                        black_box(queue.pop());
                    }
                })
            }
        );
    }
    
    group.finish();
}

// Multi-threaded benchmarks
fn bench_multi_thread_producer_consumer(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_thread_producer_consumer");
    group.measurement_time(Duration::from_secs(10));
    
    for &num_threads in THREAD_COUNTS {
        let ops_per_thread = OPERATIONS_PER_THREAD / num_threads;
        let total_ops = ops_per_thread * num_threads;
        
        group.throughput(Throughput::Elements(total_ops as u64));
        
        // VelocityX Bounded
        group.bench_with_input(
            BenchmarkId::new("velocityx_bounded", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(MpmcQueue::new(10000));
                    let barrier = Arc::new(Barrier::new(num_threads * 2));
                    let mut handles = vec![];
                    
                    // Producers
                    for producer_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..ops_per_thread {
                                let value = producer_id * ops_per_thread + i;
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
                            while received < ops_per_thread {
                                if queue.pop().is_some() {
                                    received += 1;
                                } else {
                                    thread::yield_now();
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
        
        // VelocityX Unbounded
        group.bench_with_input(
            BenchmarkId::new("velocityx_unbounded", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(UnboundedMpmcQueue::new());
                    let barrier = Arc::new(Barrier::new(num_threads * 2));
                    let mut handles = vec![];
                    
                    // Producers
                    for producer_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..ops_per_thread {
                                let value = producer_id * ops_per_thread + i;
                                queue.push(black_box(value)).unwrap();
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
                            while received < ops_per_thread {
                                if queue.pop().is_some() {
                                    received += 1;
                                } else {
                                    thread::yield_now();
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
        
        // Crossbeam ArrayQueue
        group.bench_with_input(
            BenchmarkId::new("crossbeam_array", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(ArrayQueue::new(10000));
                    let barrier = Arc::new(Barrier::new(num_threads * 2));
                    let mut handles = vec![];
                    
                    // Producers
                    for producer_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..ops_per_thread {
                                let value = producer_id * ops_per_thread + i;
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
                            while received < ops_per_thread {
                                if queue.pop().is_some() {
                                    received += 1;
                                } else {
                                    thread::yield_now();
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
        
        // Crossbeam SegQueue
        group.bench_with_input(
            BenchmarkId::new("crossbeam_seg", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(SegQueue::new());
                    let barrier = Arc::new(Barrier::new(num_threads * 2));
                    let mut handles = vec![];
                    
                    // Producers
                    for producer_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..ops_per_thread {
                                let value = producer_id * ops_per_thread + i;
                                queue.push(black_box(value));
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
                            while received < ops_per_thread {
                                if queue.pop().is_some() {
                                    received += 1;
                                } else {
                                    thread::yield_now();
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

// Contention benchmarks
fn bench_high_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("high_contention");
    group.measurement_time(Duration::from_secs(15));
    
    for &num_threads in [8, 16, 32].iter() {
        let ops_per_thread = OPERATIONS_PER_THREAD / num_threads;
        
        // VelocityX Bounded - High contention scenario
        group.bench_with_input(
            BenchmarkId::new("velocityx_bounded", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(MpmcQueue::new(100)); // Small queue to increase contention
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];
                    
                    // Mixed producer/consumer threads
                    for thread_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            
                            for i in 0..ops_per_thread {
                                let value = thread_id * ops_per_thread + i;
                                
                                // Mix of push and pop operations
                                match i % 3 {
                                    0 => {
                                        while queue.push(black_box(value)).is_err() {
                                            thread::yield_now();
                                        }
                                    }
                                    1 => {
                                        let _ = queue.pop();
                                    }
                                    2 => {
                                        if queue.push(black_box(value)).is_ok() {
                                            let _ = queue.pop();
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
                })
            }
        );
        
        // VelocityX Unbounded - High contention scenario
        group.bench_with_input(
            BenchmarkId::new("velocityx_unbounded", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(UnboundedMpmcQueue::new());
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];
                    
                    // Mixed producer/consumer threads
                    for thread_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            
                            for i in 0..ops_per_thread {
                                let value = thread_id * ops_per_thread + i;
                                
                                // Mix of push and pop operations
                                match i % 3 {
                                    0 => {
                                        queue.push(black_box(value)).unwrap();
                                    }
                                    1 => {
                                        let _ = queue.pop();
                                    }
                                    2 => {
                                        queue.push(black_box(value)).unwrap();
                                        let _ = queue.pop();
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

// Latency benchmarks
fn bench_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency");
    group.measurement_time(Duration::from_secs(5));
    
    // Single producer, single consumer latency
    group.bench_function("velocityx_bounded_1p1c", |b| {
        b.iter(|| {
            let queue: MpmcQueue<u64> = MpmcQueue::new(1000);
            
            // Producer
            let producer = thread::spawn(move || {
                for i in 0..10000 {
                    while queue.push(black_box(i)).is_err() {
                        thread::yield_now();
                    }
                }
            });
            
            // Consumer
            let consumer = thread::spawn(move || {
                let mut received = 0;
                while received < 10000 {
                    if queue.pop().is_some() {
                        received += 1;
                    }
                }
            });
            
            producer.join().unwrap();
            consumer.join().unwrap();
        })
    });
    
    group.bench_function("velocityx_unbounded_1p1c", |b| {
        b.iter(|| {
            let queue: UnboundedMpmcQueue<u64> = UnboundedMpmcQueue::new();
            
            // Producer
            let producer = thread::spawn(move || {
                for i in 0..10000 {
                    queue.push(black_box(i)).unwrap();
                }
            });
            
            // Consumer
            let consumer = thread::spawn(move || {
                let mut received = 0;
                while received < 10000 {
                    if queue.pop().is_some() {
                        received += 1;
                    }
                }
            });
            
            producer.join().unwrap();
            consumer.join().unwrap();
        })
    });
    
    group.finish();
}

// Memory usage benchmarks
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    group.bench_function("velocityx_bounded_memory", |b| {
        b.iter(|| {
            let queues: Vec<MpmcQueue<u64>> = (0..1000)
                .map(|_| MpmcQueue::new(100))
                .collect();
            black_box(queues);
        })
    });
    
    group.bench_function("velocityx_unbounded_memory", |b| {
        b.iter(|| {
            let queues: Vec<UnboundedMpmcQueue<u64>> = (0..1000)
                .map(|_| UnboundedMpmcQueue::new())
                .collect();
            black_box(queues);
        })
    });
    
    group.bench_function("crossbeam_array_memory", |b| {
        b.iter(|| {
            let queues: Vec<ArrayQueue<u64>> = (0..1000)
                .map(|_| ArrayQueue::new(100))
                .collect();
            black_box(queues);
        })
    });
    
    group.bench_function("crossbeam_seg_memory", |b| {
        b.iter(|| {
            let queues: Vec<SegQueue<u64>> = (0..1000)
                .map(|_| SegQueue::new())
                .collect();
            black_box(queues);
        })
    });
    
    group.finish();
}

// Scaling benchmarks
fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");
    group.measurement_time(Duration::from_secs(10));
    
    for &num_threads in [1, 2, 4, 8, 16, 32].iter() {
        let ops_per_thread = OPERATIONS_PER_THREAD / num_threads.max(1);
        
        // Test how throughput scales with thread count
        group.bench_with_input(
            BenchmarkId::new("velocityx_bounded_scaling", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter(|| {
                    let queue = Arc::new(MpmcQueue::new(10000));
                    let barrier = Arc::new(Barrier::new(num_threads));
                    let mut handles = vec![];
                    
                    for thread_id in 0..num_threads {
                        let queue = Arc::clone(&queue);
                        let barrier = Arc::clone(&barrier);
                        let handle = thread::spawn(move || {
                            barrier.wait();
                            for i in 0..ops_per_thread {
                                let value = thread_id * ops_per_thread + i;
                                if queue.push(black_box(value)).is_ok() {
                                    let _ = queue.pop();
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

criterion_group!(
    benches,
    bench_single_thread_push_pop,
    bench_multi_thread_producer_consumer,
    bench_high_contention,
    bench_latency,
    bench_memory_usage,
    bench_scaling
);

criterion_main!(benches);
