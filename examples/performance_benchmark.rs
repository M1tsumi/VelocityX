//! Performance benchmark example for VelocityX
//! 
//! This example demonstrates performance characteristics of the MPMC queue

use velocityx::queue::MpmcQueue;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("VelocityX Performance Benchmark");
    println!("==============================");
    
    // Configuration
    let queue_capacity = 1_000_000;
    let num_producers = 4;
    let num_consumers = 4;
    let items_per_producer = 250_000;
    let total_items = num_producers * items_per_producer;
    
    println!("Configuration:");
    println!("  Queue capacity: {}", queue_capacity);
    println!("  Producers: {}", num_producers);
    println!("  Consumers: {}", num_consumers);
    println!("  Items per producer: {}", items_per_producer);
    println!("  Total items: {}", total_items);
    
    // Create the queue
    let queue: Arc<MpmcQueue<usize>> = Arc::new(MpmcQueue::new(queue_capacity));
    
    // Benchmark
    let start_time = Instant::now();
    
    // Start consumer threads
    let consumer_handles: Vec<_> = (0..num_consumers)
        .map(|_| {
            let queue: Arc<MpmcQueue<usize>> = Arc::clone(&queue);
            thread::spawn(move || {
                let mut consumed = 0;
                while consumed < items_per_producer * num_producers / num_consumers {
                    if let Some(_) = queue.pop() {
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
                    queue.push(value).unwrap();
                }
            })
        })
        .collect();
    
    // Wait for all producers to finish
    for handle in producer_handles {
        handle.join().unwrap();
    }
    
    // Wait for all consumers to finish
    let mut total_consumed = 0;
    for handle in consumer_handles {
        total_consumed += handle.join().unwrap();
    }
    
    let elapsed = start_time.elapsed();
    
    // Results
    println!("\nBenchmark Results:");
    println!("  Total items processed: {}", total_consumed);
    println!("  Time elapsed: {:?}", elapsed);
    println!("  Throughput: {:.2} ops/sec", total_consumed as f64 / elapsed.as_secs_f64());
    println!("  Average latency: {:.2} ns/op", elapsed.as_nanos() as f64 / total_consumed as f64);
    
    // Memory usage
    let memory_usage = queue_capacity * std::mem::size_of::<usize>();
    println!("  Memory usage: {} bytes ({:.2} MB)", memory_usage, memory_usage as f64 / 1_048_576.0);
    
    println!("\n✅ Performance benchmark completed successfully!");
    Ok(())
}
