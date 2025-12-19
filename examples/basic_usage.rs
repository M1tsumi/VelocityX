//! Basic usage example for VelocityX
//!
//! This example demonstrates the basic usage of the MPMC queue with enhanced
//! multi-producer/multi-consumer scenarios and comprehensive error handling.

use std::sync::Arc;
use std::thread;
use std::time::Duration;
use velocityx::queue::MpmcQueue;
use velocityx::Error;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("VelocityX Enhanced Usage Example");
    println!("================================");

    // Create a bounded MPMC queue
    let queue: Arc<MpmcQueue<i32>> = Arc::new(MpmcQueue::new(1000));

    // Basic push/pop operations
    println!("\n1. Basic Operations:");
    queue.push(42)?;
    queue.push(24)?;

    let value1 = queue.pop();
    let value2 = queue.pop();

    println!("   Pushed: 42, 24");
    println!("   Popped: {:?}, {:?}", value1, value2);

    // Enhanced multi-producer scenario with error handling
    println!("\n2. Enhanced Multi-Producer:");
    let producer_handles: Vec<_> = (0..4)
        .map(|i| {
            let queue: Arc<MpmcQueue<i32>> = Arc::clone(&queue);
            thread::spawn(move || {
                let mut produced = 0;
                for j in 0..25 {
                    let value = i * 25 + j;
                    match queue.push(value) {
                        Ok(()) => {
                            produced += 1;
                            if j % 10 == 0 {
                                println!(
                                    "   Producer {} pushed {} (total: {})",
                                    i, value, produced
                                );
                            }
                        }
                        Err(Error::CapacityExceeded) => {
                            println!("   Producer {}: Queue full, retrying...", i);
                            thread::sleep(Duration::from_micros(100));
                            continue;
                        }
                        Err(e) => {
                            println!("   Producer {}: Unexpected error: {:?}", i, e);
                            break;
                        }
                    }
                }
                println!("   Producer {} finished with {} items", i, produced);
                produced
            })
        })
        .collect();

    // Wait for all producers to finish
    let mut total_produced = 0;
    for handle in producer_handles {
        total_produced += handle.join().unwrap();
    }

    // Enhanced multi-consumer scenario with coordination
    println!("\n3. Enhanced Multi-Consumer:");
    let consumer_handles: Vec<_> = (0..3)
        .map(|i| {
            let queue: Arc<MpmcQueue<i32>> = Arc::clone(&queue);
            thread::spawn(move || {
                let mut consumed = 0;
                let mut sum = 0;
                let start_time = std::time::Instant::now();

                while consumed < 34 {
                    // Approximate share
                    match queue.pop() {
                        Some(value) => {
                            consumed += 1;
                            sum += value;
                            if consumed % 10 == 0 {
                                let elapsed = start_time.elapsed();
                                println!(
                                    "   Consumer {} got {} (total: {}, sum: {}, elapsed: {:?})",
                                    i, value, consumed, sum, elapsed
                                );
                            }
                        }
                        None => {
                            // Queue empty, brief pause to avoid busy waiting
                            thread::sleep(Duration::from_micros(50));
                        }
                    }
                }
                println!(
                    "   Consumer {} finished: {} items, sum: {}",
                    i, consumed, sum
                );
                (consumed, sum)
            })
        })
        .collect();

    // Wait for all consumers to finish
    let mut total_consumed = 0;
    let mut total_sum = 0;
    for handle in consumer_handles {
        let (consumed, sum) = handle.join().unwrap();
        total_consumed += consumed;
        total_sum += sum;
    }

    println!("\n4. Results:");
    println!("   Total items produced: {}", total_produced);
    println!("   Total items consumed: {}", total_consumed);
    println!("   Queue size: {}", queue.len());
    println!("   Sum of all consumed items: {}", total_sum);

    // Demonstrate error handling scenarios
    println!("\n5. Error Handling Examples:");

    // Test capacity exceeded
    let small_queue: Arc<MpmcQueue<i32>> = Arc::new(MpmcQueue::new(2));
    assert!(small_queue.push(1).is_ok());
    assert!(small_queue.push(2).is_ok());
    match small_queue.push(3) {
        Err(Error::CapacityExceeded) => println!("   ✓ Capacity exceeded error handled correctly"),
        _ => println!("   ✗ Unexpected result"),
    }

    // Test empty queue pop
    let empty_queue: Arc<MpmcQueue<i32>> = Arc::new(MpmcQueue::new(10));
    assert_eq!(empty_queue.pop(), None);
    println!("   ✓ Empty queue pop returns None correctly");

    println!("\n✅ Enhanced usage example completed successfully!");
    Ok(())
}
