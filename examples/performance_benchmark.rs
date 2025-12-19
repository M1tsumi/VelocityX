//! Performance benchmark example for VelocityX
//!
//! This example demonstrates comprehensive performance characteristics of the MPMC queue
//! with multiple test scenarios and detailed metrics.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use velocityx::queue::MpmcQueue;
use velocityx::Error;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("VelocityX Comprehensive Performance Benchmark");
    println!("============================================");

    // Test 1: Basic throughput benchmark
    println!("\n📊 Test 1: Basic Throughput Benchmark");
    run_throughput_benchmark("1:1 (1 producer, 1 consumer)", 1, 1, 1_000_000, 1_000_000)?;

    run_throughput_benchmark("4:4 (4 producers, 4 consumers)", 4, 4, 250_000, 1_000_000)?;

    run_throughput_benchmark("8:8 (8 producers, 8 consumers)", 8, 8, 125_000, 1_000_000)?;

    // Test 2: Latency benchmark
    println!("\n⏱️  Test 2: Latency Benchmark");
    run_latency_benchmark()?;

    // Test 3: Contention benchmark
    println!("\n🔥 Test 3: High Contention Benchmark");
    run_contention_benchmark()?;

    // Test 4: Memory usage benchmark
    println!("\n💾 Test 4: Memory Usage Analysis");
    run_memory_benchmark()?;

    // Test 5: Error handling performance
    println!("\n⚡ Test 5: Error Handling Performance");
    run_error_handling_benchmark()?;

    println!("\n✅ All benchmarks completed successfully!");
    Ok(())
}

fn run_throughput_benchmark(
    name: &str,
    num_producers: usize,
    num_consumers: usize,
    items_per_producer: usize,
    total_items: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n  📈 {}: ", name);

    let queue_capacity = total_items.next_power_of_two();
    let queue: Arc<MpmcQueue<usize>> = Arc::new(MpmcQueue::new(queue_capacity));

    println!("    Configuration:");
    println!("      Queue capacity: {}", queue_capacity);
    println!("      Producers: {}", num_producers);
    println!("      Consumers: {}", num_consumers);
    println!("      Items per producer: {}", items_per_producer);
    println!("      Total items: {}", total_items);

    // Warm-up phase
    let warmup_items = 1000;
    for i in 0..warmup_items {
        queue.push(i)?;
    }
    while queue.pop().is_some() {}

    // Benchmark phase
    let start_time = Instant::now();

    // Start consumer threads
    let items_per_consumer = total_items / num_consumers;
    let consumer_handles: Vec<_> = (0..num_consumers)
        .map(|_| {
            let queue: Arc<MpmcQueue<usize>> = Arc::clone(&queue);
            thread::spawn(move || {
                let mut consumed = 0;
                let target = items_per_consumer;
                while consumed < target {
                    if queue.pop().is_some() {
                        consumed += 1;
                    }
                }
                consumed
            })
        })
        .collect();

    // Start producer threads
    let producer_handles: Vec<_> = (0..num_producers)
        .map(|producer_id| {
            let queue: Arc<MpmcQueue<usize>> = Arc::clone(&queue);
            thread::spawn(move || {
                for i in 0..items_per_producer {
                    let value = producer_id * items_per_producer + i;
                    loop {
                        match queue.push(value) {
                            Ok(()) => break,
                            Err(Error::CapacityExceeded) => {
                                thread::yield_now();
                            }
                            Err(_) => break,
                        }
                    }
                }
            })
        })
        .collect();

    // Wait for all producers
    for handle in producer_handles {
        handle.join().unwrap();
    }

    // Wait for all consumers
    let mut total_consumed = 0;
    for handle in consumer_handles {
        total_consumed += handle.join().unwrap();
    }

    let elapsed = start_time.elapsed();
    let throughput = total_consumed as f64 / elapsed.as_secs_f64();
    let avg_latency = elapsed.as_nanos() as f64 / total_consumed as f64;

    println!("    Results:");
    println!("      Total items processed: {}", total_consumed);
    println!("      Time elapsed: {:?}", elapsed);
    println!("      Throughput: {:.2} ops/sec", throughput);
    println!("      Average latency: {:.2} ns/op", avg_latency);
    println!(
        "      Memory usage: {} MB",
        queue_capacity * std::mem::size_of::<usize>() / 1_048_576
    );

    Ok(())
}

