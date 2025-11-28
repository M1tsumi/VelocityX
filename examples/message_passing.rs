//! High-throughput message passing example
//!
//! This example demonstrates using MPMC queues for high-performance message passing
//! between multiple producer and consumer threads, simulating a distributed system
//! event processing pipeline.

use velocityx::queue::MpmcQueue;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
struct Message {
    id: u64,
    payload: String,
    timestamp: u64,
    source: String,
}

impl Message {
    fn new(id: u64, payload: String, source: String) -> Self {
        Self {
            id,
            payload,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            source,
        }
    }
}

fn main() {
    println!("🚀 High-Throughput Message Passing Example");
    println!("==========================================");

    // Configuration
    let num_producers = 4;
    let num_consumers = 4;
    let messages_per_producer = 100_000;
    let queue_capacity = 10_000;

    // Create shared queue
    let queue = Arc::new(MpmcQueue::new(queue_capacity));
    
    // Statistics tracking
    let stats = Arc::new(Stats::new());

    println!("Configuration:");
    println!("  Producers: {}", num_producers);
    println!("  Consumers: {}", num_consumers);
    println!("  Messages per producer: {}", messages_per_producer);
    println!("  Queue capacity: {}", queue_capacity);
    println!("  Total messages: {}\n", num_producers * messages_per_producer);

    let start_time = Instant::now();

    // Spawn producer threads
    let mut producer_handles = vec![];
    for producer_id in 0..num_producers {
        let queue = Arc::clone(&queue);
        let stats = Arc::clone(&stats);
        let handle = thread::spawn(move || {
            let mut sent = 0;
            let mut blocked = 0;
            
            for i in 0..messages_per_producer {
                let message = Message::new(
                    (producer_id * messages_per_producer + i) as u64,
                    format!("Data from producer {} - message {}", producer_id, i),
                    format!("producer-{}", producer_id),
                );

                match queue.push(message) {
                    Ok(()) => sent += 1,
                    Err(_) => {
                        blocked += 1;
                        i -= 1; // Retry this message
                        thread::yield_now();
                    }
                }

                // Occasionally yield to increase contention
                if i % 1000 == 0 {
                    thread::yield_now();
                }
            }

            stats.add_producer_stats(producer_id, sent, blocked);
            (sent, blocked)
        });
        producer_handles.push(handle);
    }

    // Spawn consumer threads
    let mut consumer_handles = vec![];
    for consumer_id in 0..num_consumers {
        let queue = Arc::clone(&queue);
        let stats = Arc::clone(&stats);
        let handle = thread::spawn(move || {
            let mut received = 0;
            let mut empty_retries = 0;
            let mut total_size = 0;
            
            while received < messages_per_producer * num_producers / num_consumers {
                match queue.pop() {
                    Some(message) => {
                        received += 1;
                        total_size += message.payload.len();
                        
                        // Simulate processing
                        if message.id % 1000 == 0 {
                            thread::sleep(Duration::from_micros(1));
                        }
                    }
                    None => {
                        empty_retries += 1;
                        if empty_retries % 1000 == 0 {
                            thread::yield_now();
                        }
                    }
                }
            }

            stats.add_consumer_stats(consumer_id, received, empty_retries, total_size);
            (received, empty_retries, total_size)
        });
        consumer_handles.push(handle);
    }

    // Wait for all producers
    let mut total_sent = 0;
    let mut total_blocked = 0;
    for (i, handle) in producer_handles.into_iter().enumerate() {
        let (sent, blocked) = handle.join().unwrap();
        total_sent += sent;
        total_blocked += blocked;
        println!("Producer {} completed: {} sent, {} blocked", i, sent, blocked);
    }

    // Wait for all consumers
    let mut total_received = 0;
    let mut total_empty_retries = 0;
    let mut total_size = 0;
    for (i, handle) in consumer_handles.into_iter().enumerate() {
        let (received, empty_retries, size) = handle.join().unwrap();
        total_received += received;
        total_empty_retries += empty_retries;
        total_size += size;
        println!("Consumer {} completed: {} received, {} empty retries", i, received, empty_retries);
    }

    let elapsed = start_time.elapsed();

    // Final statistics
    println!("\n📊 Results:");
    println!("  Total sent: {}", total_sent);
    println!("  Total received: {}", total_received);
    println!("  Total blocked: {}", total_blocked);
    println!("  Total empty retries: {}", total_empty_retries);
    println!("  Total data processed: {} bytes", total_size);
    println!("  Elapsed time: {:?}", elapsed);
    println!("  Throughput: {:.2} messages/sec", total_sent as f64 / elapsed.as_secs_f64());
    println!("  Average message size: {:.2} bytes", total_size as f64 / total_received as f64);

    // Verify all messages were processed
    assert_eq!(total_sent, total_received, "Message count mismatch!");
    assert_eq!(total_sent, num_producers * messages_per_producer, "Expected message count mismatch!");

    println!("\n✅ All messages processed successfully!");
}

#[derive(Debug)]
struct Stats {
    producer_stats: std::sync::Mutex<Vec<(usize, usize, usize)>>,
    consumer_stats: std::sync::Mutex<Vec<(usize, usize, usize, usize)>>,
}

impl Stats {
    fn new() -> Self {
        Self {
            producer_stats: std::sync::Mutex::new(Vec::new()),
            consumer_stats: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn add_producer_stats(&self, producer_id: usize, sent: usize, blocked: usize) {
        if let Ok(mut stats) = self.producer_stats.lock() {
            stats.push((producer_id, sent, blocked));
        }
    }

    fn add_consumer_stats(&self, consumer_id: usize, received: usize, empty_retries: usize, size: usize) {
        if let Ok(mut stats) = self.consumer_stats.lock() {
            stats.push((consumer_id, received, empty_retries, size));
        }
    }
}