fn run_latency_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let queue: Arc<MpmcQueue<Instant>> = Arc::new(MpmcQueue::new(10_000));
    let num_samples = 10_000;

    println!(
        "    Measuring end-to-end latency with {} samples...",
        num_samples
    );

    let producer_handle = thread::spawn({
        let queue = Arc::clone(&queue);
        move || {
            for _ in 0..num_samples {
                let timestamp = Instant::now();
                while queue.push(timestamp).is_err() {
                    thread::yield_now();
                }
            }
        }
    });

    let consumer_handle = thread::spawn({
        let queue = Arc::clone(&queue);
        move || {
            let mut latencies = Vec::with_capacity(num_samples);
            while latencies.len() < num_samples {
                if let Some(timestamp) = queue.pop() {
                    let latency = timestamp.elapsed();
                    latencies.push(latency);
                }
            }
            latencies
        }
    });

    producer_handle.join().unwrap();
    let mut latencies = consumer_handle.join().unwrap();

    // Calculate statistics
    latencies.sort();
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];
    let mean = latencies.iter().sum::<Duration>().as_nanos() as f64 / latencies.len() as f64;
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99 = latencies[(latencies.len() as f64 * 0.99) as usize];

    println!("    Latency Statistics:");
    println!("      Min: {:.2} ns", min.as_nanos() as f64);
    println!("      50th percentile: {:.2} ns", p50.as_nanos() as f64);
    println!("      95th percentile: {:.2} ns", p95.as_nanos() as f64);
    println!("      99th percentile: {:.2} ns", p99.as_nanos() as f64);
    println!("      Max: {:.2} ns", max.as_nanos() as f64);
    println!("      Mean: {:.2} ns", mean);

    Ok(())
}

fn run_contention_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let queue: Arc<MpmcQueue<usize>> = Arc::new(MpmcQueue::new(100));
    let num_threads = 16;
    let operations_per_thread = 10_000;

    println!(
        "    High contention test: {} threads, {} ops each",
        num_threads, operations_per_thread
    );

    let counter = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let queue = Arc::clone(&queue);
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                let mut ops = 0;
                while ops < operations_per_thread {
                    if ops % 2 == 0 {
                        // Producer operation
                        let value = thread_id * operations_per_thread + ops;
                        match queue.push(value) {
                            Ok(()) => ops += 1,
                            Err(Error::CapacityExceeded) => {
                                thread::yield_now();
                            }
                            Err(_) => break,
                        }
                    } else {
                        // Consumer operation
                        if queue.pop().is_some() {
                            ops += 1;
                        } else {
                            thread::yield_now();
                        }
                    }
                }
                counter.fetch_add(ops, Ordering::Relaxed);
                ops
            })
        })
        .collect();

    let start_time = Instant::now();
    for handle in handles {
        handle.join().unwrap();
    }
    let elapsed = start_time.elapsed();
    let total_ops = counter.load(Ordering::Relaxed);

    println!("    Contention Results:");
    println!("      Total operations: {}", total_ops);
    println!("      Time elapsed: {:?}", elapsed);
    println!(
        "      Throughput: {:.2} ops/sec",
        total_ops as f64 / elapsed.as_secs_f64()
    );
    println!("      Queue size: {}", queue.len());

    Ok(())
}

fn run_memory_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let capacities = vec![16, 256, 4096, 65536];

    for capacity in capacities {
        let queue: Arc<MpmcQueue<usize>> = Arc::new(MpmcQueue::new(capacity));

        // Fill the queue
        for i in 0..capacity {
            queue.push(i)?;
        }

        let memory_usage = capacity * std::mem::size_of::<usize>();
        let padded_usage = capacity * std::mem::size_of::<velocityx::util::CachePadded<usize>>();

        println!(
            "    Capacity {}: {} bytes ({:.2} MB) buffer, {:.2} MB with padding",
            capacity,
            memory_usage,
            memory_usage as f64 / 1_048_576.0,
            padded_usage as f64 / 1_048_576.0
        );

        // Drain the queue
        while queue.pop().is_some() {}
    }

    Ok(())
}

fn run_error_handling_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let small_queue: Arc<MpmcQueue<usize>> = Arc::new(MpmcQueue::new(1));
    let num_error_ops = 1_000_000;

    println!(
        "    Testing error handling performance with {} error operations...",
        num_error_ops
    );

    // Fill the queue
    small_queue.push(42)?;

    let start_time = Instant::now();
    let mut error_count = 0;

    for _ in 0..num_error_ops {
        match small_queue.push(123) {
            Err(Error::CapacityExceeded) => error_count += 1,
            _ => break,
        }
    }

    let elapsed = start_time.elapsed();
    let error_throughput = error_count as f64 / elapsed.as_secs_f64();

    println!("    Error Handling Results:");
    println!("      Error operations: {}", error_count);
    println!("      Time elapsed: {:?}", elapsed);
    println!("      Error throughput: {:.2} errors/sec", error_throughput);
    println!(
        "      Average error latency: {:.2} ns/error",
        elapsed.as_nanos() as f64 / error_count as f64
    );

    Ok(())
}
